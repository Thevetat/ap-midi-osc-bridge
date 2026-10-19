//! Performance FX controls on the FLX4's hardware FX section. No FX LED is
//! driven: neither the selector reports nor outgoing macro requests confirm AP state.
use crate::ap::{FeedbackKind, SharedAp};
use anyhow::{bail, Result};
use rosc::OscType;
use serde::Deserialize;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FxConfig {
    pub(super) wet_macro: u8,
    pub(super) enable_macro: u8,
}

impl FxConfig {
    pub(super) fn validate(&self) -> Result<()> {
        if !(1..=8).contains(&self.wet_macro)
            || !(1..=8).contains(&self.enable_macro)
            || self.wet_macro == self.enable_macro
        {
            bail!("FLX4 FX macros must be distinct numbers in 1..=8");
        }
        Ok(())
    }
}

pub(super) struct Fx {
    config: FxConfig,
    ap: SharedAp,
    wet_owner: String,
    selected: [usize; 2],
    // Each selector snapshot must report BOTH membership notes. A partial or
    // invalid snapshot never routes a macro command to a guessed target.
    reports: [Option<bool>; 2],
    committed: Option<[bool; 2]>,
    pressed: bool,
    msb: Option<u8>,
    last_position: Option<u16>,
    armed: bool,
    // AP tracks bridge writer identity, not confirmed macro values. Notice a
    // foreign writer once per takeover, rather than repeatedly resetting while
    // its ownership remains visible before our deliberate dry-home write.
    foreign_seen: [bool; 4],
}

impl Fx {
    pub(super) fn new(config: FxConfig, ap: SharedAp, owner: String) -> Result<Self> {
        config.validate()?;
        for deck in 1..=4 {
            ap.borrow_mut().register(
                &owner,
                &format!("/Syndicate/Mixer/CH{deck}/FX/Macro/{}", config.wet_macro),
                FeedbackKind::Unit,
            )?;
        }
        Ok(Self {
            config,
            ap,
            wet_owner: format!("{owner}:fx:wet"),
            selected: [0, 1],
            reports: [None; 2],
            committed: None,
            pressed: false,
            msb: None,
            last_position: None,
            armed: false,
            foreign_seen: [false; 4],
        })
    }

    fn wet_address(&self, deck: usize) -> String {
        format!(
            "/Syndicate/Mixer/CH{}/FX/Macro/{}",
            deck + 1,
            self.config.wet_macro
        )
    }

    fn targets(&self) -> Vec<usize> {
        let mut decks = Vec::with_capacity(2);
        if let Some(mask) = self.committed {
            for (side, active) in mask.into_iter().enumerate() {
                if active {
                    decks.push(self.selected[side]);
                }
            }
        }
        decks
    }

    fn active_targets(&self) -> Option<Vec<usize>> {
        if self.reports.iter().any(Option::is_none) {
            return None;
        }
        let decks = self.targets();
        (!decks.is_empty()).then_some(decks)
    }

    fn reset_wet(&mut self) {
        self.msb = None;
        self.last_position = None;
        self.armed = false;
        self.foreign_seen = [false; 4];
    }

    fn invalidate_selector(&mut self) {
        self.reports = [None; 2];
        self.committed = None;
        self.reset_wet();
    }

    fn selector(&mut self, kind: u8, channel: u8, number: u8, value: u8) {
        let side = match (channel, number) {
            (4, 0x10) => Some(0),
            (5, 0x11) => Some(1),
            _ => None, // The two opposite-channel reports must remain zero.
        };
        if kind != 0x90 || (side.is_none() && value != 0) || !matches!(value, 0 | 127) {
            self.invalidate_selector();
            return;
        }
        let Some(side) = side else { return };
        // The first membership note begins a new complete snapshot, even when
        // it repeats the current mask. A selector action during that interval
        // cannot observe a transient one-sided target.
        if self.reports.iter().all(Option::is_some) {
            self.reports = [None; 2];
            // Do not complete a knob pair across a selector snapshot gap.
            self.msb = None;
        }
        self.reports[side] = Some(value == 127);
        if let [Some(left), Some(right)] = self.reports {
            let mask = [left, right];
            if self.committed != Some(mask) {
                self.reset_wet();
            }
            self.committed = Some(mask);
        }
    }

