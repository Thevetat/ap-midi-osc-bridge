# AP MIDI OSC Bridge

> [!WARNING]
> **Syndicate is in beta and changing rapidly. This bridge is experimental—use it entirely at your own risk and discretion.** Updates to Syndicate may break functionality or cause unexpected behaviour.
>
> This is an independent, unofficial project. **Do not report bridge-related issues to the Syndicate Discord, its developers, or its support channels.** Keep those reports in this repository's issue tracker.
>
> **Before reporting any suspected Syndicate bug, fully stop this bridge, restart Syndicate, and verify that the bug still occurs without the bridge running.** If it only occurs with the bridge active, report it here instead.

A native MIDI → OSC bridge for [Syndicate](https://autopilot.audio/) (targeting the API documented in [guide build 0.1.904, §14](https://autopilot.audio/user-guide/index.html?c=c14) and [recent changes 0.1.903](https://autopilot.audio/user-guide/index.html?c=c17)). Put controller mappings in `profiles/` beside the executable: on startup, the app matches connected MIDI ports to those profiles and opens the controllers it finds. No controller selection or device-list configuration is needed.

## Get started

1. Connect your controller. In Syndicate, **disable that controller's direct MIDI input** to prevent duplicate actions. Enable OSC input on **9090**, OSC feedback to **127.0.0.1:9096**, and **Toggle All** feedback subscriptions (including TempoSlider **and TempoSliderEnabled**). Feedback is change-only; there is no rate setting. Set **Settings → General → Browser Mode → CONTROL MODE** for hardware browsing.
2. Extract the app and `profiles/` together. On Windows, double-click **`ap-midi-osc-bridge.exe`**; on macOS/Linux, run `./ap-midi-osc-bridge` from its directory. Keep the console open while playing; **Ctrl+C** requests best-effort release of held controls. Do not run from inside the ZIP.
3. For another controller, start with the [controller-porting guide](CONTROLLER-PORTING.md): it explains protocol research, profiles and when a Rust adapter is needed. For different MIDI port names, edit the profile's `inputs`/`outputs`; `--list` shows available ports. See [PROFILES.md](PROFILES.md) for the JSON format.

A Windows installation can keep the executable and profiles in `%LOCALAPPDATA%\ap-midi-osc-bridge\` with a shortcut directly to the executable; there are no command launchers. The app can attach/detach matching devices while running. Ambiguous selectors and competing profiles claiming the same port are reported rather than guessed.

## Supported mappings

| Device | Included layout |
|---|---|
| **Pioneer DJ DDJ-FLX4** | Four-deck selection (left 1↔3, right 2↔4 via Shift+LOAD), content-aware pads (including track cue-region loops), mixer/tempo pickup, FLX4 FX macros, transport and LED feedback. [Full FLX4 controls](MAPPING.md). |
| **Novation Launch Control 3 (compact)** | **DAW USB ports**, not Main MIDI or XL 3. Hot Cues, Mixer and FX button layers; EQ/filter in the first two, fixed CH1–4 FX Macro 1 wet/Macro 2 enable in the third, plus Browser. Fixed cue/FX colours label actions, not state. No Components remapping. [Full LC3 controls](MAPPING.md#launch-control-3-compact-daw-usb-ports). |

Missing Syndicate feedback can block ordinary pickup/toggles; LC3 also needs a confirmed hardware mode report. FX wet controls instead require a physical dry-home gesture. See [operation and troubleshooting](MAPPING.md) before changing assignments.

Control sheets: [LC3 HTML source](LAUNCH-CONTROL-3-GUIDE.html) · [LC3 printable PDF](LAUNCH-CONTROL-3-GUIDE.pdf) · [FLX4 HTML source](CONTROL-GUIDE.html) · [FLX4 printable PDF](CONTROL-GUIDE.pdf). HTML is the editable source; PDFs are rendered artifacts.

Custom assignments belong in the JSON profiles. The bridge never changes Syndicate audio settings or controller firmware.

## Build from source

No prebuilt binary is published on [GitHub Releases](https://github.com/Thevetat/ap-midi-osc-bridge/releases) yet. Install [Rust](https://rustup.rs/) and your platform build tools (Windows: MSVC/C++ Build Tools + Windows SDK; macOS: Xcode command-line tools; Linux: C compiler, `pkg-config`, ALSA development headers), then:

```sh
git clone https://github.com/Thevetat/ap-midi-osc-bridge.git
cd ap-midi-osc-bridge
cargo build --release --locked
```

The executable is `target/release/ap-midi-osc-bridge` (`.exe` on Windows). **Copy `profiles/` beside the release binary before running it**; debug builds from the repository can use source profiles. The manual **Build AP MIDI OSC Bridge** workflow packages Windows x64, macOS ARM64/Intel and Linux x64 artifacts but publishes no GitHub Release. Binaries are unsigned/unnotarized; Linux needs compatible ALSA/libc. See [development/packaging](DEVELOPMENT.md) and [evidence limits](DEVELOPMENT.md#evidence-and-limits).

`--list` enumerates ports without opening them; `--monitor INPUT` passively logs one input (no output/OSC); `--verbose` shows traffic, not proof of an app state change. Optional `settings.json` beside the executable can override endpoints and `osc_root`, e.g. `{"osc_root":"/Stage/Left/"}`. The canonical profile/default wire root is `/Syndicate/` (0.1.903+); feedback must match the configured root, case-insensitively. Set Syndicate's OSC API root to match. Old `/AutoPilot/` custom profiles need explicit edits, without automatic migration. See [profile/OSC settings](PROFILES.md#osc-settings).

## Limits

- `TempoSlider` input/feedback must be bipolar **−1..+1**; subscribe to both `TempoSlider` and `TempoSliderEnabled`. Unknown/false availability blocks tempo writes even without pickup. No legacy 0–1 tempo API. See [tempo operation](MAPPING.md#fixed-controls).
- Both FX layouts require enabled insert chains with CH1–4 Macro 1 wet/dry and Macro 2 mappable plugin enable (0 off, 1 on; invert bypass if needed). Wet knobs must physically reach exact 0/dry to arm; button colours/FLX4 FX LED do not report effect state. Jog/scratch, repeat scrubbing and crossfader remain unmapped. [Full assignments and limits](MAPPING.md).
- A supplied profile or successful compile is not hardware verification. Current FLX4+LC3 joint operation and Syndicate end-to-end behavior have not been established by observation. OSC is unauthenticated UDP; network loss/crashes cannot guarantee release delivery.

[Editable profile format](PROFILES.md) · [Adding another controller](CONTROLLER-PORTING.md) · [MIT license](LICENSE). Independent project, not affiliated with AlphaTheta/Pioneer DJ, Novation or Syndicate.
