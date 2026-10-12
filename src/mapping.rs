#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Player {
    Track,
    Arrangement,
    Clip,
    Sampler,
    Empty,
    Unknown,
}

impl Player {
    pub fn parse(value: &str) -> Self {
        match value {
            "track" => Self::Track,
            "arrangement" => Self::Arrangement,
            "clip" => Self::Clip,
            "sampler" => Self::Sampler,
            "empty" => Self::Empty,
            _ => Self::Unknown,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Arrangement => "arrangement",
            Self::Clip => "clip",
            Self::Sampler => "sampler",
            Self::Empty => "empty",
            Self::Unknown => "unknown",
        }
    }

    // Only translate routes documented for the loaded player. No guessed aliases.
    pub fn route(self, route: &str) -> Option<String> {
        match self {
            Self::Track if matches!(route, "ControlPagePrevious" | "ControlPageNext") => None,
            Self::Track | Self::Arrangement
                if !route.starts_with("Sampler/") && !route.starts_with("Clip/") =>
            {
                Some(route.into())
            }
            Self::Clip => match route {
                "PlayPause" | "Cue" | "GridStart" | "Cue1" | "Cue2" | "Cue3" | "Cue4" | "Cue5"
                | "Cue6" | "Cue7" | "Cue8" | "PitchDown" | "PitchReset" | "PitchUp"
                | "PitchStep" | "PhaseMinus" | "PhasePlus" | "PhaseBend" | "PhaseReset" => {
                    Some(format!("Clip/{route}"))
                }
                _ if route.starts_with("Clip/") => Some(route.into()),
                _ => None,
            },
            Self::Sampler => match route {
                "PlayPause" | "PhaseBend" | "PhaseReset" | "Speed" | "Quantize" | "FaderStart" => {
                    Some(format!("Sampler/{route}"))
                }
                _ if route.starts_with("Sampler/") => Some(route.into()),
                _ => None,
            },
            _ => None,
        }
    }
}

// Use physical keys so a release is still recognised after Shift or deck changes.
pub fn key(channel: u8, note: u8) -> (u8, u8) {
    let note = match (channel, note) {
        (0..=1, 0x0e) => 0x0b,
        (0..=1, 0x48) => 0x0c,
        (0..=1, 0x4c) => 0x10,
        (0..=1, 0x4e) => 0x11,
        (0..=1, 0x50) => 0x4d,
        (0..=1, 0x5c | 0x60) => 0x58,
        (0..=1, 0x68) => 0x54,
        (0..=1, 0x69) => 0x1b,
        (0..=1, 0x6b) => 0x1e,
        (0..=1, 0x6d) => 0x20,
        (0..=1, 0x6f) => 0x22,
        (6, 0x68) => 0x46,
        (6, 0x7a) => 0x47,
        (6, 0x42) => 0x41,
        (7..=10, _) => return (7 + ((channel - 7) / 2) * 2, note & 0x0f),
        _ => note,
    };
    (channel, note)
}
