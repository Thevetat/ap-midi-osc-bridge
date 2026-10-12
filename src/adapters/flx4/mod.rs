//! DDJ-FLX4 MIDI protocol capability. MIDI decoding lives here;
//! AP assignments and pad banks live in `profiles/flx4.json`.
mod config;
mod pads;

use super::Adapter;
use crate::{
    ap::{FeedbackKind, Player, SharedAp, SlipTiming},
    profile::DeviceProfile,
};
use anyhow::{anyhow, bail, Context, Result};
use config::{Config, LedKind, SideBehavior};
use midir::MidiOutputConnection;
use pads::Action;
use rosc::OscType;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

type Key = (usize, u8, u8); // input port, wire channel, canonical physical note
const KEEPALIVE: [u8; 12] = [
    0xf0, 0x00, 0x40, 0x05, 0x00, 0x00, 0x04, 0x05, 0x00, 0x50, 0x02, 0xf7,
];

pub(super) fn validate(profile: &DeviceProfile) -> Result<()> {
    config::load(profile).map(|_| ())
}

fn resolved(route: &str, deck: usize) -> String {
    route.replace("{deck}", &(deck + 1).to_string())
}

// Use physical keys so a release is still recognised after Shift or deck changes.
fn physical_key(channel: u8, note: u8) -> (u8, u8) {
    let note = match (channel, note) {
        (0..=1, 0x0e) => 0x0b,
        (0..=1, 0x48) => 0x0c,
        (0..=1, 0x4c) => 0x10,
        (0..=1, 0x4e) => 0x11,
        (0..=1, 0x50) => 0x4d,
        (0..=1, 0x5c | 0x60) => 0x58,
        (0..=1, 0x68) => 0x54,
        (0..=1, 0x69) => 0x1b,
        (0..=1, 0x6b) => 0x1e,
        (0..=1, 0x6d) => 0x20,
        (0..=1, 0x6f) => 0x22,
        (6, 0x68) => 0x46,
        (6, 0x7a) => 0x47,
        (6, 0x42) => 0x41,
        (7..=10, _) => return (7 + ((channel - 7) / 2) * 2, note & 0x0f),
        _ => note,
    };
    (channel, note)
}

pub struct Flx4 {
    name: String,
    cfg: Config,
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
    warning: Option<String>,
}
impl Flx4 {
    pub fn new(
        name: String,
        profile: DeviceProfile,
        outputs: Vec<MidiOutputConnection>,
        ap: SharedAp,
    ) -> Result<Self> {
        profile.validate()?;
        if outputs.len() != 1 {
            bail!("FLX4 requires exactly one MIDI output connection");
        }
        let cfg = config::load(&profile)?;
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
            if let Some(timing) = action.slip_timing {
                self.ap.borrow_mut().slip_hold(
                    &owner,
                    &action.address,
                    timing,
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
impl Adapter for Flx4 {
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
        let (kind, channel, number, value) = (bytes[0] & 0xf0, bytes[0] & 15, bytes[1], bytes[2]);
        if kind == 0xb0 {
            return self.cc(port, channel, number, value);
        }
        if kind != 0x90 && kind != 0x80 {
            return Ok(());
        }
        // Shifted pad reports alias the unshifted physical channel. Preserve that
        // canonical channel for both Note On and Note Off ownership/deduplication.
        let (canonical_channel, canonical) = physical_key(channel, number);
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
            let action = self.cfg.pads.banks[self.banks[deck]]
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
                self.cfg.pads.banks[self.banks[deck]].name(self.players[deck].name()),
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
