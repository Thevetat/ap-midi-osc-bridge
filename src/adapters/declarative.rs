//! Declarative per-device decoder. All AP values and switch/hold arbitration live in SharedAp.
use super::Adapter;
use crate::{
    ap::SharedAp,
    profile::{Behavior, Binding, DeclarativeConfig, FeedbackKind, MidiKind},
};
use anyhow::{anyhow, Result};
use midir::MidiOutputConnection;
use rosc::OscType;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::{Duration, Instant},
};

type Physical = (usize, MidiKind, u8, u8);

pub struct Declarative {
    name: String,
    profile: DeclarativeConfig,
    input_count: usize,
    outputs: Vec<MidiOutputConnection>,
    ap: SharedAp,
    mode: Option<String>,
    layer: usize,
    selected: [usize; 4],
    pressed: HashSet<Physical>,
    held: HashMap<Physical, String>,
    msb: HashMap<Physical, u8>,
    positions: HashMap<(Physical, String), f32>,
    captured: HashSet<(Physical, String)>,
    sent: HashMap<(usize, u8, u8), u8>,
    echoes: HashMap<(usize, u8, u8), VecDeque<(u8, Instant)>>,
    recenter_seen: HashSet<Physical>,
    last_tick: Instant,
    warning: Option<String>,
}

impl Declarative {
    pub fn new(
        name: String,
        profile: DeclarativeConfig,
        input_count: usize,
        outputs: Vec<MidiOutputConnection>,
        ap: SharedAp,
    ) -> Result<Self> {
        for (address, kind) in profile.feedback_interests() {
            ap.borrow_mut().register(&name, &address, kind)?;
        }
        let mode = if profile.modes.len() == 1 && profile.modes[0].reports.is_empty() {
            Some(profile.modes[0].name.clone())
        } else {
            None
        };
        Ok(Self {
            name,
            profile,
            input_count,
            outputs,
            ap,
            mode,
            layer: 0,
            selected: [1, 2, 3, 4],
            pressed: HashSet::new(),
            held: HashMap::new(),
            msb: HashMap::new(),
            positions: HashMap::new(),
            captured: HashSet::new(),
            sent: HashMap::new(),
            echoes: HashMap::new(),
            recenter_seen: HashSet::new(),
            last_tick: Instant::now(),
            warning: None,
        })
    }

    fn warn(&mut self, message: String) {
        if self.warning.as_deref() != Some(&message) {
            eprintln!("{}: {message}", self.name);
        }
        self.warning = Some(message);
    }

    fn resolve(&self, address: &str, side: usize) -> String {
        address.replace("{deck}", &self.selected[side].to_string())
    }

    fn emit(
        &mut self,
        output: usize,
        bytes: &[u8],
        cache: bool,
        echo_input: Option<usize>,
    ) -> Result<bool> {
        let Some(conn) = self.outputs.get_mut(output) else {
            return Err(anyhow!("{}: MIDI output {output} unavailable", self.name));
        };
        if cache
            && bytes.len() == 3
            && self.sent.get(&(output, bytes[0], bytes[1])) == Some(&bytes[2])
        {
            return Ok(false);
        }
        conn.send(bytes)
            .map_err(|e| anyhow!("{} output {output}: {e}", self.name))?;
        if bytes.len() == 3 {
            if cache {
                self.sent.insert((output, bytes[0], bytes[1]), bytes[2]);
            }
            // Only explicitly configured output feedback/recenter can be echoed.
            // Lifecycle hooks (including mode queries) must never mask mode reports.
            if let Some(input) = echo_input {
                let pending = self.echoes.entry((input, bytes[0], bytes[1])).or_default();
                pending.retain(|(_, when)| when.elapsed() < Duration::from_millis(500));
                if pending.len() >= 8 {
                    pending.pop_front();
                }
                pending.push_back((bytes[2], Instant::now()));
            }
        }
        Ok(true)
    }

