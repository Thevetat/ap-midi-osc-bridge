//! Version-1 FLX4 pad layout compatibility. The bundled layout is editable JSON, not Rust defaults.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlipTiming {
    Straight,
    Dotted,
    Triplet,
    /// Use the deck's latched timing instead of a fixed per-pad override.
    Selected,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    /// OSC address; {deck} expands to the selected deck (1–4).
    pub address: String,
    pub press: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slip_timing: Option<SlipTiming>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub toggle: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}

impl Action {
    pub fn resolved(&self, deck: usize) -> Self {
        Self {
            address: self.address.replace("{deck}", &(deck + 1).to_string()),
            ..self.clone()
        }
    }

    /// Validate both inline FLX4 actions and version-1 pad overrides identically.
    /// Old slip-loop actions without explicit timing use Straight.
    pub fn validate(&mut self) -> Result<()> {
        if !crate::device_profile::valid_address(&self.address) {
            bail!("Invalid OSC address {:?}; only {{deck}} substitution and literal /AutoPilot routes are supported", self.address);
        }
        let expanded = self.address.replace("{deck}", "1");
        if self.toggle {
            let valid = expanded.rsplit_once('/').is_some_and(|(deck, route)| {
                matches!(
                    deck,
                    "/AutoPilot/Deck/1"
                        | "/AutoPilot/Deck/2"
                        | "/AutoPilot/Deck/3"
                        | "/AutoPilot/Deck/4"
                ) && matches!(route, "SlipDotted" | "SlipTriplet")
            });
            if !valid || self.press != 1 || self.release.is_some() || self.slip_timing.is_some() {
                bail!("toggle requires a deck SlipDotted/SlipTriplet route, press 1 and no release/slip_timing");
            }
        }
        if let Some((deck, length)) = expanded.split_once("/SlipLoop/") {
            if !matches!(
                deck,
                "/AutoPilot/Deck/1"
                    | "/AutoPilot/Deck/2"
                    | "/AutoPilot/Deck/3"
                    | "/AutoPilot/Deck/4"
            ) || !matches!(
                length,
                "ThirtySecond"
                    | "Sixteenth"
                    | "Eighth"
                    | "Quarter"
                    | "Half"
                    | "One"
                    | "Two"
                    | "Four"
            ) || self.press != 1
                || self.release != Some(0)
            {
                bail!("Slip loops require a deck SlipLoop route, press 1 and release 0");
            }
            self.slip_timing.get_or_insert(SlipTiming::Straight);
        } else if self.slip_timing.is_some() {
            bail!("slip_timing requires a SlipLoop route");
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PadLayout {
    pub name: String,
    pub pads: Vec<Option<Action>>,
    /// Omit to reuse pads; null explicitly leaves a shifted pad unassigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shifted_pads: Option<Vec<Option<Action>>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bank {
    pub name: String,
    pub pads: Vec<Option<Action>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shifted_pads: Option<Vec<Option<Action>>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub for_player: BTreeMap<String, PadLayout>,
}
impl Bank {
    pub fn name(&self, player: &str) -> &str {
        self.for_player
            .get(player)
            .map_or(&self.name, |layout| &layout.name)
    }
    pub fn action(&self, player: &str, pad: usize, shifted: bool) -> Option<&Action> {
        let (pads, shifted_pads) = self
            .for_player
            .get(player)
            .map_or((&self.pads, &self.shifted_pads), |layout| {
                (&layout.pads, &layout.shifted_pads)
            });
        if shifted {
            if let Some(pads) = shifted_pads {
                return pads[pad].as_ref();
            }
        }
        pads[pad].as_ref()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub banks: Vec<Bank>,
}
impl Profile {
    /// No implicit embedded layout: callers load a version-1 pad file or an inline object.
    pub fn load(path: &Path) -> Result<Self> {
        let mut profile: Self = serde_json::from_str(
            &std::fs::read_to_string(path)
                .with_context(|| format!("Read profile {}", path.display()))?,
        )?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&mut self) -> Result<()> {
        if self.version != 1 || self.banks.len() != 8 {
            bail!("Profile must have version 1 and exactly eight banks");
        }
        for bank in &mut self.banks {
            for player in bank.for_player.keys() {
                if !matches!(
                    player.as_str(),
                    "track" | "arrangement" | "clip" | "sampler"
                ) {
                    bail!("Unknown player override {player:?}");
                }
            }
            let layouts = std::iter::once((&bank.name, &mut bank.pads, &mut bank.shifted_pads))
                .chain(
                    bank.for_player
                        .values_mut()
                        .map(|layout| (&layout.name, &mut layout.pads, &mut layout.shifted_pads)),
                );
            for (name, pads, shifted) in layouts {
                if pads.len() != 8 || shifted.as_ref().is_some_and(|pads| pads.len() != 8) {
                    bail!("Layout {name:?} must have eight pads");
                }
                for action in pads
                    .iter_mut()
                    .chain(shifted.iter_mut().flatten())
                    .filter_map(Option::as_mut)
                {
                    action.validate()?;
                }
            }
        }
        Ok(())
    }
}
