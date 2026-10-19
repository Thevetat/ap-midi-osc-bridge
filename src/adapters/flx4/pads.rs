//! Inline FLX4 pad banks in the current device profile.
use crate::ap::SlipTiming;
use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    /// OSC address; {deck} expands to the selected deck (1–4).
    pub address: String,
    pub press: i32,
    #[serde(default)]
    pub release: Option<i32>,
    #[serde(default)]
    pub slip_timing: Option<SlipTiming>,
    #[serde(default)]
    pub toggle: bool,
}

impl Action {
    pub fn resolved(&self, deck: usize) -> Self {
        Self {
            address: self.address.replace("{deck}", &(deck + 1).to_string()),
            ..self.clone()
        }
    }

    /// Slip-loop actions must state their timing explicitly.
    pub fn validate(&self) -> Result<()> {
        if !crate::profile::valid_address(&self.address) {
            bail!("Invalid OSC address {:?}; only {{deck}} substitution and literal /Syndicate routes are supported", self.address);
        }
        let expanded = self.address.replace("{deck}", "1");
        if let Some(deck) = expanded.strip_suffix("/SetCueLoop") {
            if !matches!(
                deck,
                "/Syndicate/Deck/1"
                    | "/Syndicate/Deck/2"
                    | "/Syndicate/Deck/3"
                    | "/Syndicate/Deck/4"
            ) || !(1..=8).contains(&self.press)
                || self.release.is_some()
                || self.toggle
                || self.slip_timing.is_some()
            {
                bail!(
                    "SetCueLoop requires a deck route, cue 1..8 and no release/toggle/slip_timing"
                );
            }
        }
        if self.toggle {
            let valid = expanded.rsplit_once('/').is_some_and(|(deck, route)| {
                matches!(
                    deck,
                    "/Syndicate/Deck/1"
                        | "/Syndicate/Deck/2"
                        | "/Syndicate/Deck/3"
                        | "/Syndicate/Deck/4"
                ) && matches!(route, "SlipDotted" | "SlipTriplet")
            });
            if !valid || self.press != 1 || self.release.is_some() || self.slip_timing.is_some() {
                bail!("toggle requires a deck SlipDotted/SlipTriplet route, press 1 and no release/slip_timing");
            }
        }
        if let Some((deck, length)) = expanded.split_once("/SlipLoop/") {
            if !matches!(
                deck,
                "/Syndicate/Deck/1"
                    | "/Syndicate/Deck/2"
                    | "/Syndicate/Deck/3"
                    | "/Syndicate/Deck/4"
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
            if self.slip_timing.is_none() {
                bail!("Slip loops require explicit slip_timing: straight, dotted, triplet or selected");
            }
        } else if self.slip_timing.is_some() {
            bail!("slip_timing requires a SlipLoop route");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PadLayout {
    pub name: String,
    pub pads: Vec<Option<Action>>,
    /// Omit to reuse pads; null entries explicitly leave shifted pads unassigned.
    #[serde(default)]
    pub shifted_pads: Option<Vec<Option<Action>>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bank {
    pub name: String,
    pub pads: Vec<Option<Action>>,
    #[serde(default)]
    pub shifted_pads: Option<Vec<Option<Action>>>,
    #[serde(default)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pads {
    pub banks: Vec<Bank>,
}
impl Pads {
    pub fn validate(&self) -> Result<()> {
        if self.banks.len() != 8 {
            bail!("FLX4 pads must have exactly eight banks");
        }
        for bank in &self.banks {
            for player in bank.for_player.keys() {
                if !matches!(
                    player.as_str(),
                    "track" | "arrangement" | "clip" | "sampler"
                ) {
                    bail!("Unknown player override {player:?}");
                }
            }
            let layouts = std::iter::once((&bank.name, &bank.pads, &bank.shifted_pads)).chain(
                bank.for_player
                    .values()
                    .map(|layout| (&layout.name, &layout.pads, &layout.shifted_pads)),
            );
            for (name, pads, shifted) in layouts {
                if pads.len() != 8 || shifted.as_ref().is_some_and(|pads| pads.len() != 8) {
                    bail!("Layout {name:?} must have eight pads");
                }
                for action in pads
                    .iter()
                    .chain(shifted.iter().flatten())
                    .filter_map(Option::as_ref)
                {
                    action.validate()?;
                }
            }
        }
        Ok(())
    }
}