    fn hooks(&mut self, messages: &[crate::profile::MidiMessage]) -> Result<()> {
        for m in messages {
            // Raw hooks may replace an LED that previously followed AP feedback.
            // Do not let its cached old colour prevent repainting on layer return.
            if m.bytes.len() == 3 {
                self.sent.remove(&(m.output, m.bytes[0], m.bytes[1]));
            }
            self.emit(m.output, &m.bytes, false, None)?;
        }
        Ok(())
    }

    fn owner(&self, key: Physical) -> String {
        format!("{}:{}:{:?}:{}:{}", self.name, key.0, key.1, key.2, key.3)
    }

    fn mode_confirmed(&self) -> bool {
        self.mode.is_some() || self.profile.modes.iter().all(|m| m.reports.is_empty())
    }

    fn active(&self, mode: &Option<String>) -> bool {
        mode.as_ref()
            .map_or(true, |m| self.mode.as_ref() == Some(m))
    }

    fn active_layer(&self, layer: &Option<String>) -> bool {
        layer.as_ref().is_none_or(|name| {
            self.profile
                .layers
                .get(self.layer)
                .is_some_and(|l| &l.name == name)
        })
    }

    fn render_selection(&mut self) -> Result<()> {
        if !self.mode_confirmed() {
            return Ok(());
        }
        // These are local selection indicators, never claims about AP cue occupancy.
        let messages: Vec<_> = self
            .profile
            .deck_feedback
            .iter()
            .filter(|f| self.selected[f.deck_side] == f.deck)
            .flat_map(|f| f.messages.iter().cloned())
            .chain(
                self.profile
                    .layers
                    .get(self.layer)
                    .into_iter()
                    .flat_map(|l| l.on_enter.iter().cloned()),
            )
            .collect();
        self.hooks(&messages)
    }

    fn select_deck(&mut self, side: usize, deck: usize) -> Result<()> {
        if self.selected[side] == deck {
            return Ok(());
        }
        self.selected[side] = deck;
        self.rearm_deck_side(side);
        self.render_selection()?;
        self.refresh()?;
        self.ap
            .borrow_mut()
            .send("/AutoPilot/Feedback/Resend", OscType::Int(1))?;
        Ok(())
    }

    fn cycle_layer(&mut self) -> Result<()> {
        self.layer = (self.layer + 1) % self.profile.layers.len();
        // Only layer-scoped controls need re-arming. Fixed EQ/filter encoders
        // must not be disturbed when the bottom button row changes function.
        let affected: HashSet<_> = self
            .profile
            .bindings
            .iter()
            .filter(|b| b.layer.is_some())
            .map(|b| (b.input, b.kind, b.channel, b.number))
            .collect();
        self.rearm_controls(&affected);
        for f in &self.profile.feedback {
            if f.layer.is_some() {
                let status = (match f.midi_kind {
                    MidiKind::Note => 0x90,
                    MidiKind::Cc => 0xb0,
                }) | (f.channel - 1);
                self.sent.remove(&(f.output, status, f.number));
            }
        }
        // Keep pressed/held identities: changing layer must never retrigger a
        // held button or redirect its eventual release to the new layer.
        self.render_selection()?;
        self.refresh()?;
        Ok(())
    }

