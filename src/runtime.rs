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

struct ProfileEntry {
    owner: String,
    profile: DeviceProfile,
    last_status: Option<String>,
}
impl ProfileEntry {
    fn status(&mut self, message: String) {
        if self.last_status.as_ref() != Some(&message) {
            println!("[{}: {}] {message}", self.owner, self.profile.name);
        }
        self.last_status = Some(message);
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
impl Live {
    fn status(&mut self, entry: &mut ProfileEntry, verbose: bool) {
        let status = self.engine.status(verbose);
        if verbose {
            println!(
                "[{}] {status} | MIDI in={} OSC sent={} feedback received={}",
                entry.owner, self.midi_count, self.osc_count, self.feedback_count
            );
        } else if self.last_status.as_ref() != Some(&status) {
            entry.status(status.clone());
        }
        self.last_status = Some(status);
    }
}
fn discover(dir: &Path) -> Result<Vec<ProfileEntry>> {
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
            Ok(profile) if entries.len() < 32 => entries.push(ProfileEntry {
                owner,
                profile,
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
fn plan(
    entries: &mut [ProfileEntry],
    inputs: &[String],
    outputs: &[String],
) -> Vec<Option<Selection>> {
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
fn attach(entry: &ProfileEntry, selected: Selection, ap: &SharedAp) -> Result<Live> {
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
    ap.borrow_mut().registration_owner(&entry.owner);
    let mut engine = adapters::create(
        entry.owner.clone(),
        entry.profile.clone(),
        outputs,
        ap.clone(),
    )
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
fn detach(entry: &ProfileEntry, live: &mut Option<Live>, ap: &SharedAp) {
    if let Some(mut session) = live.take() {
        session.connections.clear(); // Stop callbacks before releasing held controls and MIDI outputs.
        session.engine.cleanup();
    }
    ap.borrow_mut().remove_session(&entry.owner);
}
fn reconcile(
    entries: &mut [ProfileEntry],
    sessions: &mut [Option<Live>],
    ap: &SharedAp,
    inputs: &[String],
    outputs: &[String],
    failed_at: &mut [Option<Instant>],
) -> Result<()> {
    let mut attached = false;
    // Determine every claim before opening any MIDI ports. Conflicts invalidate ALL claimants.
    let plans = plan(entries, inputs, outputs);
    // Release every stale session before opening any replacement; port ownership can
    // move between profiles in this inventory snapshot.
    for (i, desired) in plans.iter().enumerate() {
        if sessions[i]
            .as_ref()
            .is_some_and(|live| Some(&live.selected) != desired.as_ref())
        {
            detach(&entries[i], &mut sessions[i], ap);
        }
        if desired.is_none() {
            failed_at[i] = None;
        }
    }
    for (i, desired) in plans.into_iter().enumerate() {
        if sessions[i].is_some() || desired.is_none() {
            continue;
        }
        if failed_at[i].is_some_and(|at| at.elapsed() < Duration::from_secs(10)) {
            continue;
        }
        match attach(&entries[i], desired.unwrap(), ap) {
            Ok(mut live) => {
                println!(
                    "[{}: {}] Connected: IN {} | OUT {}",
                    entries[i].owner,
                    entries[i].profile.name,
                    live.selected.inputs.join(", "),
                    live.selected.outputs.join(", ")
                );
                entries[i].last_status = None;
                live.status(&mut entries[i], false);
                sessions[i] = Some(live);
                failed_at[i] = None;
                attached = true;
            }
            Err(e) => {
                ap.borrow_mut().remove_session(&entries[i].owner);
                entries[i].status(format!(
                    "Device attach failed: {e:#}; retrying in 10s or on port change"
                ));
                failed_at[i] = Some(Instant::now());
            }
        }
    }
    if attached {
        ap.borrow()
            .send("/AutoPilot/Feedback/Resend", OscType::Int(1))?;
    }
    Ok(())
}
fn dispatch(live: &mut Live, entry: &ProfileEntry, ap: &SharedAp, verbose: bool) -> Result<()> {
    if live.overflow.load(Ordering::Relaxed) {
        bail!("MIDI queue overflow or packet >1024 bytes; releasing controls");
    }
    let before = ap.borrow().counters().0;
    for _ in 0..256 {
        match live.receiver.try_recv() {
            Ok((port, bytes)) => {
                live.midi_count += 1;
                if verbose {
                    println!("[{}] MIDI IN {port}: {:02X?}", entry.owner, bytes);
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
    let mut sessions: Vec<Option<Live>> = (0..entries.len()).map(|_| None).collect();
    let mut failed_at = vec![None; entries.len()];
    println!("OSC → {destination} | feedback ← {feedback}. Waiting for matching MIDI devices; Ctrl+C exits.");
    let result = event_loop(
        &mut entries,
        &mut sessions,
        &mut failed_at,
        &socket,
        &ap,
        destination,
        verbose,
        &running,
    );
    for (entry, live) in entries.iter().zip(&mut sessions) {
        detach(entry, live, &ap);
    }
    ap.borrow_mut().cleanup();
    result
}
fn event_loop(
    entries: &mut [ProfileEntry],
    sessions: &mut [Option<Live>],
    failed_at: &mut [Option<Instant>],
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
                        failed_at.fill(None);
                    }
                    if changed
                        || failed_at
                            .iter()
                            .flatten()
                            .any(|at| at.elapsed() >= Duration::from_secs(10))
                    {
                        reconcile(entries, sessions, ap, &current.0, &current.1, failed_at)?;
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
        for i in 0..sessions.len() {
            if let Some(live) = &mut sessions[i] {
                if let Err(e) = dispatch(live, &entries[i], ap, verbose) {
                    entries[i].status(format!(
                        "Device failed: {e:#}; retrying in 10s or on port change"
                    ));
                    detach(&entries[i], &mut sessions[i], ap);
                    failed_at[i] = Some(Instant::now());
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
                            for i in 0..sessions.len() {
                                let Some(live) = &mut sessions[i] else {
                                    continue;
                                };
                                let before = ap.borrow().counters().0;
                                let result = addresses
                                    .iter()
                                    .filter(|address| {
                                        ap.borrow().interested(&entries[i].owner, address)
                                    })
                                    .try_for_each(|address| {
                                        live.engine.feedback(address)?;
                                        live.feedback_count += 1;
                                        Ok::<(), anyhow::Error>(())
                                    });
                                live.osc_count += ap.borrow().counters().0 - before;
                                if let Err(e) = result {
                                    entries[i].status(format!("Hardware feedback failed: {e:#}; retrying in 10s or on port change"));
                                    detach(&entries[i], &mut sessions[i], ap);
                                    failed_at[i] = Some(Instant::now());
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
            for (entry, session) in entries.iter_mut().zip(sessions.iter_mut()) {
                if let Some(live) = session {
                    live.status(entry, verbose);
                }
            }
            if !feedback_warning
                && ap.borrow().counters().1 == 0
                && sessions.iter().any(Option::is_some)
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
