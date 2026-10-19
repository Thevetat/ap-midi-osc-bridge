//! Device identity and declarative adapter configuration. Channels are one-based;
//! port indexes are zero-based.
//! Bindings may be global or scoped to a reported mode. A mode with reports starts unknown
//! until the hardware reports it; sending a selection hook does not confirm a mode.
use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

fn period() -> u64 {
    200
}
fn yes() -> bool {
    true
}
fn max_value() -> u8 {
    127
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceProfile {
    pub version: u32,
    pub name: String,
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    pub adapter: crate::adapters::Config,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclarativeConfig {
    #[serde(default)]
    pub startup: Vec<MidiMessage>,
    #[serde(default)]
    pub periodic: Vec<MidiMessage>,
    #[serde(default = "period")]
    pub period_ms: u64,
    #[serde(default)]
    pub shutdown: Vec<MidiMessage>,
    #[serde(default)]
    pub modes: Vec<Mode>,
    /// Independent software layers; the first is selected on connection.
    #[serde(default)]
    pub layers: Vec<Layer>,
    #[serde(default)]
    pub deck_feedback: Vec<DeckFeedback>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
    #[serde(default)]
    pub feedback: Vec<Feedback>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MidiMessage {
    pub output: usize,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    #[serde(default)]
    pub on_enter: Vec<MidiMessage>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeckFeedback {
    pub deck: usize,
    #[serde(default)]
    pub deck_side: usize,
    pub messages: Vec<MidiMessage>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mode {
    pub name: String,
    #[serde(default)]
    pub reports: Vec<ModeReport>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeReport {
    pub input: usize,
    pub kind: MidiKind,
    pub channel: u8,
    pub number: u8,
    pub value: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiKind {
    Note,
    Cc,
}

/// A single software layer or a nonempty set of layers sharing one mapping.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum LayerScope {
    One(String),
    Many(Vec<String>),
}

impl LayerScope {
    pub fn names(&self) -> &[String] {
        match self {
            Self::One(name) => std::slice::from_ref(name),
            Self::Many(names) => names,
        }
    }

    pub fn contains(&self, name: &str) -> bool {
        self.names().iter().any(|layer| layer == name)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub input: usize,
    pub kind: MidiKind,
    pub channel: u8,
    pub number: u8,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub layer: Option<LayerScope>,
    /// Optional exact button press value (e.g. 127, excluding LED colour echoes).
    #[serde(default)]
    pub press_value: Option<u8>,
    #[serde(default)]
    pub deck_side: usize,
    pub behavior: Behavior,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Behavior {
    /// Press alone, or a press/release pair owned by this physical button.
    Button {
        address: String,
        #[serde(default = "max_int")]
        press: i32,
        #[serde(default)]
        release: Option<i32>,
    },
    /// One-shot unit macro write on press; release has no action.
    MacroButton {
        address: String,
        value: f32,
    },
    /// Toggle only using AP's confirmed switch value.
    Toggle {
        address: String,
    },
    /// First report establishes a physical baseline; pickup needs confirmed AP feedback.
    Absolute {
        address: String,
        #[serde(default)]
        bipolar: bool,
        #[serde(default = "yes")]
        pickup: bool,
    },
    /// Output-only unit CC: physical movement to exactly zero is required to arm.
    AbsoluteDryHome {
        address: String,
    },
    /// MSB at number, LSB at number+32, same MIDI channel. A fresh MSB is required per LSB.
    #[serde(rename = "absolute_14")]
    Absolute14 {
        address: String,
        #[serde(default)]
        bipolar: bool,
        #[serde(default = "yes")]
        pickup: bool,
    },
    /// Two's-complement relative CC (1..63 up, 65..127 down).
    Relative {
        up: String,
        down: String,
    },
    /// Absolute CC centered at `center`: one AP step per physical report, then recenter.
    Recenter {
        up: String,
        down: String,
        #[serde(default = "center")]
        center: u8,
        output: usize,
    },
    SelectDeck {
        deck: usize,
    },
    /// Previous/next deck, clamped to 1..4, without changing AP's UI or transport.
    StepDeck {
        delta: i8,
    },
    CycleLayer {},
}
fn max_int() -> i32 {
    1
}
fn center() -> u8 {
    64
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FeedbackKind {
    Led {
        #[serde(default = "max_value")]
        on: u8,
        #[serde(default)]
        off: u8,
        #[serde(default)]
        unknown: Option<u8>,
    },
    /// Sends the last confirmed AP position to a DAW encoder (0..1 → 0..127).
    Position {},
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feedback {
    pub address: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub layer: Option<LayerScope>,
    pub output: usize,
    /// Input port that can echo the outgoing message. Single-input devices infer port 0.
    #[serde(default)]
    pub echo_input: Option<usize>,
    pub midi_kind: MidiKind,
    pub channel: u8,
    pub number: u8,
    #[serde(default)]
    pub deck_side: usize,
    pub render: FeedbackKind,
}

pub fn valid_address(address: &str) -> bool {
    if !address.starts_with("/Syndicate/") || address.chars().any(char::is_whitespace) {
        return false;
    }
    let expanded = address.replace("{deck}", "1");
    !expanded.contains(['{', '}', '*', '?', '[', ']', ',', '\0'])
}

fn midi_valid(bytes: &[u8]) -> bool {
    if bytes.len() == 3
        && matches!(bytes[0] & 0xf0, 0x80 | 0x90 | 0xa0 | 0xb0 | 0xe0)
        && bytes[1..].iter().all(|v| *v < 128)
    {
        return true;
    }
    bytes.len() >= 3
        && bytes.len() <= 64
        && bytes[0] == 0xf0
        && bytes.last() == Some(&0xf7)
        && bytes[1..bytes.len() - 1].iter().all(|v| *v < 128)
}

impl DeviceProfile {
    /// Common preflight; adapter configuration is validated by the loader.
    pub fn validate(&self) -> Result<()> {
        if self.version != 2 {
            bail!("Mapping {}: expected version 2", self.name);
        }
        if self.name.trim().is_empty() || self.name.len() > 100 {
            bail!("Mapping name must be 1..100 characters");
        }
        if self.inputs.is_empty()
            || self.inputs.len() > 8
            || self.outputs.len() > 8
            || self
                .inputs
                .iter()
                .chain(&self.outputs)
                .any(|s| s.trim().is_empty() || s.len() > 200)
        {
            bail!("Mapping requires 1..8 nonempty inputs and 0..8 nonempty outputs");
        }
        Ok(())
    }
}

impl DeclarativeConfig {
    /// Pure preflight before opening ports; port counts belong to the device identity.
    pub fn validate(&self, inputs: usize, outputs: usize) -> Result<()> {
        if self.bindings.len() > 1024 || self.feedback.len() > 512 || self.modes.len() > 32 {
            bail!("Mapping exceeds binding/feedback/mode limits (1024/512/32)");
        }
        if self.layers.len() > 16 || self.deck_feedback.len() > 16 {
            bail!("Mapping exceeds layer/deck-feedback limits (16 each)");
        }
        let mut layers = HashSet::new();
        for layer in &self.layers {
            if layer.name.trim().is_empty()
                || layer.name.len() > 80
                || !layers.insert(layer.name.clone())
            {
                bail!("Layer name must be unique and 1..80 bytes");
            }
        }
        let mut decks = HashSet::new();
        for feedback in &self.deck_feedback {
            if !(1..=4).contains(&feedback.deck)
                || feedback.deck_side > 3
                || !decks.insert((feedback.deck_side, feedback.deck))
            {
                bail!("Deck feedback needs a unique deck_side/deck pair (side 0..3, deck 1..4)");
            }
        }
        for (label, messages) in [
            ("startup", &self.startup),
            ("periodic", &self.periodic),
            ("shutdown", &self.shutdown),
        ]
        .into_iter()
        .chain(self.layers.iter().map(|l| ("layer on_enter", &l.on_enter)))
        .chain(
            self.deck_feedback
                .iter()
                .map(|f| ("deck feedback", &f.messages)),
        ) {
            if messages.len() > 32 {
                bail!("{label} hook exceeds 32 MIDI messages");
            }
            for message in messages {
                if message.output >= outputs || !midi_valid(&message.bytes) {
                    bail!("{label} MIDI hook needs a valid output and a three-byte MIDI message or bounded (<=64 byte) SysEx");
                }
            }
        }
        if !self.periodic.is_empty() && !(20..=60_000).contains(&self.period_ms) {
            bail!("period_ms must be 20..60000 with periodic MIDI hooks");
        }
        if self.modes.len() > 1 && self.modes.iter().any(|m| m.reports.is_empty()) {
            bail!(
                "Multiple modes require report messages for every mode (no select_mode capability)"
            );
        }
        let mut modes = HashSet::new();
        let mut reports = HashSet::new();
        let mut report_controls = HashSet::new();
        for mode in &self.modes {
            if mode.name.trim().is_empty()
                || mode.name.len() > 80
                || mode.reports.len() > 64
                || !modes.insert(&mode.name)
            {
                bail!("Mode name must be unique and <=80 bytes with <=64 reports");
            }
            for report in &mode.reports {
                if report.input >= inputs
                    || !valid_control(report.channel, report.number)
                    || report.value > 127
                    || !reports.insert((
                        report.input,
                        report.kind,
                        report.channel,
                        report.number,
                        report.value,
                    ))
                {
                    bail!("Invalid or duplicate mode report in {}", mode.name);
                }
                report_controls.insert((report.input, report.kind, report.channel, report.number));
            }
        }
        let mut controls = Vec::new();
        for binding in &self.bindings {
            if binding.input >= inputs
                || !valid_control(binding.channel, binding.number)
                || binding.deck_side > 3
                || binding.mode.as_ref().is_some_and(|m| !modes.contains(m))
                || !valid_layer_scope(&binding.layer, &layers)
            {
                bail!("Invalid binding input/channel/number/deck_side/mode/layer");
            }
            if let Some(value) = binding.press_value {
                if !(1..=127).contains(&value)
                    || !matches!(
                        binding.behavior,
                        Behavior::Button { .. }
                            | Behavior::MacroButton { .. }
                            | Behavior::Toggle { .. }
                            | Behavior::SelectDeck { .. }
                            | Behavior::StepDeck { .. }
                            | Behavior::CycleLayer {}
                    )
                {
                    bail!("press_value requires a button behavior and a value in 1..127");
                }
            }
            let mut numbers = vec![binding.number];
            if matches!(binding.behavior, Behavior::Absolute14 { .. }) && binding.number <= 31 {
                numbers.push(binding.number + 32);
            }
            for number in numbers {
                if report_controls.contains(&(binding.input, binding.kind, binding.channel, number))
                {
                    bail!("Binding conflicts with mode-report control: input {} channel {} number {number}", binding.input, binding.channel);
                }
                let identity = (binding.input, binding.kind, binding.channel, number);
                if controls.iter().any(|(previous, mode, layer)| {
                    *previous == identity
                        && scopes_overlap(mode, &binding.mode)
                        && layers_overlap(layer, &binding.layer)
                }) {
                    bail!(
                        "Overlapping MIDI binding at input {} channel {} number {number}",
                        binding.input,
                        binding.channel
                    );
                }
                controls.push((identity, binding.mode.clone(), binding.layer.clone()));
            }
            let addresses: Vec<&str> = match &binding.behavior {
                Behavior::Button { address, .. }
                | Behavior::MacroButton { address, .. }
                | Behavior::Toggle { address }
                | Behavior::Absolute { address, .. }
                | Behavior::AbsoluteDryHome { address }
                | Behavior::Absolute14 { address, .. } => vec![address],
                Behavior::Relative { up, down } | Behavior::Recenter { up, down, .. } => {
                    vec![up, down]
                }
                Behavior::SelectDeck { deck } => {
                    if !(1..=4).contains(deck) {
                        bail!("SelectDeck must be 1..4");
                    }
                    vec![]
                }
                Behavior::StepDeck { delta } => {
                    if !matches!(delta, -1 | 1) {
                        bail!("StepDeck delta must be -1 or 1");
                    }
                    vec![]
                }
                Behavior::CycleLayer {} => {
                    if self.layers.len() < 2 {
                        bail!("CycleLayer requires at least two layers");
                    }
                    vec![]
                }
            };
            if addresses.iter().any(|a| !valid_address(a)) {
                bail!("Invalid AP binding address");
            }
            if addresses.iter().any(|a| {
                let a = a.to_ascii_lowercase();
                a.contains("/sliploop/")
                    || a.ends_with("/slipdotted")
                    || a.ends_with("/sliptriplet")
            }) {
                bail!("Declarative bindings do not yet support slip timing/ownership; use a protocol capability");
            }
            match &binding.behavior {
                Behavior::Button { .. }
                | Behavior::MacroButton { .. }
                | Behavior::Toggle { .. }
                | Behavior::SelectDeck { .. } => {}
                Behavior::Absolute { .. }
                | Behavior::AbsoluteDryHome { .. }
                | Behavior::Relative { .. }
                | Behavior::Recenter { .. }
                | Behavior::Absolute14 { .. }
                    if binding.kind != MidiKind::Cc =>
                {
                    bail!("Analog behavior requires CC input")
                }
                _ => {}
            }
            match &binding.behavior {
                Behavior::AbsoluteDryHome { address } if !valid_macro_address(address) => {
                    bail!("absolute_dry_home requires /Syndicate/Mixer/CH1..4/FX/Macro/1..8 (CH{{deck}} allowed)");
                }
                Behavior::MacroButton { address, value }
                    if !valid_macro_address(address)
                        || !value.is_finite()
                        || !(0.0..=1.0).contains(value) =>
                {
                    bail!("macro_button requires a CH1..4 FX Macro1..8 address and finite value in 0..1");
                }
                _ => {}
            }
            if matches!(binding.behavior, Behavior::Absolute14 { .. }) && binding.number > 31 {
                bail!("14-bit control MSB number must be 0..31 (LSB is number+32)");
            }
            if let Behavior::Recenter { output, center, .. } = &binding.behavior {
                if *output >= outputs || *center > 127 {
                    bail!("Recenter requires a valid MIDI output and center <=127");
                }
            }
        }
        let mut destinations = Vec::new();
        for f in &self.feedback {
            if f.output >= outputs
                || f.echo_input.is_some_and(|i| i >= inputs)
                || !valid_control(f.channel, f.number)
                || f.deck_side > 3
                || !valid_address(&f.address)
                || f.mode.as_ref().is_some_and(|m| !modes.contains(m))
                || !valid_layer_scope(&f.layer, &layers)
            {
                bail!("Invalid feedback address/output/control/mode/layer");
            }
            let identity = (f.output, f.midi_kind, f.channel, f.number);
            if destinations.iter().any(|(previous, mode, layer)| {
                *previous == identity
                    && scopes_overlap(mode, &f.mode)
                    && layers_overlap(layer, &f.layer)
            }) {
                bail!(
                    "Overlapping feedback MIDI destination at output {} channel {} number {}",
                    f.output,
                    f.channel,
                    f.number
                );
            }
            destinations.push((identity, f.mode.clone(), f.layer.clone()));
            if matches!(f.render, FeedbackKind::Position {})
                && self.bindings.iter().any(|b| match &b.behavior {
                    Behavior::AbsoluteDryHome { address } => (1..=4).any(|deck| {
                        address
                            .replace("{deck}", &deck.to_string())
                            .eq_ignore_ascii_case(&f.address.replace("{deck}", &deck.to_string()))
                    }),
                    _ => false,
                })
            {
                bail!("absolute_dry_home cannot have Position feedback");
            }
            if matches!(f.render, FeedbackKind::Position {}) && f.midi_kind != MidiKind::Cc {
                bail!("Position feedback requires a CC output");
            }
            if let FeedbackKind::Led { on, off, unknown } = &f.render {
                if *on > 127 || *off > 127 || unknown.is_some_and(|v| v > 127) {
                    bail!("LED values must be 0..127");
                }
            }
        }
        // Never hide conflicting AP scales by deduplicating on address alone.
        let mut scales = HashMap::new();
        for (address, kind) in self.feedback_interests() {
            if let Some(previous) = scales.insert(address.to_ascii_lowercase(), kind) {
                if previous != kind {
                    bail!("Conflicting AP feedback scales for {address}: {previous:?} vs {kind:?}");
                }
            }
        }
        Ok(())
    }

    /// Routes to register with the shared AP feedback client before starting a session.
    pub fn feedback_interests(&self) -> Vec<(String, crate::ap::FeedbackKind)> {
        let mut routes = Vec::new();
        let mut add = |address: &str, kind| {
            for deck in 1..=4 {
                let resolved = address.replace("{deck}", &deck.to_string());
                routes.push((resolved, kind));
                if !address.contains("{deck}") {
                    break;
                }
            }
        };
        for binding in &self.bindings {
            match &binding.behavior {
                Behavior::Absolute {
                    address, bipolar, ..
                }
                | Behavior::Absolute14 {
                    address, bipolar, ..
                } => add(
                    address,
                    if *bipolar {
                        crate::ap::FeedbackKind::Bipolar
                    } else {
                        crate::ap::FeedbackKind::Unit
                    },
                ),
                Behavior::AbsoluteDryHome { address } | Behavior::MacroButton { address, .. } => {
                    add(address, crate::ap::FeedbackKind::Unit)
                }
                Behavior::Toggle { address } => add(address, crate::ap::FeedbackKind::Switch),
                _ => {}
            }
        }
        for f in &self.feedback {
            let kind = match &f.render {
                FeedbackKind::Led { .. } => crate::ap::FeedbackKind::Switch,
                FeedbackKind::Position {} => {
                    // Position feedback uses the wire scale of its matching absolute binding.
                    if self.bindings.iter().any(|b| match &b.behavior {
                        Behavior::Absolute {
                            address,
                            bipolar: true,
                            ..
                        }
                        | Behavior::Absolute14 {
                            address,
                            bipolar: true,
                            ..
                        } => address.eq_ignore_ascii_case(&f.address),
                        _ => false,
                    }) {
                        crate::ap::FeedbackKind::Bipolar
                    } else {
                        crate::ap::FeedbackKind::Unit
                    }
                }
            };
            add(&f.address, kind);
        }
        routes
    }
}
fn scopes_overlap(a: &Option<String>, b: &Option<String>) -> bool {
    a.is_none() || b.is_none() || a == b
}

fn valid_layer_scope(scope: &Option<LayerScope>, declared: &HashSet<String>) -> bool {
    scope.as_ref().is_none_or(|scope| {
        let names = scope.names();
        names.len() <= 16
            && !names.is_empty()
            && names.iter().all(|name| declared.contains(name))
            && names.iter().collect::<HashSet<_>>().len() == names.len()
    })
}

fn layers_overlap(a: &Option<LayerScope>, b: &Option<LayerScope>) -> bool {
    a.as_ref()
        .zip(b.as_ref())
        .is_none_or(|(a, b)| a.names().iter().any(|name| b.contains(name)))
}

fn valid_macro_address(address: &str) -> bool {
    let valid = |address: &str| {
        let Some(rest) = address.strip_prefix("/Syndicate/Mixer/CH") else {
            return false;
        };
        let Some((channel, macro_number)) = rest.split_once("/FX/Macro/") else {
            return false;
        };
        matches!(channel, "1" | "2" | "3" | "4")
            && matches!(macro_number, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8")
    };
    if address.contains("{deck}") {
        (1..=4).all(|deck| valid(&address.replace("{deck}", &deck.to_string())))
    } else {
        valid(address)
    }
}

fn valid_control(channel: u8, number: u8) -> bool {
    (1..=16).contains(&channel) && number <= 127
}
