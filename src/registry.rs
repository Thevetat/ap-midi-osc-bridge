//! Select a MIDI decoder; device matching and OSC transport live in the runtime.
use crate::{
    ap::SharedAp, device_profile::DeviceProfile, flx4::Flx4, runtime::Device, session::Session,
};
use anyhow::{bail, Result};
use midir::MidiOutputConnection;

pub fn validate(profile: &DeviceProfile) -> Result<()> {
    profile.validate()?;
    match profile.protocol.as_str() {
        "flx4" => crate::flx4::validate_config(profile),
        "declarative" => Ok(()),
        other => bail!("Unsupported MIDI protocol {other:?}; supported: declarative, flx4"),
    }
}

pub fn create(
    name: String,
    profile: DeviceProfile,
    outputs: Vec<MidiOutputConnection>,
    ap: SharedAp,
) -> Result<Box<dyn Device>> {
    match profile.protocol.as_str() {
        "flx4" => Ok(Box::new(Flx4::new(name, profile, outputs, ap)?)),
        "declarative" => Ok(Box::new(Session::new(name, profile, outputs, ap)?)),
        other => bail!("Unsupported MIDI protocol {other:?}"),
    }
}
