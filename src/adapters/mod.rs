//! MIDI adapters and profile loading. Port lifecycle belongs to runtime; OSC state to AP.
mod declarative;
mod flx4;

use crate::{ap::SharedAp, profile::DeviceProfile};
use anyhow::{bail, Context, Result};
use declarative::Declarative;
use flx4::Flx4;
use midir::MidiOutputConnection;
use std::path::Path;

pub trait Adapter {
    fn start(&mut self) -> Result<()>;
    fn input(&mut self, port: usize, bytes: &[u8]) -> Result<()>;
    fn tick(&mut self) -> Result<()>;
    fn feedback(&mut self, address: &str) -> Result<()>;
    fn status(&self, verbose: bool) -> String;
    fn cleanup(&mut self);
}

/// Load the current profile format and validate before runtime opens any MIDI ports.
pub fn load(path: &Path) -> Result<DeviceProfile> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("Read profile {}", path.display()))?;
    let profile: DeviceProfile =
        serde_json::from_str(&json).with_context(|| format!("Parse profile {}", path.display()))?;
    validate(&profile)?;
    Ok(profile)
}

fn validate(profile: &DeviceProfile) -> Result<()> {
    profile.validate()?;
    match profile.protocol.as_str() {
        "flx4" => flx4::validate(profile),
        "declarative" => Ok(()),
        other => bail!("Unsupported MIDI protocol {other:?}; supported: declarative, flx4"),
    }
}

pub fn create(
    name: String,
    profile: DeviceProfile,
    outputs: Vec<MidiOutputConnection>,
    ap: SharedAp,
) -> Result<Box<dyn Adapter>> {
    match profile.protocol.as_str() {
        "flx4" => Ok(Box::new(Flx4::new(name, profile, outputs, ap)?)),
        "declarative" => Ok(Box::new(Declarative::new(name, profile, outputs, ap)?)),
        other => bail!("Unsupported MIDI protocol {other:?}"),
    }
}
