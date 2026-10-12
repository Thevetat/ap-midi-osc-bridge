# Release status — AP MIDI OSC Bridge 0.3.4

This is a profile-driven preview for AUTO/PILOT with supplied DDJ-FLX4 and compact Launch Control 3 profiles. Its inclusion in a profile is **not** evidence of successful operation on a particular OS, AP version or physical controller. No GitHub Release binary is published; the manual workflow produces build artifacts, not releases.

## Evidence and coverage

| Area | Known status |
|---|---|
| Prior FLX4 Pilot 0.3.4 | A Windows x64 FLX4-only archive was built and packaged before the multi-controller change; this does **not** validate the current binary. |
| Current Windows x64 / macOS ARM64 and Intel / Linux x64 | Manual build-and-package workflow is defined. Successful builds or packaging of this version on each platform have not been established in this document. Linux needs compatible ALSA/libc. |
| DDJ-FLX4 and Launch Control 3 hardware | Current integration and joint operation have not been established by hardware observation. |
| AP end-to-end behaviour | Not established across AP versions. `Filter1`/`Filter2` full input paths are inferred from a change note while the published mixer table still uses older names. |
| Signing | Binaries are unsigned and unnotarized. An archive SHA-256 detects corruption, not who produced it. |

The old FLX4-only `dist/` files are historical artifacts, **not** packages of the current multi-controller source. Do not use their filenames as evidence of current compatibility.

## Included and bounded

- Editable version-2 controller profiles, port autodiscovery/hotplug, shared AP feedback and independent per-device sessions; FLX4's existing pad/deck behaviour and a compact LC3 DAW-port layout are documented in [MAPPING.md](MAPPING.md).
- FLX4 deck selection, content-aware pads, slip-loop timing, mixer and bipolar tempo pickup; LC3 EQ/filter encoders, four-deck hot-cue target selection, layer display and mixer buttons. These are implementation/mapping descriptions, not hardware-validation claims.
- AP `TempoSlider` input and feedback must use −1..+1; missing compatible feedback blocks pickup. Pickup/toggles need confirmed AP feedback. Clip/sampler actions differ from track/arrangement; jog/scratch, repeat scrubbing, crossfader and FX are not mapped on FLX4.
- Device detach and Ctrl+C attempt release/cleanup. AP feedback/OSC is UDP; crashes, force-kills, packet loss and AP restarts may leave state uncertain. A reconnect is not an AP end-to-end recovery guarantee.

[README.md](README.md) is the setup guide; [USER-GUIDE.md](USER-GUIDE.md) explains operation and troubleshooting; [PROFILES.md](PROFILES.md) defines the editable schema; [DEVELOPMENT.md](DEVELOPMENT.md) describes build and runtime contracts; [CONTROLLER-PORTING.md](CONTROLLER-PORTING.md) gives research and safety boundaries. Both supplied printable guides are retained; the FLX4 sheet dates from FLX4 Pilot and contains legacy naming/CLI wording, so prefer the current README and mapping when they differ.
