//! MIDI adapters and profile loading. Port lifecycle belongs to runtime; OSC state to AP.
mod declarative;
mod flx4;

use crate::{
    ap::SharedAp,
    profile::{DeclarativeConfig, DeviceProfile},
};
use anyhow::{bail, Context, Result};
use declarative::Declarative;
use flx4::Flx4;
use midir::MidiOutputConnection;
use serde::Deserialize;
use std::path::Path;

/// One typed configuration per device; variant fields are strict, not flattened.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Config {
    Declarative(DeclarativeConfig),
    Flx4(flx4::Config),
}

pub trait Adapter {
    fn start(&mut self) -> Result<()>;
    fn input(&mut self, port: usize, bytes: &[u8]) -> Result<()>;
    fn tick(&mut self) -> Result<()>;
    /// Interested, unique lowercase addresses from one fully processed OSC packet.
    /// Invalidate affected captures before refreshing the hardware once.
    fn feedback(&mut self, addresses: &[&str]) -> Result<()>;
    fn status(&self, verbose: bool) -> String;
    fn cleanup(&mut self);
}

/// Load the current profile format and validate before runtime opens any MIDI ports.
pub fn load(path: &Path) -> Result<DeviceProfile> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("Read profile {}", path.display()))?;
    let profile: DeviceProfile =
        serde_json::from_str(&json).with_context(|| format!("Parse profile {}", path.display()))?;
    profile.validate()?;
    let (inputs, outputs) = (profile.inputs.len(), profile.outputs.len());
    match &profile.adapter {
        Config::Declarative(cfg) => cfg.validate(inputs, outputs)?,
        Config::Flx4(cfg) => cfg.validate(inputs, outputs)?,
    }
    Ok(profile)
}

pub fn create(
    name: String,
    profile: &DeviceProfile,
    outputs: Vec<MidiOutputConnection>,
    ap: SharedAp,
) -> Result<Box<dyn Adapter>> {
    if outputs.len() != profile.outputs.len() {
        bail!(
            "{name}: expected {} MIDI outputs, got {}",
            profile.outputs.len(),
            outputs.len()
        );
    }
    // Discovery validated these typed configurations. Reconnect only clones the
    // selected adapter's configuration; no JSON parsing or preflight is repeated.
    match &profile.adapter {
        Config::Flx4(cfg) => Ok(Box::new(Flx4::new(name, cfg.clone(), outputs, ap)?)),
        Config::Declarative(cfg) => Ok(Box::new(Declarative::new(
            name,
            cfg.clone(),
            profile.inputs.len(),
            outputs,
            ap,
        )?)),
    }
}