    fn ownership_changed(&mut self, decks: &[usize]) {
        let mut takeover = false;
        for &deck in decks {
            let foreign = !self
                .ap
                .borrow()
                .control_owned(&self.wet_owner, &self.wet_address(deck));
            if foreign && !self.foreign_seen[deck] {
                takeover = true;
            }
            self.foreign_seen[deck] = foreign;
        }
        if takeover {
            // Keep foreign_seen: require a fresh physical baseline, then an
            // intentional trip to zero before taking over without macro echo.
            self.msb = None;
            self.last_position = None;
            self.armed = false;
        }
    }

    fn wet(&mut self, number: u8, value: u8) -> Result<()> {
        let Some(decks) = self.active_targets() else {
            return Ok(());
        };
        self.ownership_changed(&decks);
        if number == 0x02 {
            self.msb = Some(value);
            return Ok(());
        }
        let Some(msb) = self.msb.take() else {
            return Ok(());
        };
        let position = ((msb as u16) << 7) | value as u16;
        let Some(previous) = self.last_position.replace(position) else {
            // A first complete high/low pair is only a physical baseline.
            return Ok(());
        };
        if previous == position {
            return Ok(());
        }
        if !self.armed {
            if position != 0 {
                return Ok(());
            }
            self.armed = true;
        }
        let normalized = position as f32 / 16383.0;
        for deck in decks {
            self.ap.borrow_mut().send_control(
                &self.wet_owner,
                &self.wet_address(deck),
                OscType::Float(normalized),
            )?;
            self.foreign_seen[deck] = false;
        }
        Ok(())
    }

    /// Called on validated port-zero, three-byte MIDI before generic MIDI handling.
    pub(super) fn input(
        &mut self,
        bytes: &[u8],
        selected: [usize; 2],
        shifts: [bool; 2],
    ) -> Result<bool> {
        self.selection_changed(selected);
        let (kind, channel, number, value) = (bytes[0] & 0xf0, bytes[0] & 15, bytes[1], bytes[2]);
        if matches!(kind, 0x80 | 0x90)
            && matches!((channel, number), (4, 0x10 | 0x11) | (5, 0x10 | 0x11))
        {
            self.selector(kind, channel, number, value);
            return Ok(true);
        }
        if kind == 0xb0 && channel == 4 && matches!(number, 0x02 | 0x22) {
            self.wet(number, value)?;
            return Ok(true);
        }
        if matches!(kind, 0x80 | 0x90) && matches!(channel, 4 | 5) && matches!(number, 0x47 | 0x43)
        {
            if kind == 0x80 || value == 0 {
                self.pressed = false;
            } else if value == 127 && !self.pressed {
                self.pressed = true;
                if let Some(decks) = self.active_targets() {
                    let request = if number == 0x43 || shifts.into_iter().any(|s| s) {
                        0.0
                    } else {
                        1.0
                    };
                    for deck in decks {
                        self.ap.borrow().send(
                            &format!(
                                "/Syndicate/Mixer/CH{}/FX/Macro/{}",
                                deck + 1,
                                self.config.enable_macro
                            ),
                            OscType::Float(request),
                        )?;
                    }
                }
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn selection_changed(&mut self, selected: [usize; 2]) {
        if self.selected == selected {
            return;
        }
        let old = self.targets();
        self.selected = selected;
        if old != self.targets() {
            self.reset_wet();
        }
        // The physical ON/OFF latch deliberately survives deck retargeting.
    }

    pub(super) fn status(&self) -> String {
        let target = if self.reports.iter().any(Option::is_none) {
            "selector incomplete/unknown".to_string()
        } else {
            let decks = self.targets();
            if decks.is_empty() {
                "selector off (no targets)".to_string()
            } else {
                decks
                    .iter()
                    .map(|deck| format!("CH{}", deck + 1))
                    .collect::<Vec<_>>()
                    .join("+")
            }
        };
        format!(
            "FX: {target}, wet {} (dry-home); macro requests unconfirmed, LED unmanaged",
            if self.armed { "armed" } else { "unarmed" }
        )
    }
}