    fn refresh(&mut self) -> Result<()> {
        if !self.mode_confirmed() {
            return Ok(());
        }
        for index in 0..self.profile.feedback.len() {
            let f = &self.profile.feedback[index];
            if !self.active(&f.mode) || !self.active_layer(&f.layer) {
                continue;
            }
            let address = self.resolve(&f.address, f.deck_side);
            let raw = self.ap.borrow().value(&address);
            if raw.is_none() && matches!(f.render, FeedbackKind::Position {}) {
                let status = 0xb0 | (f.channel - 1);
                self.sent.remove(&(f.output, status, f.number));
            }
            let code = match &f.render {
                FeedbackKind::Led { on, off, unknown } => match raw {
                    Some(v) if v.is_finite() && (0.0..=1.0).contains(&v) => {
                        Some(if v >= 0.5 { *on } else { *off })
                    }
                    _ => *unknown,
                },
                FeedbackKind::Position {} => raw
                    .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    .map(|v| (v * 127.0).round() as u8),
            };
            if let Some(code) = code {
                let status = match f.midi_kind {
                    MidiKind::Note => 0x90,
                    MidiKind::Cc => 0xb0,
                } | (f.channel - 1);
                let echo_input = f
                    .echo_input
                    .or_else(|| (self.input_count == 1).then_some(0));
                let wrote = self.emit(f.output, &[status, f.number, code], true, echo_input)?;
                // Reborrow config after emit mutates output/echo caches; no list clone.
                let f = &self.profile.feedback[index];
                if wrote
                    && matches!(f.render, FeedbackKind::Position {})
                    && f.midi_kind == MidiKind::Cc
                {
                    // Position-synchronized endless encoders are already at the confirmed AP
                    // value. Mark that physical/target pair captured; host echoes are ignored.
                    for b in &self.profile.bindings {
                        let target = match &b.behavior {
                            Behavior::Absolute { address, .. }
                            | Behavior::Absolute14 { address, .. } => address,
                            _ => continue,
                        };
                        if echo_input == Some(b.input)
                            && !matches!(b.behavior, Behavior::Absolute14 { .. })
                            && b.kind == f.midi_kind
                            && b.channel == f.channel
                            && b.number == f.number
                            && self.active(&b.mode)
                            && self.active_layer(&b.layer)
                            && self
                                .resolve(target, b.deck_side)
                                .eq_ignore_ascii_case(&address)
                        {
                            let identity = (
                                (b.input, b.kind, b.channel, b.number),
                                address.to_ascii_lowercase(),
                            );
                            self.positions.insert(identity.clone(), code as f32 / 127.0);
                            self.captured.insert(identity);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Center only controls in the confirmed hardware mode and current software layer.
    fn recenter_controls(&mut self) -> Result<()> {
        let recenter: Vec<_> = self
            .profile
            .bindings
            .iter()
            .filter(|b| {
                self.mode_confirmed() && self.active(&b.mode) && self.active_layer(&b.layer)
            })
            .filter_map(|b| match &b.behavior {
                Behavior::Recenter { output, center, .. } => {
                    Some((*output, b.input, b.channel, b.number, *center))
                }
                _ => None,
            })
            .collect();
        for (output, input, channel, number, center) in recenter {
            self.emit(
                output,
                &[0xb0 | (channel - 1), number, center],
                false,
                Some(input),
            )?;
        }
        Ok(())
    }

    fn changed_mode(&mut self, next: Option<String>) -> Result<()> {
        if self.mode == next {
            return Ok(());
        }
        self.mode = next;
        // Never clear held owners: their original action/address must survive layer changes.
        self.msb.clear();
        self.positions.clear();
        self.captured.clear();
        self.sent.clear();
        self.echoes.clear();
        self.recenter_seen.clear();
        self.render_selection()?;
        self.refresh()?;
        self.recenter_controls()?;
        println!(
            "{}: mode {}",
            self.name,
            self.mode.as_deref().unwrap_or("unmapped/unknown")
        );
        Ok(())
    }

    fn rearm_controls(&mut self, affected: &HashSet<Physical>) {
        self.msb.retain(|key, _| !affected.contains(key));
        self.positions.retain(|(key, _), _| !affected.contains(key));
        self.captured.retain(|(key, _)| !affected.contains(key));
        self.recenter_seen.retain(|key| !affected.contains(key));
    }

    fn rearm_deck_side(&mut self, side: usize) {
        let affected: HashSet<Physical> = self
            .profile
            .bindings
            .iter()
            .filter(|b| {
                b.deck_side == side
                    && match &b.behavior {
                        Behavior::Button { address, .. }
                        | Behavior::Toggle { address }
                        | Behavior::Absolute { address, .. }
                        | Behavior::Absolute14 { address, .. } => address.contains("{deck}"),
                        Behavior::Relative { up, down } | Behavior::Recenter { up, down, .. } => {
                            up.contains("{deck}") || down.contains("{deck}")
                        }
                        Behavior::SelectDeck { .. }
                        | Behavior::StepDeck { .. }
                        | Behavior::CycleLayer {} => false,
                    }
            })
            .map(|b| (b.input, b.kind, b.channel, b.number))
            .collect();
        self.rearm_controls(&affected);
        for f in &self.profile.feedback {
            if f.deck_side == side && f.address.contains("{deck}") {
                let status = (match f.midi_kind {
                    MidiKind::Note => 0x90,
                    MidiKind::Cc => 0xb0,
                }) | (f.channel - 1);
                self.sent.remove(&(f.output, status, f.number));
                if let Some(input) = f
                    .echo_input
                    .or_else(|| (self.input_count == 1).then_some(0))
                {
                    self.echoes.remove(&(input, status, f.number));
                }
            }
        }
    }

    fn step(&mut self, address: &str) -> Result<()> {
        self.ap.borrow_mut().send(address, OscType::Int(1))?;
        Ok(())
    }

    fn absolute(
        &mut self,
        key: Physical,
        address: String,
        position: f32,
        bipolar: bool,
        pickup: bool,
        tolerance: f32,
    ) -> Result<()> {
        let identity = (key, address.to_ascii_lowercase());
        let previous = self.positions.insert(identity.clone(), position);
        // Reports at startup (or following a mode/deck switch) establish a baseline,
        // not movement. An unchanged position also cannot seize an AP parameter.
        let Some(previous) = previous.filter(|v| *v != position) else {
            return Ok(());
        };
        let owner = self.owner(key);
        if pickup && !self.ap.borrow().control_owned(&owner, &address) {
            self.captured.remove(&identity);
        }
        if pickup && self.ap.borrow().control_pending(&owner, &address) {
            self.warn(format!("Pickup waiting at {address}: another physical control wrote it; awaiting AP feedback"));
            return Ok(());
        }
        if pickup && !self.captured.contains(&identity) {
            let Some(target) = self.ap.borrow().value(&address) else {
                self.warn(format!(
                    "Pickup blocked at {address}: enable AP OSC feedback"
                ));
                return Ok(());
            };
            if !target.is_finite() || !(0.0..=1.0).contains(&target) {
                self.warn(format!("Pickup blocked at {address}: invalid AP feedback"));
                return Ok(());
            }
            if (previous - target) * (position - target) > 0.0
                && (position - target).abs() > tolerance
            {
                return Ok(());
            }
            self.captured.insert(identity);
        }
        self.ap.borrow_mut().send_control(
            &owner,
            &address,
            OscType::Float(if bipolar {
                position * 2.0 - 1.0
            } else {
                position
            }),
        )?;
        Ok(())
    }

    fn apply(&mut self, key: Physical, binding: Binding, value: u8) -> Result<()> {
        match binding.behavior {
            Behavior::Button {
                address,
                press,
                release,
            } => {
                if value == 0 {
                    return Ok(());
                }
                let address = self.resolve(&address, binding.deck_side);
                if let Some(release) = release {
                    let owner = self.owner(key);
                    self.held.insert(key, owner.clone());
                    self.ap
                        .borrow_mut()
                        .hold(&owner, &address, press, release)?;
                } else {
                    self.ap.borrow_mut().send(&address, OscType::Int(press))?;
                }
            }
            Behavior::Toggle { address } => {
                if value != 0 {
                    let address = self.resolve(&address, binding.deck_side);
                    self.ap.borrow_mut().toggle(&address)?;
                }
            }
            Behavior::SelectDeck { deck } => {
                if value != 0 {
                    self.select_deck(binding.deck_side, deck)?;
                }
            }
            Behavior::StepDeck { delta } => {
                if value != 0 {
                    let deck =
                        (self.selected[binding.deck_side] as i8 + delta).clamp(1, 4) as usize;
                    self.select_deck(binding.deck_side, deck)?;
                }
            }
            Behavior::CycleLayer {} => {
                if value != 0 {
                    self.cycle_layer()?;
                }
            }
            Behavior::Absolute {
                address,
                bipolar,
                pickup,
            } => {
                let address = self.resolve(&address, binding.deck_side);
                self.absolute(
                    key,
                    address,
                    value as f32 / 127.0,
                    bipolar,
                    pickup,
                    1.0 / 127.0,
                )?;
            }
            Behavior::Absolute14 {
                address,
                bipolar,
                pickup,
            } => {
                if key.3 != binding.number + 32 {
                    self.msb
                        .insert((key.0, key.1, key.2, binding.number), value);
                    return Ok(());
                }
                let msb = self.msb.remove(&(key.0, key.1, key.2, binding.number));
                if let Some(msb) = msb {
                    let full = ((msb as u16) << 7) | value as u16;
                    let address = self.resolve(&address, binding.deck_side);
                    self.absolute(
                        (key.0, key.1, key.2, binding.number),
                        address,
                        full as f32 / 16383.0,
                        bipolar,
                        pickup,
                        2.0 / 16383.0,
                    )?;
                }
            }
            Behavior::Relative { up, down } => {
                if value == 0 || value == 64 {
                    return Ok(());
                }
                let steps = if value < 64 {
                    value as i16
                } else {
                    value as i16 - 128
                };
                let address = self.resolve(if steps > 0 { &up } else { &down }, binding.deck_side);
                for _ in 0..steps.unsigned_abs() {
                    self.step(&address)?;
                }
            }
            Behavior::Recenter {
                up,
                down,
                center,
                output,
            } => {
                if self.recenter_seen.insert(key) {
                    // First hardware report in this layer may be a startup snapshot,
                    // not a human turn. Establish the center without sending an AP step.
                    if value != center {
                        self.emit(
                            output,
                            &[0xb0 | (binding.channel - 1), binding.number, center],
                            false,
                            Some(key.0),
                        )?;
                    }
                    return Ok(());
                }
                if value != center {
                    let address =
                        self.resolve(if value > center { &down } else { &up }, binding.deck_side);
                    self.step(&address)?;
                    self.emit(
                        output,
                        &[0xb0 | (binding.channel - 1), binding.number, center],
                        false,
                        Some(key.0),
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl Adapter for Declarative {
    fn start(&mut self) -> Result<()> {
        let hooks = self.profile.startup.clone();
        self.hooks(&hooks)?;
        self.last_tick = Instant::now();
        self.render_selection()?;
        self.refresh()?;
        self.recenter_controls()?;
        Ok(())
    }

    fn input(&mut self, port: usize, bytes: &[u8]) -> Result<()> {
        if port >= self.input_count || bytes.len() != 3 || bytes[1] > 127 || bytes[2] > 127 {
            return Ok(());
        }
        let (kind, channel) = match bytes[0] & 0xf0 {
            0x80 | 0x90 => (MidiKind::Note, (bytes[0] & 15) + 1),
            0xb0 => (MidiKind::Cc, (bytes[0] & 15) + 1),
            _ => return Ok(()),
        };
        let (number, value) = (bytes[1], bytes[2]);
        let key = (port, kind, channel, number);
        // Reports are control-plane messages, never echoes or physical movement. In
        // particular, a startup select/query hook must not swallow a matching report.
        let report_identity = self.profile.modes.iter().any(|m| {
            m.reports
                .iter()
                .any(|r| (r.input, r.kind, r.channel, r.number) == key)
        });
        if report_identity {
            let next = self
                .profile
                .modes
                .iter()
                .find(|m| {
                    m.reports.iter().any(|r| {
                        (r.input, r.kind, r.channel, r.number, r.value)
                            == (port, kind, channel, number, value)
                    })
                })
                .map(|m| m.name.clone());
            return self.changed_mode(next);
        }
        if let Some(pending) = self.echoes.get_mut(&(port, bytes[0], number)) {
            pending.retain(|(_, when)| when.elapsed() < Duration::from_millis(500));
            if let Some(index) = pending.iter().position(|(expected, _)| *expected == value) {
                pending.remove(index);
                return Ok(());
            }
        }
        let released = kind == MidiKind::Note && ((bytes[0] & 0xf0) == 0x80 || value == 0);
        // Release physically-owned actions *before* evaluating the current layer.
        if released || (kind == MidiKind::Cc && value == 0) {
            let was_button = self.pressed.remove(&key);
            if let Some(owner) = self.held.remove(&key) {
                self.ap.borrow_mut().release(&owner)?;
            }
            // A CC button released after a layer switch is not movement of a newly
            // assigned absolute control at the same MIDI identity.
            if released || was_button {
                return Ok(());
            }
        }
        if !self.mode_confirmed() {
            return Ok(());
        }
        let binding = self
            .profile
            .bindings
            .iter()
            .find(|b| {
                b.input == port
                    && b.kind == kind
                    && b.channel == channel
                    && (b.number == number
                        || matches!(b.behavior, Behavior::Absolute14 { .. })
                            && b.number <= 31
                            && b.number + 32 == number)
                    && self.active(&b.mode)
                    && self.active_layer(&b.layer)
            })
            .cloned();
        let Some(binding) = binding else {
            return Ok(());
        };
        if matches!(
            &binding.behavior,
            Behavior::Button { .. }
                | Behavior::Toggle { .. }
                | Behavior::SelectDeck { .. }
                | Behavior::StepDeck { .. }
                | Behavior::CycleLayer {}
        ) {
            if kind == MidiKind::Note || kind == MidiKind::Cc {
                if value == 0
                    || binding
                        .press_value
                        .is_some_and(|expected| value != expected)
                {
                    return Ok(());
                }
                if !self.pressed.insert(key) {
                    return Ok(());
                }
            }
        }
        self.apply(key, binding, value)
    }

    fn tick(&mut self) -> Result<()> {
        if self.last_tick.elapsed() >= Duration::from_millis(self.profile.period_ms) {
            self.last_tick = Instant::now();
            let hooks = self.profile.periodic.clone();
            self.hooks(&hooks)?;
        }
        Ok(())
    }

    fn feedback(&mut self, addresses: &[&str]) -> Result<()> {
        {
            let ap = self.ap.borrow();
            for b in &self.profile.bindings {
                let target = match &b.behavior {
                    Behavior::Absolute { address, .. } | Behavior::Absolute14 { address, .. } => {
                        address
                    }
                    _ => continue,
                };
                let address = self.resolve(target, b.deck_side).to_ascii_lowercase();
                if !addresses.contains(&address.as_str()) {
                    continue;
                }
                let key = (b.input, b.kind, b.channel, b.number);
                // Keep capture through delayed reports. Invalidate all affected
                // controls before refresh synchronizes endless encoders once.
                if ap.value(&address).is_none() || !ap.control_owned(&self.owner(key), &address) {
                    self.captured.remove(&(key, address));
                }
            }
        }
        self.refresh()
    }

    fn status(&self, verbose: bool) -> String {
        let mut status = format!("mode={}", self.mode.as_deref().unwrap_or("unknown"));
        if let Some(layer) = self.profile.layers.get(self.layer) {
            status.push_str(&format!(
                " | layer={} | deck={}",
                layer.name, self.selected[0]
            ));
        }
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
        for (_, owner) in std::mem::take(&mut self.held) {
            if let Err(e) = self.ap.borrow_mut().release(&owner) {
                eprintln!("{}: release: {e}", self.name);
            }
        }
        let hooks = self.profile.shutdown.clone();
        for hook in &hooks {
            if let Err(e) = self.emit(hook.output, &hook.bytes, false, None) {
                eprintln!("{}: shutdown: {e}", self.name);
            }
        }
    }
}
