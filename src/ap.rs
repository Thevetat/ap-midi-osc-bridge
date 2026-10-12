//! Shared OSC transport, confirmed AP state and held controls for all devices.
use crate::profile::SlipTiming;
use anyhow::{bail, Result};
use rosc::{OscMessage, OscPacket, OscType};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    net::{SocketAddr, UdpSocket},
    rc::Rc,
    time::{Duration, Instant},
};

pub type SharedAp = Rc<RefCell<Ap>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackKind {
    Unit,
    Bipolar,
    // Display-only levels may exceed unity. Clamp at the MIDI LED output,
    // not at the shared feedback cache; control/pickup scales stay strict.
    Meter,
    Switch,
    Text,
}
#[derive(Clone)]
struct Hold {
    owner: String,
    address: String,
    press: i32,
    release: i32,
    timing: Option<SlipTiming>,
}

pub struct Ap {
    socket: UdpSocket,
    destination: SocketAddr,
    kinds: HashMap<String, FeedbackKind>,
    registration_owner: String,
    interests: HashMap<String, HashSet<String>>,
    values: HashMap<String, f32>,
    texts: HashMap<String, String>,
    pending: HashMap<String, (f32, Instant)>,
    // Local continuous writes coordinate pickup immediately, before UDP feedback
    // arrives. These are ownership hints, never substituted for confirmed values.
    control_writers: HashMap<String, (String, bool)>,
    holds: Vec<Hold>,
    slip_modes: HashMap<String, (String, SlipTiming)>,
    warnings: HashSet<String>,
    verbose: bool,
    sent: Cell<u64>,
    accepted: u64,
}
impl Ap {
    pub fn new(socket: UdpSocket, destination: SocketAddr, verbose: bool) -> Self {
        Self {
            socket,
            destination,
            kinds: HashMap::new(),
            registration_owner: String::new(),
            interests: HashMap::new(),
            values: HashMap::new(),
            texts: HashMap::new(),
            pending: HashMap::new(),
            control_writers: HashMap::new(),
            holds: Vec::new(),
            slip_modes: HashMap::new(),
            warnings: HashSet::new(),
            verbose,
            sent: Cell::new(0),
            accepted: 0,
        }
    }
    pub fn registration_owner(&mut self, owner: &str) {
        self.registration_owner = owner.into();
    }
    /// Remove one disconnected device without discarding another device's state.
    pub fn remove_session(&mut self, owner: &str) {
        let prefix = format!("{owner}:");
        let held: Vec<_> = self
            .holds
            .iter()
            .filter(|hold| hold.owner.starts_with(&prefix))
            .map(|hold| hold.owner.clone())
            .collect();
        for control in held {
            if let Err(error) = self.release(&control) {
                eprintln!("[{owner}] release failed: {error}");
            }
        }
        // Cleanup is best effort. Stale owners must not survive a reconnect.
        self.holds.retain(|hold| !hold.owner.starts_with(&prefix));
        self.control_writers
            .retain(|_, (writer, _)| !writer.starts_with(&prefix));
        let mut unused = Vec::new();
        for (address, owners) in &mut self.interests {
            owners.remove(owner);
            if owners.is_empty() {
                unused.push(address.clone());
            }
        }
        for address in unused {
            self.interests.remove(&address);
            self.kinds.remove(&address);
            self.values.remove(&address);
            self.texts.remove(&address);
            self.pending.remove(&address);
            self.control_writers.remove(&address);
        }
        let unused_decks: Vec<_> = self
            .slip_modes
            .iter()
            .filter(|(key, (deck, _))| {
                self.active_slip(deck).is_none()
                    && !self
                        .interests
                        .keys()
                        .any(|route| route.starts_with(&format!("{key}/")))
            })
            .map(|(key, (deck, _))| (key.clone(), deck.clone()))
            .collect();
        for (key, deck) in unused_decks {
            if let Err(error) = self.timing(&deck, SlipTiming::Straight) {
                eprintln!("[{owner}] slip timing cleanup failed: {error}");
            }
            self.slip_modes.remove(&key);
        }
        if self.registration_owner == owner {
            self.registration_owner.clear();
        }
    }
    pub fn interested(&self, owner: &str, address: &str) -> bool {
        self.interests
            .get(address)
            .is_some_and(|owners| owners.contains(owner))
    }
    pub fn describe_shared_routes(&self) {
        let mut routes: Vec<_> = self
            .interests
            .iter()
            .filter(|(_, owners)| owners.len() > 1)
            .collect();
        routes.sort_by_key(|(address, _)| *address);
        for (address, owners) in routes {
            let mut owners: Vec<_> = owners.iter().map(String::as_str).collect();
            owners.sort();
            println!(
                "Shared AP feedback: {address} → {} (pickup/toggles coordinated)",
                owners.join(", ")
            );
        }
    }
    pub fn register(&mut self, address: &str, kind: FeedbackKind) -> Result<()> {
        if !crate::device_profile::valid_address(address) || address.contains('{') {
            bail!("Invalid resolved feedback route {address}");
        }
        let key = address.to_ascii_lowercase();
        if let Some(old) = self.kinds.get(&key) {
            if *old != kind {
                bail!("Conflicting feedback scales for {address}: {old:?} and {kind:?}");
            }
        } else {
            if self.kinds.len() >= 16384 {
                bail!("Configuration exceeds 16384 feedback routes");
            }
            self.kinds.insert(key.clone(), kind);
        }
        self.interests
            .entry(key)
            .or_default()
            .insert(self.registration_owner.clone());
        Ok(())
    }
    pub fn value(&self, address: &str) -> Option<f32> {
        self.values.get(&address.to_ascii_lowercase()).copied()
    }
    pub fn text(&self, address: &str) -> Option<String> {
        self.texts.get(&address.to_ascii_lowercase()).cloned()
    }
    pub fn counters(&self) -> (u64, u64) {
        (self.sent.get(), self.accepted)
    }
    pub fn send(&self, address: &str, value: OscType) -> Result<()> {
        if self.verbose {
            println!("OSC send {address} {value:?} (not AP confirmation)");
        }
        let packet = OscPacket::Message(OscMessage {
            addr: address.into(),
            args: vec![value],
        });
        self.socket
            .send_to(&rosc::encoder::encode(&packet)?, self.destination)?;
        self.sent.set(self.sent.get() + 1);
        Ok(())
    }
    pub fn control_owned(&self, owner: &str, address: &str) -> bool {
        self.control_writers
            .get(&address.to_ascii_lowercase())
            .is_none_or(|(writer, _)| writer == owner)
    }
    pub fn control_pending(&self, owner: &str, address: &str) -> bool {
        self.control_writers
            .get(&address.to_ascii_lowercase())
            .is_some_and(|(writer, pending)| writer != owner && *pending)
    }
    pub fn send_control(&mut self, owner: &str, address: &str, value: OscType) -> Result<()> {
        self.send(address, value)?;
        self.control_writers
            .insert(address.to_ascii_lowercase(), (owner.into(), true));
        Ok(())
    }
    fn warn(&mut self, message: String) {
        if self.warnings.insert(message.clone()) {
            eprintln!("{message}");
        }
    }
    pub fn toggle(&mut self, address: &str) -> Result<()> {
        let key = address.to_ascii_lowercase();
        if let Some((_, when)) = self.pending.get(&key) {
            if when.elapsed() >= Duration::from_secs(1) {
                self.pending.remove(&key);
                self.values.remove(&key);
                self.send("/AutoPilot/Feedback/Resend", OscType::Int(1))?;
            }
            self.warn(format!("{address}: toggle blocked waiting for AP confirmation; enable its subscription and press again after feedback arrives."));
            return Ok(());
        }
        let Some(current) = self.value(address) else {
            self.warn(format!("{address}: toggle blocked; no confirmed AP switch feedback. Enable its subscription."));
            return Ok(());
        };
        let next = if current >= 0.5 { 0 } else { 1 };
        self.send(address, OscType::Int(next))?;
        self.pending.insert(key, (next as f32, Instant::now()));
        Ok(())
    }
    /// Identities include session and physical input. Identical holds share an AP release.
    pub fn hold(&mut self, owner: &str, address: &str, press: i32, release: i32) -> Result<()> {
        if self.holds.iter().any(|h| h.owner == owner) {
            return Ok(());
        }
        if self.holds.iter().any(|h| {
            h.address.eq_ignore_ascii_case(address)
                && (h.press != press || h.release != release || h.timing.is_some())
        }) {
            bail!("Conflicting simultaneous held values for {address}");
        }
        let already = self
            .holds
            .iter()
            .any(|h| h.address.eq_ignore_ascii_case(address));
        self.holds.push(Hold {
            owner: owner.into(),
            address: address.into(),
            press,
            release,
            timing: None,
        });
        if !already {
            self.send(address, OscType::Int(press))?;
        }
        Ok(())
    }
    pub fn release(&mut self, owner: &str) -> Result<()> {
        let Some(i) = self.holds.iter().position(|h| h.owner == owner) else {
            return Ok(());
        };
        if self.holds[i].timing.is_some() {
            return self.slip_release(owner);
        }
        let held = &self.holds[i];
        if !self
            .holds
            .iter()
            .enumerate()
            .any(|(j, h)| j != i && h.address.eq_ignore_ascii_case(&held.address))
        {
            self.send(&held.address, OscType::Int(held.release))?;
        }
        self.holds.remove(i);
        Ok(())
    }
    pub fn slip_selected(&self, deck: &str) -> SlipTiming {
        self.slip_modes
            .get(&deck.to_ascii_lowercase())
            .map(|(_, timing)| *timing)
            .unwrap_or(SlipTiming::Straight)
    }
    fn timing(&self, deck: &str, timing: SlipTiming) -> Result<()> {
        let timing = if timing == SlipTiming::Selected {
            self.slip_selected(deck)
        } else {
            timing
        };
        self.send(&format!("{deck}/SlipDotted"), OscType::Int(0))?;
        self.send(&format!("{deck}/SlipTriplet"), OscType::Int(0))?;
        match timing {
            SlipTiming::Dotted => self.send(&format!("{deck}/SlipDotted"), OscType::Int(1)),
            SlipTiming::Triplet => self.send(&format!("{deck}/SlipTriplet"), OscType::Int(1)),
            _ => Ok(()),
        }
    }
    fn slip_deck(address: &str) -> Option<&str> {
        address.rsplit_once("/SlipLoop/").map(|(deck, _)| deck)
    }
    fn active_slip(&self, deck: &str) -> Option<&Hold> {
        self.holds.iter().rev().find(|h| {
            h.timing.is_some()
                && Self::slip_deck(&h.address).is_some_and(|d| d.eq_ignore_ascii_case(deck))
        })
    }
    pub fn slip_select(&mut self, deck: &str, timing: SlipTiming) -> Result<()> {
        if timing == SlipTiming::Selected {
            bail!("Selected is a slip binding policy, not a timing selection");
        }
        self.slip_modes
            .insert(deck.to_ascii_lowercase(), (deck.into(), timing));
        if let Some(active) = self.active_slip(deck) {
            if active.timing == Some(SlipTiming::Selected) {
                self.send(&active.address, OscType::Int(active.release))?;
                self.timing(deck, timing)?;
                self.send(&active.address, OscType::Int(active.press))?;
            }
        } else {
            self.timing(deck, timing)?;
        }
        Ok(())
    }
    pub fn slip_toggle(&mut self, deck: &str, timing: SlipTiming) -> Result<()> {
        self.slip_select(
            deck,
            if self.slip_selected(deck) == timing {
                SlipTiming::Straight
            } else {
                timing
            },
        )
    }
    pub fn slip_hold(
        &mut self,
        owner: &str,
        address: &str,
        timing: SlipTiming,
        press: i32,
        release: i32,
    ) -> Result<()> {
        if self.holds.iter().any(|h| h.owner == owner) {
            return Ok(());
        }
        let Some(deck) = Self::slip_deck(address) else {
            bail!("Slip hold requires a /SlipLoop/ route");
        };
        let previous = self.active_slip(deck).cloned();
        self.holds.push(Hold {
            owner: owner.into(),
            address: address.into(),
            press,
            release,
            timing: Some(timing),
        });
        // Record timing ownership even if a later send fails, for global cleanup.
        self.slip_modes
            .entry(deck.to_ascii_lowercase())
            .or_insert_with(|| (deck.into(), SlipTiming::Straight));
        if let Some(previous) = previous {
            self.send(&previous.address, OscType::Int(previous.release))?;
        }
        self.timing(deck, timing)?;
        self.send(address, OscType::Int(press))
    }
    pub fn slip_release(&mut self, owner: &str) -> Result<()> {
        let Some(i) = self.holds.iter().position(|h| h.owner == owner) else {
            return Ok(());
        };
        let held = &self.holds[i];
        let Some(deck) = Self::slip_deck(&held.address) else {
            bail!("Not a slip owner: {owner}");
        };
        if self.active_slip(deck).is_some_and(|h| h.owner == owner) {
            self.send(&held.address, OscType::Int(held.release))?;
            if let Some(previous) = self.holds[..i].iter().rev().find(|h| {
                h.timing.is_some()
                    && Self::slip_deck(&h.address).is_some_and(|d| d.eq_ignore_ascii_case(deck))
            }) {
                self.timing(deck, previous.timing.unwrap())?;
                self.send(&previous.address, OscType::Int(previous.press))?;
            } else {
                self.timing(deck, SlipTiming::Selected)?;
            }
        }
        self.holds.remove(i);
        Ok(())
    }
    /// Called only after every device's own best-effort cleanup.
    pub fn cleanup(&mut self) {
        let owners: Vec<_> = self.holds.iter().map(|h| h.owner.clone()).collect();
        for owner in owners {
            if let Err(e) = self.release(&owner) {
                eprintln!("AP release {owner}: {e}");
            }
        }
        for (deck, _) in self.slip_modes.values() {
            if let Err(e) = self.timing(deck, SlipTiming::Straight) {
                eprintln!("AP timing cleanup: {e}");
            }
        }
    }
    pub fn accept(&mut self, packet: OscPacket) -> Vec<String> {
        let mut addresses = Vec::new();
        let mut budget = 1024;
        self.accept_inner(packet, 0, &mut budget, &mut addresses);
        addresses
    }
    fn accept_inner(
        &mut self,
        packet: OscPacket,
        depth: usize,
        budget: &mut usize,
        addresses: &mut Vec<String>,
    ) {
        if depth > 8 || *budget == 0 {
            return;
        }
        *budget -= 1;
        match packet {
            OscPacket::Bundle(bundle) => {
                for packet in bundle.content {
                    self.accept_inner(packet, depth + 1, budget, addresses);
                }
            }
            OscPacket::Message(message) => {
                let key = message.addr.to_ascii_lowercase();
                let Some(kind) = self.kinds.get(&key) else {
                    return;
                };
                let accepted = match (kind, message.args.as_slice()) {
                    (FeedbackKind::Text, [OscType::String(text)])
                        if text.len() <= 128 && !text.chars().any(char::is_control) =>
                    {
                        self.texts.insert(key.clone(), text.clone());
                        true
                    }
                    (FeedbackKind::Text, _) => false,
                    (_, args) => {
                        let value = match args {
                            [OscType::Int(n)] => Some(*n as f32),
                            [OscType::Float(n)] => Some(*n),
                            _ => None,
                        };
                        if let Some(value) = value.filter(|n| {
                            n.is_finite()
                                && match kind {
                                    FeedbackKind::Bipolar => (-1.0..=1.0).contains(n),
                                    FeedbackKind::Switch => *n == 0.0 || *n == 1.0,
                                    FeedbackKind::Meter => true,
                                    _ => (0.0..=1.0).contains(n),
                                }
                        }) {
                            let value = if *kind == FeedbackKind::Bipolar {
                                (value + 1.0) * 0.5
                            } else {
                                value
                            };
                            self.values.insert(key.clone(), value);
                            if self
                                .pending
                                .get(&key)
                                .is_some_and(|(expected, _)| (*expected - value).abs() < 0.001)
                            {
                                self.pending.remove(&key);
                            }
                            true
                        } else {
                            false
                        }
                    }
                };
                if let Some((_, pending)) = self.control_writers.get_mut(&key) {
                    *pending = false;
                }
                if accepted {
                    self.accepted += 1;
                } else {
                    self.values.remove(&key);
                    self.texts.remove(&key);
                    self.warn(format!(
                        "Invalid AP feedback for {}: {:?}; dependent controls blocked/re-armed",
                        message.addr, kind
                    ));
                }
                if !addresses.contains(&key) {
                    addresses.push(key);
                }
            }
        }
    }
}
