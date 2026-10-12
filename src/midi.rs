//! MIDI inventory, safe automatic port matching and read-only CLI utilities.
use anyhow::{anyhow, bail, Context, Result};
use midir::{Ignore, MidiInput, MidiOutput};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

// #index remains useful for a one-off monitor, but never for automatic discovery.
fn choose(names: &[String], selector: &str) -> Result<usize> {
    if let Some(index) = selector.strip_prefix('#') {
        let index: usize = index
            .parse()
            .context("Port index must be # followed by a number")?;
        if index < names.len() {
            return Ok(index);
        }
        bail!("Port {selector} does not exist. Use --list");
    }
    if selector.is_empty() {
        bail!("Empty MIDI selector; use --list");
    }
    let matches: Vec<_> = names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.to_lowercase().contains(&selector.to_lowercase()))
        .map(|(i, _)| i)
        .collect();
    match matches.as_slice() {
        [index] => Ok(*index),
        [] => bail!("No MIDI port matches {selector:?}. Use --list"),
        _ => bail!("Multiple MIDI ports match {selector:?}. Choose an explicit #index from --list"),
    }
}
pub fn list() -> Result<()> {
    let input = MidiInput::new("MIDI bridge list")?;
    for (i, port) in input.ports().iter().enumerate() {
        println!("IN #{i}: {}", input.port_name(port)?);
    }
    let output = MidiOutput::new("MIDI bridge list")?;
    for (i, port) in output.ports().iter().enumerate() {
        println!("OUT #{i}: {}", output.port_name(port)?);
    }
    Ok(())
}
pub fn monitor(selector: &str, running: Arc<AtomicBool>) -> Result<()> {
    let mut input = MidiInput::new("MIDI bridge read-only monitor")?;
    input.ignore(Ignore::None);
    let ports = input.ports();
    let names = ports
        .iter()
        .map(|p| input.port_name(p))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let index = choose(&names, selector)?;
    let (tx, rx) = mpsc::sync_channel(4096);
    let overflow = Arc::new(AtomicBool::new(false));
    let flag = overflow.clone();
    let _connection = input
        .connect(
            &ports[index],
            "MIDI bridge monitor",
            move |_, bytes, _| {
                if bytes.len() > 1024 || tx.try_send(bytes.to_vec()).is_err() {
                    flag.store(true, Ordering::Relaxed);
                }
            },
            (),
        )
        .map_err(|e| anyhow!("Open MIDI input: {e}"))?;
    println!(
        "Read-only monitor: {}. No MIDI output or OSC. Ctrl+C exits.",
        names[index]
    );
    while running.load(Ordering::Relaxed) {
        if overflow.load(Ordering::Relaxed) {
            bail!("MIDI queue overflow or oversized packet; capture incomplete");
        }
        match rx.recv_timeout(Duration::from_millis(20)) {
            Ok(bytes) => println!("{:02X?}", bytes),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("MIDI monitor input closed"),
        }
    }
    Ok(())
}

pub fn inventory() -> Result<(Vec<String>, Vec<String>)> {
    let input = MidiInput::new("MIDI bridge presence")?;
    let inputs = input
        .ports()
        .iter()
        .map(|p| input.port_name(p))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let output = MidiOutput::new("MIDI bridge presence")?;
    let outputs = output
        .ports()
        .iter()
        .map(|p| output.port_name(p))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok((inputs, outputs))
}
// Exact names take precedence over substrings; neither ambiguous names nor shifting indexes
// are safe to guess during hotplug. None means the port is simply unplugged.
pub fn match_port(names: &[String], selector: &str) -> std::result::Result<Option<usize>, String> {
    if selector.starts_with('#') {
        return Err(format!(
            "selector {selector:?} uses an unstable #index; use a MIDI port name instead"
        ));
    }
    let exact: Vec<_> = names
        .iter()
        .enumerate()
        .filter(|(_, name)| name.as_str() == selector)
        .map(|(i, _)| i)
        .collect();
    let matches = if exact.is_empty() {
        names
            .iter()
            .enumerate()
            .filter(|(_, name)| name.to_lowercase().contains(&selector.to_lowercase()))
            .map(|(i, _)| i)
            .collect::<Vec<_>>()
    } else {
        exact
    };
    match matches.as_slice() {
        [] => Ok(None),
        [index] => Ok(Some(*index)),
        _ => Err(format!(
            "selector {selector:?} matches multiple ports ({}); use a unique port name",
            matches
                .iter()
                .map(|i| names[*i].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}
