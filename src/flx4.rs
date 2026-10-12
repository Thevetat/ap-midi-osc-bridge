//! DDJ-FLX4 MIDI protocol capability. MIDI decoding lives here; AP assignments live in
//! `profiles/flx4.json`; its pads use the legacy version-1 layout schema.
use crate::{
    ap::{FeedbackKind, SharedAp},
    device_profile::DeviceProfile,
    mapping::Player,
    profile::{Action, Profile, SlipTiming},
    runtime::Device,
};
use anyhow::{anyhow, bail, Context, Result};
use midir::MidiOutputConnection;
use rosc::OscType;
use serde::{de::Error as _, Deserialize, Deserializer};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

type Key = (usize, u8, u8); // input port, wire channel, canonical physical note
const KEEPALIVE: [u8; 12] = [
    0xf0, 0x00, 0x40, 0x05, 0x00, 0x00, 0x04, 0x05, 0x00, 0x50, 0x02, 0xf7,
];

// Flatten + deny_unknown_fields is unsupported by serde. Decode the shared identity
// separately, then let the tagged behavior reject every unrecognized assignment key.
#[derive(Clone)]
struct SideButton {
    channel: u8,
    note: u8,
    side: Option<usize>,
    behavior: SideBehavior,
}
impl<'de> Deserialize<'de> for SideButton {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let mut value = serde_json::Value::deserialize(deserializer)?;
        let obj = value
            .as_object_mut()
            .ok_or_else(|| D::Error::custom("FLX4 button must be an object"))?;
        let channel = obj
            .remove("channel")
            .ok_or_else(|| D::Error::missing_field("channel"))?;
        let note = obj
            .remove("note")
            .ok_or_else(|| D::Error::missing_field("note"))?;
        let side = obj.remove("side");
        let channel = serde_json::from_value(channel).map_err(D::Error::custom)?;
        let note = serde_json::from_value(note).map_err(D::Error::custom)?;
        let side = side
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?;
        let behavior = serde_json::from_value(value).map_err(D::Error::custom)?;
        Ok(Self {
            channel,
            note,
            side,
            behavior,
        })
    }
}
#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum SideBehavior {
    Action { action: Action },
    Toggle { address: String },
    Bank { bank: usize },
    Shift,
    SelectDeck,
    Browse { address: String },
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Analog {
    channel: u8,
    cc: u8,
    #[serde(default)]
    side: Option<usize>,
    address: String,
    #[serde(default)]
    shifted_address: Option<String>,
    #[serde(default)]
    bipolar: bool,
    #[serde(default)]
    direct: bool,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Encoder {
    channel: u8,
    cc: u8,
    up: String,
    down: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Led {
    address: String,
    channel: u8,
    kind: LedKind,
    number: u8,
    #[serde(default = "on_value")]
    on: u8,
    #[serde(default)]
    off: u8,
}
fn on_value() -> u8 {
    127
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LedKind {
    Switch,
    Meter,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PadWiring {
    channels: [u8; 4],
    bank_notes: [u8; 8],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    buttons: Vec<SideButton>,
    analog: Vec<Analog>,
    browser_encoder: Encoder,
    leds: Vec<Led>,
    pads: Option<Profile>,
    pad_wiring: PadWiring,
}
fn config(profile: &DeviceProfile) -> Result<Config> {
    if profile.protocol != "flx4" {
        bail!("FLX4 capability requires protocol 'flx4'");
    }
    let value = profile.protocol_config.clone();
    if value.is_null() {
        bail!("FLX4 protocol_config is required and cannot be null");
    }
    serde_json::from_value(value).context("Invalid FLX4 protocol_config")
}
fn valid_route(route: &str) -> bool {
    crate::device_profile::valid_address(route)
}
fn resolved(route: &str, deck: usize) -> String {
    route.replace("{deck}", &(deck + 1).to_string())
}
/// Pure pre-MIDI validation, including the embedded version-1 pad format.
pub fn validate_config(profile: &DeviceProfile) -> Result<()> {
    profile.validate()?;
    let mut cfg = config(profile)?;
    if profile.inputs.len() != 1 || profile.outputs.len() != 1 {
        bail!("FLX4 requires exactly one input and one output");
    }
    if !profile.startup.is_empty()
        || !profile.periodic.is_empty()
        || !profile.shutdown.is_empty()
        || !profile.bindings.is_empty()
        || !profile.feedback.is_empty()
        || !profile.modes.is_empty()
        || !profile.layers.is_empty()
        || !profile.deck_feedback.is_empty()
    {
        bail!("FLX4 protocol_config owns MIDI hooks, buttons, feedback and modes; top-level declarative bindings/hooks are not supported");
    }
    cfg.pads
        .as_mut()
        .context("FLX4 protocol_config.pads is required")?
        .validate()?;
    if cfg.buttons.len() > 512 || cfg.analog.len() > 64 || cfg.leds.len() > 128 {
        bail!("FLX4 profile exceeds limits (512 buttons, 64 analog controls, 128 LEDs)");
    }
    let mut buttons = HashSet::new();
    for b in &mut cfg.buttons {
        if !matches!(b.channel, 0 | 1 | 6)
            || b.note > 127
            || b.side
                .is_some_and(|side| side > 1 || b.channel <= 1 && side != b.channel as usize)
            || !buttons.insert((b.channel, b.note))
        {
            bail!(
                "Invalid/duplicate FLX4 button on channel {} note {}",
                b.channel,
                b.note
            );
        }
        match &mut b.behavior {
            SideBehavior::Action { action } => action.validate()?,
            SideBehavior::Toggle { address } | SideBehavior::Browse { address }
                if !valid_route(address) =>
            {
                bail!("Invalid FLX4 button route: {address}")
            }
            SideBehavior::Bank { bank } if *bank >= 8 || b.channel > 1 => {
                bail!("FLX4 bank must be 0..7 on deck side")
            }
            SideBehavior::Shift if b.channel > 1 => bail!("FLX4 Shift must be on deck side"),
            SideBehavior::SelectDeck if b.channel != 6 => {
                bail!("FLX4 deck selector requires channel 6")
            }
            _ => {}
        }
        if b.channel == 6
            && b.side.is_none()
            && (matches!(&b.behavior, SideBehavior::SelectDeck)
                || matches!(&b.behavior, SideBehavior::Browse { address } if address.contains("{deck}"))
                || matches!(&b.behavior, SideBehavior::Action { action } if action.address.contains("{deck}"))
                || matches!(&b.behavior, SideBehavior::Toggle { address } if address.contains("{deck}")))
        {
            bail!(
                "FLX4 channel-6 deck-targeting button {} needs explicit side 0 or 1",
                b.note
            );
        }
    }
    let mut cc = HashSet::new();
    for a in &cfg.analog {
        if !matches!(a.channel, 0 | 1 | 6)
            || a.cc > 31
            || a.side
                .is_some_and(|side| side > 1 || a.channel <= 1 && side != a.channel as usize)
            || !cc.insert((a.channel, a.cc))
            || !valid_route(&a.address)
            || a.shifted_address.as_ref().is_some_and(|v| !valid_route(v))
        {
            bail!(
                "Invalid/duplicate FLX4 14-bit analog channel {} CC {}",
                a.channel,
                a.cc
            );
        }
        if a.channel == 6
            && a.side.is_none()
            && (a.address.contains("{deck}")
                || a.shifted_address
                    .as_ref()
                    .is_some_and(|v| v.contains("{deck}")))
        {
            bail!(
                "FLX4 channel-6 deck-targeting CC {} needs explicit side 0 or 1",
                a.cc
            );
        }
        if a.bipolar != a.address.ends_with("/TempoSlider")
            || a.shifted_address
                .as_ref()
                .is_some_and(|v| a.bipolar != v.ends_with("/TempoSlider"))
        {
            bail!("FLX4 bipolar scaling is required only for TempoSlider");
        }
    }
    let e = &cfg.browser_encoder;
    if e.channel > 15
        || e.cc > 127
        || !valid_route(&e.up)
        || !valid_route(&e.down)
        || cc.contains(&(e.channel, e.cc))
        || (e.cc >= 32 && cc.contains(&(e.channel, e.cc - 32)))
    {
        bail!("Invalid or conflicting FLX4 browser encoder");
    }
    if cfg.pad_wiring.channels != [7, 8, 9, 10]
        || cfg.pad_wiring.bank_notes.iter().any(|n| *n > 127)
    {
        bail!(
            "FLX4 pad wiring requires physical MIDI channels 7,8,9,10 and eight valid bank notes"
        );
    }
    for (i, note) in cfg.pad_wiring.bank_notes.iter().enumerate() {
        if !cfg.buttons.iter().any(|b| {
            b.channel <= 1
                && b.note == *note
                && matches!(&b.behavior, SideBehavior::Bank { bank } if *bank == i)
        }) {
            bail!("FLX4 bank note {note} has no matching bank button {i}");
        }
    }
    let mut leds = HashSet::new();
    for led in &cfg.leds {
        if led.channel > 1
            || led.number > 127
            || led.on > 127
            || led.off > 127
            || !valid_route(&led.address)
            || !leds.insert((led.channel, led.number, matches!(led.kind, LedKind::Meter)))
        {
            bail!("Invalid or duplicate FLX4 LED");
        }
    }
    Ok(())
}

pub struct Flx4 {
    name: String,
    cfg: Config,
    pads: Profile,
    outputs: Vec<MidiOutputConnection>,
    ap: SharedAp,
    selected: [usize; 2],
    banks: [usize; 4],
    players: [Player; 4],
    pressed: HashSet<Key>,
    held: HashMap<Key, String>,
    shifts: [bool; 2],
    msb: HashMap<Key, u8>,
    positions: HashMap<Key, u16>,
    captured: HashSet<(Key, String)>,
    leds: HashMap<(u8, u8), u8>,
    last_keepalive: Instant,
    midi_count: u64,
    feedback_count: u64,
    warning: Option<String>,
}
impl Flx4 {
    pub fn new(
        name: String,
        profile: DeviceProfile,
        outputs: Vec<MidiOutputConnection>,
        ap: SharedAp,
    ) -> Result<Self> {
        validate_config(&profile)?;
        if outputs.len() != 1 {
            bail!("FLX4 requires exactly one MIDI output connection");
        }
        let mut cfg = config(&profile)?;
        let mut pads = cfg.pads.take().expect("validated FLX4 pads");
        pads.validate()?;
        for a in &cfg.analog {
            for route in std::iter::once(&a.address).chain(a.shifted_address.iter()) {
                for deck in 0..4 {
                    ap.borrow_mut().register(
                        &resolved(route, deck),
                        if a.bipolar {
                            FeedbackKind::Bipolar
                        } else {
                            FeedbackKind::Unit
                        },
                    )?;
                }
            }
        }
        for b in &cfg.buttons {
            if let SideBehavior::Toggle { address } = &b.behavior {
                for deck in 0..4 {
                    ap.borrow_mut()
                        .register(&resolved(address, deck), FeedbackKind::Switch)?;
                }
            }
        }
        for led in &cfg.leds {
            for deck in 0..4 {
                ap.borrow_mut().register(
                    &resolved(&led.address, deck),
                    match led.kind {
                        LedKind::Switch => FeedbackKind::Switch,
                        LedKind::Meter => FeedbackKind::Meter,
                    },
                )?;
            }
        }
        for deck in 0..4 {
            ap.borrow_mut().register(
                &format!("/AutoPilot/Deck/{}/PlayerType", deck + 1),
                FeedbackKind::Text,
            )?;
        }
        Ok(Self {
            name,
            cfg,
            pads,
            outputs,
            ap,
            selected: [0, 1],
            banks: [0; 4],
            players: [Player::Unknown; 4],
            pressed: HashSet::new(),
            held: HashMap::new(),
            shifts: [false; 2],
            msb: HashMap::new(),
            positions: HashMap::new(),
            captured: HashSet::new(),
            leds: HashMap::new(),
            last_keepalive: Instant::now() - Duration::from_secs(1),
            midi_count: 0,
            feedback_count: 0,
            warning: None,
        })
    }
    fn warn(&mut self, message: String) {
        if self.warning.as_deref() != Some(&message) {
            eprintln!("{}: {message}", self.name);
        }
        self.warning = Some(message);
    }
    fn emit(&mut self, bytes: &[u8]) -> Result<()> {
        self.outputs[0]
            .send(bytes)
            .map_err(|e| anyhow!("{} FLX4 output: {e}", self.name))
    }
    fn send(&mut self, address: &str, value: OscType) -> Result<()> {
        self.ap.borrow().send(address, value)
    }
    fn owner(&self, key: Key) -> String {
        format!("{}:{}:note:{}:{}", self.name, key.0, key.1, key.2)
    }
    fn route(&mut self, address: String) -> Option<String> {
        let Some(rest) = address.strip_prefix("/AutoPilot/Deck/") else {
            return Some(address);
        };
        let (number, route) = rest.split_once('/')?;
        let deck = number.parse::<usize>().ok()?.checked_sub(1)?;
        let player = *self.players.get(deck)?;
        match player.route(route) {
            Some(route) => Some(format!("/AutoPilot/Deck/{}/{route}", deck + 1)),
            None => {
                self.warn(format!("Deck {} ({player:?}): {route} unavailable; check PlayerType feedback or player layout", deck + 1));
                None
            }
        }
    }
    fn press(&mut self, key: Key, action: Action, deck: usize) -> Result<()> {
        let mut action = action.resolved(deck);
        let Some(address) = self.route(action.address) else {
            return Ok(());
        };
        action.address = address;
        if action.address.contains("/SlipLoop/") {
            action.slip_timing.get_or_insert(SlipTiming::Straight);
        }
        if action.toggle {
            let (prefix, suffix) = action
                .address
                .rsplit_once('/')
                .context("Slip toggle address")?;
            let timing = match suffix {
                "SlipDotted" => SlipTiming::Dotted,
                "SlipTriplet" => SlipTiming::Triplet,
                _ => bail!("Invalid slip toggle"),
            };
            self.ap.borrow_mut().slip_toggle(prefix, timing)?;
        } else if let Some(release) = action.release {
            let owner = self.owner(key);
            self.held.insert(key, owner.clone());
            // Version-1 actions without slip_timing default to Straight (normalized
            // by Profile::validate when the mapping is loaded).
            if action.slip_timing.is_some() {
                self.ap.borrow_mut().slip_hold(
                    &owner,
                    &action.address,
                    action.slip_timing.unwrap_or(SlipTiming::Straight),
                    action.press,
                    release,
                )?;
            } else {
                self.ap
                    .borrow_mut()
                    .hold(&owner, &action.address, action.press, release)?;
            }
        } else {
            self.send(&action.address, OscType::Int(action.press))?;
        }
        Ok(())
    }
    fn release(&mut self, key: Key) -> Result<()> {
        self.pressed.remove(&key);
        if let Some(owner) = self.held.get(&key) {
            self.ap.borrow_mut().release(owner)?;
            self.held.remove(&key);
        }
        Ok(())
    }
    fn mode_feedback(&mut self, side: usize) -> Result<()> {
        let note = self.cfg.pad_wiring.bank_notes[self.banks[self.selected[side]]];
        self.emit(&[0x90 + side as u8, note, 127])
    }
    fn rearm(&mut self, keys: &HashSet<Key>) {
        self.msb.retain(|key, _| !keys.contains(key));
        self.positions.retain(|key, _| !keys.contains(key));
        self.captured.retain(|(key, _)| !keys.contains(key));
    }
    fn analog_keys(&self, side: usize, shift_only: bool) -> HashSet<Key> {
        self.cfg
            .analog
            .iter()
            .filter(|analog| {
                let target_side = analog.side.unwrap_or(if analog.channel <= 1 {
                    analog.channel as usize
                } else {
                    0
                });
                target_side == side
                    && if shift_only {
                        analog
                            .shifted_address
                            .as_ref()
                            .is_some_and(|shifted| shifted != &analog.address)
                    } else {
                        analog.address.contains("{deck}")
                            || analog
                                .shifted_address
                                .as_ref()
                                .is_some_and(|s| s.contains("{deck}"))
                    }
            })
            .map(|analog| (0, analog.channel, analog.cc))
            .collect()
    }
    fn select_deck(&mut self, side: usize) -> Result<()> {
        self.selected[side] ^= 2;
        let keys = self.analog_keys(side, false);
        self.rearm(&keys);
        self.mode_feedback(side)?;
        self.refresh_leds(side)?;
        self.send("/AutoPilot/Feedback/Resend", OscType::Int(1))
    }
    fn shift(&mut self, side: usize, on: bool) -> Result<()> {
        if self.shifts[side] == on {
            return Ok(());
        }
        self.shifts[side] = on;
        let keys = self.analog_keys(side, true);
        self.rearm(&keys);
        self.send("/AutoPilot/Feedback/Resend", OscType::Int(1))
    }
    fn button(&mut self, key: Key, channel: u8, raw: u8) -> Result<()> {
        let Some(binding) = self
            .cfg
            .buttons
            .iter()
            .find(|b| b.channel == channel && b.note == raw)
            .cloned()
        else {
            return Ok(());
        };
        let side = binding
            .side
            .unwrap_or(if channel <= 1 { channel as usize } else { 0 });
        let deck = self.selected[side];
        match binding.behavior {
            SideBehavior::Action { action } => self.press(key, action, deck)?,
            SideBehavior::Toggle { address } => {
                let address = resolved(&address, deck);
                self.ap.borrow_mut().toggle(&address)?;
            }
            SideBehavior::Bank { bank } => {
                self.banks[deck] = bank;
                self.mode_feedback(side)?;
            }
            SideBehavior::Shift => self.shift(side, true)?,
            SideBehavior::SelectDeck => self.select_deck(side)?,
            SideBehavior::Browse { address } => {
                let address = resolved(&address, deck);
                self.send(&address, OscType::Int(1))?;
            }
        }
        Ok(())
    }
    fn cc(&mut self, port: usize, channel: u8, number: u8, value: u8) -> Result<()> {
        let e = &self.cfg.browser_encoder;
        if channel == e.channel && number == e.cc {
            // 0 and 64 are neutral reports in the FLX4 relative encoder protocol.
            let delta = if value == 0 || value == 64 {
                0
            } else if value < 64 {
                value as i16
            } else {
                value as i16 - 128
            };
            if delta != 0 {
                let address = if delta > 0 {
                    e.down.clone()
                } else {
                    e.up.clone()
                };
                for _ in 0..delta.unsigned_abs() {
                    self.send(&address, OscType::Int(1))?;
                }
            }
            return Ok(());
        }
        if number >= 64 {
            return Ok(());
        }
        let base = number & 0x1f;
        let Some(binding) = self
            .cfg
            .analog
            .iter()
            .find(|a| a.channel == channel && a.cc == base)
            .cloned()
        else {
            return Ok(());
        };
        let side = binding
            .side
            .unwrap_or(if channel <= 1 { channel as usize } else { 0 });
        let deck = self.selected[side];
        let route = if self.shifts[side] {
            binding.shifted_address.as_ref().unwrap_or(&binding.address)
        } else {
            &binding.address
        };
        let address = resolved(route, deck);
        let key = (port, channel, base);
        if number < 32 {
            self.msb.insert(key, value);
            return Ok(());
        }
        let Some(high) = self.msb.get(&key) else {
            return Ok(());
        };
        let full = ((*high as u16) << 7) | value as u16;
        let Some(previous) = self.positions.insert(key, full).filter(|old| *old != full) else {
            return Ok(());
        };
        let Some(address) = self.route(address) else {
            return Ok(());
        };
        let identity = (key, address.to_ascii_lowercase());
        let position = full as f32 / 16383.0;
        let direct = binding.direct;
        let owner = format!("{}:{}:cc:{}:{}", self.name, port, channel, base);
        if !direct && !self.ap.borrow().control_owned(&owner, &address) {
            self.captured.remove(&identity);
        }
        if !direct && self.ap.borrow().control_pending(&owner, &address) {
            self.warn(format!("Pickup waiting at {address}: another physical control wrote it; awaiting AP feedback"));
            return Ok(());
        }
        if !direct && !self.captured.contains(&identity) {
            let target = self.ap.borrow().value(&address);
            let Some(target) = target else {
                self.warn(format!(
                    "Pickup blocked at {address}; enable OSC feedback subscription"
                ));
                return Ok(());
            };
            let previous = previous as f32 / 16383.0;
            if (previous - target) * (position - target) > 0.0
                && (position - target).abs() > 2.0 / 16383.0
            {
                return Ok(());
            }
            self.captured.insert(identity);
        }
        let output = if binding.bipolar {
            if matches!(full, 8191 | 8192) {
                0.0
            } else {
                position * 2.0 - 1.0
            }
        } else {
            position
        };
        self.ap
            .borrow_mut()
            .send_control(&owner, &address, OscType::Float(output))?;
        Ok(())
    }
    fn led(&mut self, status: u8, number: u8, value: u8) -> Result<()> {
        if self.leds.get(&(status, number)) != Some(&value) {
            self.emit(&[status, number, value])?;
            self.leds.insert((status, number), value);
        }
        Ok(())
    }
    fn refresh_leds(&mut self, side: usize) -> Result<()> {
        let deck = self.selected[side];
        let leds: Vec<_> = self
            .cfg
            .leds
            .iter()
            .filter(|led| led.channel as usize == side)
            .cloned()
            .collect();
        for led in leds {
            let address = resolved(&led.address, deck);
            let value = self.ap.borrow().value(&address);
            let (status, code) = match led.kind {
                LedKind::Switch => (
                    0x90 + side as u8,
                    if value.is_some_and(|v| v >= 0.5) {
                        led.on
                    } else {
                        led.off
                    },
                ),
                LedKind::Meter => (
                    0xb0 + side as u8,
                    (value.unwrap_or(0.0).clamp(0.0, 1.0) * 127.0).round() as u8,
                ),
            };
            self.led(status, led.number, code)?;
        }
        Ok(())
    }
}
impl Device for Flx4 {
    fn start(&mut self) -> Result<()> {
        for side in 0..2 {
            self.mode_feedback(side)?;
            self.refresh_leds(side)?;
        }
        self.send("/AutoPilot/Feedback/Resend", OscType::Int(1))
    }
    fn input(&mut self, port: usize, bytes: &[u8]) -> Result<()> {
        if port != 0 || bytes.len() != 3 || bytes[1] > 127 || bytes[2] > 127 {
            return Ok(());
        }
        self.midi_count += 1;
        let (kind, channel, number, value) = (bytes[0] & 0xf0, bytes[0] & 15, bytes[1], bytes[2]);
        if kind == 0xb0 {
            return self.cc(port, channel, number, value);
        }
        if kind != 0x90 && kind != 0x80 {
            return Ok(());
        }
        // Shifted pad reports alias the unshifted physical channel. Preserve that
        // canonical channel for both Note On and Note Off ownership/deduplication.
        let (canonical_channel, canonical) = crate::mapping::key(channel, number);
        let key = (port, canonical_channel, canonical);
        if kind == 0x80 || value == 0 {
            if channel <= 1 && canonical == 0x3f {
                self.shift(channel as usize, false)?;
            }
            return self.release(key);
        }
        if !self.pressed.insert(key) {
            return Ok(());
        }
        if channel <= 1 || channel == 6 {
            self.button(key, channel, number)?;
        } else if let Some(index) = self
            .cfg
            .pad_wiring
            .channels
            .iter()
            .position(|ch| *ch == channel)
        {
            if number & 15 >= 8 {
                return Ok(());
            }
            let side = index / 2;
            let deck = self.selected[side];
            let action = self.pads.banks[self.banks[deck]]
                .action(
                    self.players[deck].name(),
                    (number & 15) as usize,
                    index % 2 != 0,
                )
                .cloned();
            if let Some(action) = action {
                self.press(key, action, deck)?;
            }
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<()> {
        if self.last_keepalive.elapsed() >= Duration::from_millis(200) {
            self.emit(&KEEPALIVE)?;
            self.last_keepalive = Instant::now();
        }
        Ok(())
    }
    fn feedback(&mut self, address: &str) -> Result<()> {
        self.feedback_count += 1;
        let key = address.to_ascii_lowercase();
        for deck in 0..4 {
            if key == format!("/autopilot/deck/{}/playertype", deck + 1) {
                let player = self
                    .ap
                    .borrow()
                    .text(address)
                    .map(|s| Player::parse(&s.to_ascii_lowercase()))
                    .unwrap_or(Player::Unknown);
                if self.players[deck] != player {
                    self.players[deck] = player;
                    self.captured.retain(|(_, route)| {
                        route != &format!("/autopilot/deck/{}/temposlider", deck + 1)
                    });
                }
            }
        }
        {
            let ap = self.ap.borrow();
            let name = &self.name;
            // Latch pickup, as in the original FLX4 bridge. AP feedback has no
            // writer identity and can lag several physical reports: disagreement
            // with the latest position is NOT evidence of another writer.
            // Known bridge takeovers and invalid feedback still re-arm pickup.
            self.captured.retain(|((port, channel, base), route)| {
                route != &key
                    || (ap.value(address).is_some()
                        && ap.control_owned(&format!("{name}:{port}:cc:{channel}:{base}"), address))
            });
        }
        for side in 0..2 {
            self.refresh_leds(side)?;
        }
        Ok(())
    }
    fn status(&self, verbose: bool) -> String {
        let side = |i: usize| {
            let deck = self.selected[i];
            let timing = self
                .ap
                .borrow()
                .slip_selected(&format!("/AutoPilot/Deck/{}", deck + 1));
            format!(
                "{}: Deck {} {:?} / {} / Repeat {:?}",
                if i == 0 { "LEFT" } else { "RIGHT" },
                deck + 1,
                self.players[deck],
                self.pads.banks[self.banks[deck]].name(self.players[deck].name()),
                timing
            )
        };
        let mut status = format!("{} | {}", side(0), side(1));
        if verbose {
            status.push_str(&format!(
                " | pickup={}/{}",
                self.captured.len(),
                self.positions.len()
            ));
        }
        status
    }
    fn cleanup(&mut self) {
        for owner in self.held.values() {
            if let Err(e) = self.ap.borrow_mut().release(owner) {
                eprintln!("{}: release: {e}", self.name);
            }
        }
        self.held.clear();
        for side in 0..2 {
            let leds: Vec<_> = self
                .cfg
                .leds
                .iter()
                .filter(|led| led.channel as usize == side)
                .map(|l| (l.kind, l.number, l.off))
                .collect();
            for (kind, number, off) in leds {
                let status = match kind {
                    LedKind::Switch => 0x90,
                    LedKind::Meter => 0xb0,
                } + side as u8;
                if let Err(e) = self.emit(&[status, number, off]) {
                    eprintln!("{}: LED cleanup: {e}", self.name);
                }
            }
            // Restore FLX4 pad-mode indication, without touching another session's outputs.
            if let Err(e) = self.emit(&[0x90 + side as u8, self.cfg.pad_wiring.bank_notes[0], 127])
            {
                eprintln!("{}: mode cleanup: {e}", self.name);
            }
        }
    }
}
