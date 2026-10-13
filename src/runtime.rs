//! Automatic profile discovery, independent MIDI sessions and the shared AP event loop.
use crate::{
    adapters::{self, Adapter},
    ap::{Ap, SharedAp},
    midi::{inventory, match_port},
    profile::DeviceProfile,
};
use anyhow::{anyhow, bail, Context, Result};
use midir::{Ignore, MidiInput, MidiInputConnection, MidiOutput};
use rosc::OscType;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    io::ErrorKind,
    net::{SocketAddr, UdpSocket},
    path::Path,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

struct Device {
    owner: String,
    profile: DeviceProfile,
    live: Option<Live>,
    failed_at: Option<Instant>,
    last_status: Option<String>,
}
impl Device {
    fn status(&mut self, message: String) {
        if self.last_status.as_ref() != Some(&message) {
            println!("[{}: {}] {message}", self.owner, self.profile.name);
        }
        self.last_status = Some(message);
    }
    fn report(&mut self, verbose: bool) {
        let Some(live) = &mut self.live else { return };
        let status = live.engine.status(verbose);
        let changed = live.last_status.as_ref() != Some(&status);
        live.last_status = Some(status.clone());
        if verbose {
            println!(
                "[{}] {status} | MIDI in={} OSC sent={} feedback received={}",
                self.owner, live.midi_count, live.osc_count, live.feedback_count
            );
        } else if changed {
            self.status(status);
        }
    }
    fn detach(&mut self, ap: &SharedAp) {
        if let Some(mut live) = self.live.take() {
            // Stop callbacks before releasing held controls and MIDI outputs.
            live.connections.clear();
            live.engine.cleanup();
        }
        ap.borrow_mut().remove_session(&self.owner);
    }
    fn fail(&mut self, message: String, ap: &SharedAp) {
        self.status(format!("{message}; retrying in 10s or on port change"));
        self.detach(ap);
        self.failed_at = Some(Instant::now());
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Selection {
    inputs: Vec<String>,
    outputs: Vec<String>,
}
struct Live {
    engine: Box<dyn Adapter>,
    connections: Vec<MidiInputConnection<()>>,
    receiver: mpsc::Receiver<(usize, Vec<u8>)>,
    overflow: Arc<AtomicBool>,
    selected: Selection,
    midi_count: u64,
    osc_count: u64,
    feedback_count: u64,
    last_status: Option<String>,
}
fn discover(dir: &Path) -> Result<Vec<Device>> {
    let mut paths = std::fs::read_dir(dir)
        .with_context(|| format!("Read profile directory {}", dir.display()))?
        .map(|item| item.map(|item| item.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut found = 0;
    let mut entries = Vec::new();
    for path in paths {
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        found += 1;
        // Encode unusual filename bytes rather than filtering the human profile name.
        // In particular, ':' must not collide with session-owned hold prefixes.
        let owner = path
            .file_name()
            .unwrap()
            .as_encoded_bytes()
            .iter()
            .map(|&byte| {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
                    (byte as char).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect::<String>();
        match adapters::load(&path).and_then(|p| {
            if let Some(selector) = p
                .inputs
                .iter()
                .chain(&p.outputs)
                .find(|s| s.starts_with('#'))
            {
                bail!(
                    "selector {selector:?} uses an unstable #index; use a MIDI port name instead"
                );
            }
            Ok(p)
        }) {
            Ok(profile) if entries.len() < 32 => entries.push(Device {
                owner,
                profile,
                live: None,
                failed_at: None,
                last_status: None,
            }),
            Ok(_) => eprintln!(
                "{}: at most 32 valid profiles supported; skipping",
                path.display()
            ),
            Err(e) => eprintln!("{}: invalid profile, skipping: {e:#}", path.display()),
        }
    }
    if found == 0 {
        bail!(
            "No .json profiles in {}; add a device profile and restart",
            dir.display()
        );
    }
    if entries.is_empty() {
        bail!(
            "No valid MIDI profiles in {}; fix profile errors and restart",
            dir.display()
        );
    }
    println!(
        "Loaded {} profile(s) from {}. Profile edits require restart.",
        entries.len(),
        dir.display()
    );
    Ok(entries)
}
fn plan(entries: &mut [Device], inputs: &[String], outputs: &[String]) -> Vec<Option<Selection>> {
    let mut plans = Vec::new();
    for entry in entries.iter_mut() {
        let mut ins = Vec::new();
        let mut outs = Vec::new();
        let mut issue = None;
        for selector in &entry.profile.inputs {
            match match_port(inputs, selector) {
                Ok(Some(i)) => ins.push(i),
                Ok(None) => {
                    issue = Some(format!("Waiting for MIDI input {selector:?}"));
                    break;
                }
                Err(e) => {
                    issue = Some(format!("MIDI input: {e}"));
                    break;
                }
            }
        }
        if issue.is_none() {
            for selector in &entry.profile.outputs {
                match match_port(outputs, selector) {
                    Ok(Some(i)) => outs.push(i),
                    Ok(None) => {
                        issue = Some(format!("MIDI input found, but output {selector:?} unavailable; check its connection/port name"));
                        break;
                    }
                    Err(e) => {
                        issue = Some(format!("MIDI output: {e}"));
                        break;
                    }
                }
            }
        }
        if let Some(issue) = issue {
            entry.status(issue);
            plans.push(None);
        } else {
            plans.push(Some((ins, outs)));
        }
    }
    let mut claims: HashMap<(bool, usize), HashSet<usize>> = HashMap::new();
    for (candidate, pair) in plans.iter().enumerate() {
        if let Some((ins, outs)) = pair {
            for &i in ins {
                claims.entry((true, i)).or_default().insert(candidate);
            }
            for &i in outs {
                claims.entry((false, i)).or_default().insert(candidate);
            }
        }
    }
    let mut result = Vec::new();
    for (candidate, pair) in plans.into_iter().enumerate() {
        if let Some((ins, outs)) = pair {
            let duplicate = |direction, indexes: &[usize]| {
                indexes
                    .iter()
                    .find(|&&i| {
                        claims[&(direction, i)].len() > 1
                            || indexes.iter().filter(|&&j| i == j).count() > 1
                    })
                    .copied()
            };
            if let Some(i) = duplicate(true, &ins) {
                entries[candidate].status(format!(
                    "Conflicting MIDI input {}; every physical port must belong to one profile",
                    inputs[i]
                ));
                result.push(None);
            } else if let Some(i) = duplicate(false, &outs) {
                entries[candidate].status(format!(
                    "Conflicting MIDI output {}; every physical port must belong to one profile",
                    outputs[i]
                ));
                result.push(None);
            } else {
                result.push(Some(Selection {
                    inputs: ins.into_iter().map(|i| inputs[i].clone()).collect(),
                    outputs: outs.into_iter().map(|i| outputs[i].clone()).collect(),
                }));
            }
        } else {
            result.push(None);
        }
    }
    result
}
fn attach(entry: &Device, selected: Selection, ap: &SharedAp) -> Result<Live> {
    let (sender, receiver) = mpsc::sync_channel(4096);
    let overflow = Arc::new(AtomicBool::new(false));
    let mut connections = Vec::new();
    let mut outputs = Vec::new();
    for (port, name) in selected.inputs.iter().enumerate() {
        let mut input = MidiInput::new(&format!("MIDI bridge {}", entry.owner))?;
        input.ignore(Ignore::None);
        let ports = input.ports();
        let names = ports
            .iter()
            .map(|p| input.port_name(p))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let matches: Vec<_> = names
            .iter()
            .enumerate()
            .filter(|(_, current)| *current == name)
            .map(|(i, _)| i)
            .collect();
        let [index] = matches.as_slice() else {
            bail!("MIDI input {name:?} disappeared or became ambiguous during attach");
        };
        let tx = sender.clone();
        let flag = overflow.clone();
        connections.push(
            input
                .connect(
                    &ports[*index],
                    &format!("{} input {port}", entry.owner),
                    move |_, bytes, _| {
                        if bytes.len() > 1024 || tx.try_send((port, bytes.to_vec())).is_err() {
                            flag.store(true, Ordering::Relaxed);
                        }
                    },
                    (),
                )
                .map_err(|e| anyhow!("open input {name}: {e}"))?,
        );
    }
    drop(sender);
    for (port, name) in selected.outputs.iter().enumerate() {
        let output = MidiOutput::new(&format!("MIDI bridge {}", entry.owner))?;
        let ports = output.ports();
        let names = ports
            .iter()
            .map(|p| output.port_name(p))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let matches: Vec<_> = names
            .iter()
            .enumerate()
            .filter(|(_, current)| *current == name)
            .map(|(i, _)| i)
            .collect();
        let [index] = matches.as_slice() else {
            bail!("MIDI output {name:?} disappeared or became ambiguous during attach");
        };
        outputs.push(
            output
                .connect(&ports[*index], &format!("{} output {port}", entry.owner))
                .map_err(|e| anyhow!("open output {name}: {e}"))?,
        );
    }
    let mut engine = adapters::create(entry.owner.clone(), &entry.profile, outputs, ap.clone())
        .context("construct capability")?;
    let before = ap.borrow().counters().0;
    if let Err(e) = engine.start() {
        connections.clear();
        engine.cleanup();
        return Err(e).context("startup");
    }
    Ok(Live {
        engine,
        connections,
        receiver,
        overflow,
        selected,
        midi_count: 0,
        osc_count: ap.borrow().counters().0 - before,
        feedback_count: 0,
        last_status: None,
    })
}
fn reconcile(
    entries: &mut [Device],
    ap: &SharedAp,
    inputs: &[String],
    outputs: &[String],
) -> Result<()> {
    let mut attached = false;
    // Determine every claim before opening any MIDI ports. Conflicts invalidate ALL claimants.
    let plans = plan(entries, inputs, outputs);
    // Release every stale session before opening any replacement; port ownership can
    // move between profiles in this inventory snapshot.
    for (entry, desired) in entries.iter_mut().zip(&plans) {
        if entry
            .live
            .as_ref()
            .is_some_and(|live| Some(&live.selected) != desired.as_ref())
        {
            entry.detach(ap);
        }
        if desired.is_none() {
            entry.failed_at = None;
        }
    }
    for (entry, desired) in entries.iter_mut().zip(plans) {
        let Some(desired) = desired else { continue };
        if entry.live.is_some()
            || entry
                .failed_at
                .is_some_and(|at| at.elapsed() < Duration::from_secs(10))
        {
            continue;
        }
        match attach(entry, desired, ap) {
            Ok(live) => {
                println!(
                    "[{}: {}] Connected: IN {} | OUT {}",
                    entry.owner,
                    entry.profile.name,
                    live.selected.inputs.join(", "),
                    live.selected.outputs.join(", ")
                );
                entry.last_status = None;
                entry.live = Some(live);
                entry.failed_at = None;
                entry.report(false);
                attached = true;
            }
            Err(e) => entry.fail(format!("Device attach failed: {e:#}"), ap),
        }
    }
    if attached {
        ap.borrow()
            .send("/AutoPilot/Feedback/Resend", OscType::Int(1))?;
    }
    Ok(())
}
fn dispatch(live: &mut Live, owner: &str, ap: &SharedAp, verbose: bool) -> Result<()> {
    if live.overflow.load(Ordering::Relaxed) {
        bail!("MIDI queue overflow or packet >1024 bytes; releasing controls");
    }
    let before = ap.borrow().counters().0;
    for _ in 0..256 {
        match live.receiver.try_recv() {
            Ok((port, bytes)) => {
                live.midi_count += 1;
                if verbose {
                    println!("[{owner}] MIDI IN {port}: {:02X?}", bytes);
                }
                live.engine.input(port, &bytes).context("MIDI dispatch")?;
            }
            Err(mpsc::TryRecvError::Empty) => break,
            Err(mpsc::TryRecvError::Disconnected) => bail!("MIDI input callbacks disconnected"),
        }
    }
    live.engine.tick().context("periodic hook")?;
    live.osc_count += ap.borrow().counters().0 - before;
    Ok(())
}
pub fn run(
    profile_dir: &Path,
    destination: SocketAddr,
    feedback: SocketAddr,
    verbose: bool,
    running: Arc<AtomicBool>,
) -> Result<()> {
    if destination.is_ipv4() != feedback.is_ipv4() {
        bail!("OSC destination and feedback bind must use the same address family");
    }
    let mut entries = discover(profile_dir)?;
    let socket = UdpSocket::bind(feedback)
        .context("Bind AP feedback socket; stop any old bridge using this port")?;
    socket.set_nonblocking(true)?;
    let ap: SharedAp = Rc::new(RefCell::new(Ap::new(
        socket.try_clone()?,
        destination,
        verbose,
    )));
    println!("OSC → {destination} | feedback ← {feedback}. Waiting for matching MIDI devices; Ctrl+C exits.");
    let result = event_loop(&mut entries, &socket, &ap, destination, verbose, &running);
    for entry in &mut entries {
        entry.detach(&ap);
    }
    ap.borrow_mut().cleanup();
    result
}
fn event_loop(
    entries: &mut [Device],
    socket: &UdpSocket,
    ap: &SharedAp,
    destination: SocketAddr,
    verbose: bool,
    running: &AtomicBool,
) -> Result<()> {
    let mut previous_inventory = None;
    let mut inventory_warning = None;
    let mut last_ports = Instant::now() - Duration::from_secs(2);
    let mut last_status = Instant::now();
    let mut buffer = [0u8; 65535];
    let mut packet_seen = false;
    let mut feedback_warning = false;
    while running.load(Ordering::Relaxed) {
        if last_ports.elapsed() >= Duration::from_secs(2) {
            match inventory() {
                Ok(current) => {
                    inventory_warning = None;
                    let changed = previous_inventory.as_ref() != Some(&current);
                    if changed {
                        for entry in entries.iter_mut() {
                            entry.failed_at = None;
                        }
                    }
                    if changed
                        || entries.iter().any(|entry| {
                            entry
                                .failed_at
                                .is_some_and(|at| at.elapsed() >= Duration::from_secs(10))
                        })
                    {
                        reconcile(entries, ap, &current.0, &current.1)?;
                        if verbose && changed {
                            ap.borrow().describe_shared_routes();
                        }
                    }
                    previous_inventory = Some(current);
                }
                Err(e) => {
                    let message = format!("MIDI inventory unavailable: {e:#}; retrying");
                    if inventory_warning.as_ref() != Some(&message) {
                        eprintln!("{message}");
                    }
                    inventory_warning = Some(message);
                }
            }
            last_ports = Instant::now();
        }
        for entry in entries.iter_mut() {
            if let Some(live) = &mut entry.live {
                if let Err(e) = dispatch(live, &entry.owner, ap, verbose) {
                    entry.fail(format!("Device failed: {e:#}"), ap);
                }
            }
        }
        for _ in 0..32 {
            match socket.recv_from(&mut buffer) {
                Ok((length, source)) if source.ip() == destination.ip() => {
                    if let Ok((remaining, packet)) = rosc::decoder::decode_udp(&buffer[..length]) {
                        if remaining.is_empty() {
                            packet_seen = true;
                            let addresses = ap.borrow_mut().accept(packet);
                            for entry in entries.iter_mut() {
                                let Some(live) = &mut entry.live else {
                                    continue;
                                };
                                let interested: Vec<_> = addresses
                                    .iter()
                                    .map(String::as_str)
                                    .filter(|address| ap.borrow().interested(&entry.owner, address))
                                    .collect();
                                if interested.is_empty() {
                                    continue;
                                }
                                let before = ap.borrow().counters().0;
                                let result = live.engine.feedback(&interested);
                                live.osc_count += ap.borrow().counters().0 - before;
                                match result {
                                    // Count addresses, not batches or hardware writes.
                                    Ok(()) => live.feedback_count += interested.len() as u64,
                                    Err(e) => {
                                        entry.fail(format!("Hardware feedback failed: {e:#}"), ap)
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // Windows UDP reports an ICMP port-unreachable reply as a receive error.
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::ConnectionReset | ErrorKind::ConnectionRefused
                    ) =>
                {
                    break
                }
                Err(e) => return Err(e).context("Receive AP feedback"),
            }
        }
        if last_status.elapsed() >= Duration::from_secs(5) {
            for entry in entries.iter_mut() {
                entry.report(verbose);
            }
            if !feedback_warning
                && ap.borrow().counters().1 == 0
                && entries.iter().any(|entry| entry.live.is_some())
            {
                feedback_warning = true;
                eprintln!("{} Enable AP OSC feedback subscriptions and check destination {destination} / feedback {}.", if packet_seen { "OSC received, but no valid subscribed control state." } else { "No valid OSC packets received from AP." }, socket.local_addr()?);
            }
            last_status = Instant::now();
        }
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}
