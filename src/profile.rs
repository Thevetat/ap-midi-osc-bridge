//! Version-2 declarative MIDI device format. Channels are one-based; port indexes are zero-based.
//! Bindings may be global or scoped to a reported mode. A mode with reports starts unknown
//! until the hardware reports it; sending a selection hook does not confirm a mode.
use anyhow::{bail, Result};
use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn declarative() -> String {
    "declarative".into()
}
fn period() -> u64 {
    200
}
fn yes() -> bool {
    true
}
fn max_value() -> u8 {
    127
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceProfile {
    pub version: u32,
    pub name: String,
    #[serde(default = "declarative")]
    pub protocol: String,
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub protocol_config: Value,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MidiMessage {
    pub output: usize,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    #[serde(default)]
    pub on_enter: Vec<MidiMessage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeckFeedback {
    pub deck: usize,
    #[serde(default)]
    pub deck_side: usize,
    pub messages: Vec<MidiMessage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mode {
    pub name: String,
    #[serde(default)]
    pub reports: Vec<ModeReport>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModeReport {
    pub input: usize,
    pub kind: MidiKind,
    pub channel: u8,
    pub number: u8,
    pub value: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MidiKind {
    Note,
    Cc,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub input: usize,
    pub kind: MidiKind,
    pub channel: u8,
    pub number: u8,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub layer: Option<String>,
    /// Optional exact button press value (e.g. 127, excluding LED colour echoes).
    #[serde(default)]
    pub press_value: Option<u8>,
    #[serde(default)]
    pub deck_side: usize,
    pub behavior: Behavior,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    CycleLayer,
}
fn max_int() -> i32 {
    1
}
fn center() -> u8 {
    64
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    Position,
}

#[derive(Clone, Debug, Serialize)]
pub struct Feedback {
    pub address: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub layer: Option<String>,
    pub output: usize,
    /// Input port that can echo the outgoing message. Single-input devices infer port 0.
    #[serde(default)]
    pub echo_input: Option<usize>,
    pub midi_kind: MidiKind,
    pub channel: u8,
    pub number: u8,
    #[serde(default)]
    pub deck_side: usize,
    #[serde(flatten)]
    pub kind: FeedbackKind,
}

// serde's flatten and deny_unknown_fields cannot be combined reliably. Check keys
// explicitly, then deserialize the same shape through a private wire struct.
impl<'de> Deserialize<'de> for Feedback {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            address: String,
            #[serde(default)]
            mode: Option<String>,
            #[serde(default)]
            layer: Option<String>,
            output: usize,
            midi_kind: MidiKind,
            channel: u8,
            number: u8,
            #[serde(default)]
            echo_input: Option<usize>,
            #[serde(default)]
            deck_side: usize,
            #[serde(flatten)]
            kind: FeedbackKind,
        }
        let value = Value::deserialize(deserializer)?;
        let object = value
            .as_object()
            .ok_or_else(|| de::Error::custom("feedback must be an object"))?;
        for key in object.keys() {
            if !matches!(
                key.as_str(),
                "address"
                    | "mode"
                    | "layer"
                    | "output"
                    | "midi_kind"
                    | "channel"
                    | "number"
                    | "echo_input"
                    | "deck_side"
                    | "kind"
                    | "on"
                    | "off"
                    | "unknown"
            ) {
                return Err(de::Error::custom(format!("unknown feedback field {key:?}")));
            }
        }
        let kind = object.get("kind").and_then(Value::as_str);
        if !matches!(kind, Some("led" | "position")) {
            return Err(de::Error::custom("feedback kind must be led or position"));
        }
        if kind == Some("position")
            && object
                .keys()
                .any(|key| matches!(key.as_str(), "on" | "off" | "unknown"))
        {
            return Err(de::Error::custom(
                "position feedback cannot specify LED colours",
            ));
        }
        let w: Wire = serde_json::from_value(value).map_err(de::Error::custom)?;
        Ok(Self {
            address: w.address,
            mode: w.mode,
            layer: w.layer,
            output: w.output,
            midi_kind: w.midi_kind,
            channel: w.channel,
            number: w.number,
            echo_input: w.echo_input,
            deck_side: w.deck_side,
            kind: w.kind,
        })
    }
}

pub fn valid_address(address: &str) -> bool {
    if !address.starts_with("/AutoPilot/") || address.chars().any(char::is_whitespace) {
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
    /// Pure preflight: no MIDI or AP side effects. Unsupported protocol implementations must
    /// reject their own protocol_config before opening ports.
    pub fn validate(&self) -> Result<()> {
        if self.version != 2 {
            bail!("Mapping {}: expected version 2", self.name);
        }
        if self.name.trim().is_empty() || self.name.len() > 100 {
            bail!("Mapping name must be 1..100 characters");
        }
        if self.protocol.trim().is_empty() {
            bail!("Mapping protocol cannot be empty");
        }
        if self.protocol == "declarative" && !self.protocol_config.is_null() {
            bail!("Declarative protocol does not support protocol_config; remove it or choose a supported protocol");
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
        if self.bindings.len() > 1024 || self.feedback.len() > 512 || self.modes.len() > 32 {
            bail!("Mapping exceeds binding/feedback/mode limits (1024/512/32)");
        }
        if self.layers.len() > 16 || self.deck_feedback.len() > 16 {
            bail!("Mapping exceeds layer/deck-feedback limits (16 each)");
        }
        let mut layers = HashSet::new();
        for layer in &self.layers {
            if layer.name.trim().is_empty() || layer.name.len() > 80 || !layers.insert(&layer.name)
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
                if message.output >= self.outputs.len() || !midi_valid(&message.bytes) {
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
                if report.input >= self.inputs.len()
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
            if binding.input >= self.inputs.len()
                || !valid_control(binding.channel, binding.number)
                || binding.deck_side > 3
                || binding.mode.as_ref().is_some_and(|m| !modes.contains(m))
                || binding.layer.as_ref().is_some_and(|l| !layers.contains(l))
            {
                bail!("Invalid binding input/channel/number/deck_side/mode/layer");
            }
            if let Some(value) = binding.press_value {
                if !(1..=127).contains(&value)
                    || !matches!(
                        binding.behavior,
                        Behavior::Button { .. }
                            | Behavior::Toggle { .. }
                            | Behavior::SelectDeck { .. }
                            | Behavior::StepDeck { .. }
                            | Behavior::CycleLayer
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
                        && scopes_overlap(layer, &binding.layer)
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
                | Behavior::Toggle { address }
                | Behavior::Absolute { address, .. }
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
                Behavior::CycleLayer => {
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
                Behavior::Button { .. } | Behavior::Toggle { .. } | Behavior::SelectDeck { .. } => {
                }
                Behavior::Absolute { .. }
                | Behavior::Relative { .. }
                | Behavior::Recenter { .. }
                | Behavior::Absolute14 { .. }
                    if binding.kind != MidiKind::Cc =>
                {
                    bail!("Analog behavior requires CC input")
                }
                _ => {}
            }
            if matches!(binding.behavior, Behavior::Absolute14 { .. }) && binding.number > 31 {
                bail!("14-bit control MSB number must be 0..31 (LSB is number+32)");
            }
            if let Behavior::Recenter { output, center, .. } = &binding.behavior {
                if *output >= self.outputs.len() || *center > 127 {
                    bail!("Recenter requires a valid MIDI output and center <=127");
                }
            }
        }
        let mut outputs = Vec::new();
        for f in &self.feedback {
            if f.output >= self.outputs.len()
                || f.echo_input.is_some_and(|i| i >= self.inputs.len())
                || !valid_control(f.channel, f.number)
                || f.deck_side > 3
                || !valid_address(&f.address)
                || f.mode.as_ref().is_some_and(|m| !modes.contains(m))
                || f.layer.as_ref().is_some_and(|l| !layers.contains(l))
            {
                bail!("Invalid feedback address/output/control/mode/layer");
            }
            let identity = (f.output, f.midi_kind, f.channel, f.number);
            if outputs.iter().any(|(previous, mode, layer)| {
                *previous == identity
                    && scopes_overlap(mode, &f.mode)
                    && scopes_overlap(layer, &f.layer)
            }) {
                bail!(
                    "Overlapping feedback MIDI destination at output {} channel {} number {}",
                    f.output,
                    f.channel,
                    f.number
                );
            }
            outputs.push((identity, f.mode.clone(), f.layer.clone()));
            if matches!(f.kind, FeedbackKind::Position) && f.midi_kind != MidiKind::Cc {
                bail!("Position feedback requires a CC output");
            }
            if let FeedbackKind::Led { on, off, unknown } = &f.kind {
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
                Behavior::Toggle { address } => add(address, crate::ap::FeedbackKind::Switch),
                _ => {}
            }
        }
        for f in &self.feedback {
            let kind = match &f.kind {
                FeedbackKind::Led { .. } => crate::ap::FeedbackKind::Switch,
                FeedbackKind::Position => {
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

fn valid_control(channel: u8, number: u8) -> bool {
    (1..=16).contains(&channel) && number <= 127
}
