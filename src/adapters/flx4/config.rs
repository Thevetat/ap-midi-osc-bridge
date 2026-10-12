//! FLX4-specific configuration and preflight validation.
use super::pads::{Action, Pads};
use crate::profile::{valid_address, DeviceProfile};
use anyhow::{bail, Context, Result};
use serde::{de::Error as _, Deserialize, Deserializer};
use std::collections::HashSet;

// Flatten + deny_unknown_fields is unsupported by serde. Decode the shared identity
// separately, then let the tagged behavior reject every unrecognized assignment key.
#[derive(Clone)]
pub(super) struct SideButton {
    pub(super) channel: u8,
    pub(super) note: u8,
    pub(super) side: Option<usize>,
    pub(super) behavior: SideBehavior,
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
pub(super) enum SideBehavior {
    Action { action: Action },
    Toggle { address: String },
    Bank { bank: usize },
    Shift,
    SelectDeck,
    Browse { address: String },
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Analog {
    pub(super) channel: u8,
    pub(super) cc: u8,
    #[serde(default)]
    pub(super) side: Option<usize>,
    pub(super) address: String,
    #[serde(default)]
    pub(super) shifted_address: Option<String>,
    #[serde(default)]
    pub(super) bipolar: bool,
    #[serde(default)]
    pub(super) direct: bool,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Encoder {
    pub(super) channel: u8,
    pub(super) cc: u8,
    pub(super) up: String,
    pub(super) down: String,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Led {
    pub(super) address: String,
    pub(super) channel: u8,
    pub(super) kind: LedKind,
    pub(super) number: u8,
    #[serde(default = "on_value")]
    pub(super) on: u8,
    #[serde(default)]
    pub(super) off: u8,
}
fn on_value() -> u8 {
    127
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum LedKind {
    Switch,
    Meter,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PadWiring {
    pub(super) channels: [u8; 4],
    pub(super) bank_notes: [u8; 8],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub(super) buttons: Vec<SideButton>,
    pub(super) analog: Vec<Analog>,
    pub(super) browser_encoder: Encoder,
    pub(super) leds: Vec<Led>,
    pub(super) pads: Pads,
    pub(super) pad_wiring: PadWiring,
}
fn parse(profile: &DeviceProfile) -> Result<Config> {
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
    valid_address(route)
}
/// Parse once and retain the validated configuration.
/// The caller validates the common device-profile fields first.
pub(super) fn load(profile: &DeviceProfile) -> Result<Config> {
    let cfg = parse(profile)?;
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
    cfg.pads.validate()?;
    if cfg.buttons.len() > 512 || cfg.analog.len() > 64 || cfg.leds.len() > 128 {
        bail!("FLX4 profile exceeds limits (512 buttons, 64 analog controls, 128 LEDs)");
    }
    let mut buttons = HashSet::new();
    for b in &cfg.buttons {
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
        match &b.behavior {
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
    Ok(cfg)
}
