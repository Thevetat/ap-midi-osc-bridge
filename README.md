# AP MIDI OSC Bridge

> [!WARNING]
> **AUTO/PILOT is in beta and changing rapidly. This bridge is experimental—use it entirely at your own risk and discretion.** Updates to AUTO/PILOT may break functionality or cause unexpected behaviour.
>
> This is an independent, unofficial project. **Do not report bridge-related issues to the AUTO/PILOT Discord, its developers, or its support channels.** Keep those reports in this repository's issue tracker.
>
> **Before reporting any suspected AUTO/PILOT bug, fully stop this bridge, restart AUTO/PILOT, and verify that the bug still occurs without the bridge running.** If it only occurs with the bridge active, report it here instead.

A native MIDI → OSC bridge for [AUTO/PILOT](https://autopilot.audio/). Put controller mappings in `profiles/` beside the executable: on startup, the app matches connected MIDI ports to those profiles and opens the controllers it finds. No controller selection or device-list configuration is needed.

## Get started

1. Connect your controller. In AUTO/PILOT, **disable that controller's direct MIDI input** to prevent duplicate actions. Enable OSC input on **9090**, OSC feedback to **127.0.0.1:9096** at 30 Hz, and **Toggle All** feedback subscriptions (including TempoSlider). Set **Settings → General → Browser Mode → CONTROL MODE** for hardware browsing.
2. Extract the app and `profiles/` together. On Windows, double-click **`ap-midi-osc-bridge.exe`**; on macOS/Linux, run `./ap-midi-osc-bridge` from its directory. Keep the console open while playing; **Ctrl+C** requests best-effort release of held controls. Do not run from inside the ZIP.
3. For another controller, add an editable JSON profile to `profiles/`. For different MIDI port names, edit that profile's `inputs`/`outputs`; `--list` shows available ports. See [PROFILES.md](PROFILES.md).

A Windows installation can keep the executable and profiles in `%LOCALAPPDATA%\ap-midi-osc-bridge\` with a shortcut directly to the executable; there are no command launchers. The app can attach/detach matching devices while running. Ambiguous selectors and competing profiles claiming the same port are reported rather than guessed.

## Supported mappings

| Device | Included layout |
|---|---|
| **Pioneer DJ DDJ-FLX4** | Four-deck selection (left 1↔3, right 2↔4 via Shift+LOAD), content-aware pads, mixer/tempo pickup, transport and LED feedback. [Full FLX4 controls](MAPPING.md). |
| **Novation Launch Control 3 (compact)** | Built-in DAW Mixer/Control modes over the **DAW USB ports**, not the Main MIDI port or the XL 3. Mixer mode maps CH1–4 EQ; Control mode maps CH1–4 Filter1/Filter2. Both provide Browser via top-right encoder. Bottom buttons default to Hot Cues 1–8; Track ◀/▶ selects Deck 1–4. Function toggles to the existing CH1–4 Mute/headphone Cue buttons. Screen shows deck/layer; cue colours identify slots, not populated cues. No Novation Components remapping is needed. |

The LC3 needs AP feedback subscriptions for all four channels' `EQ/Low`, `EQ/Mid`, `EQ/High`, `Filter1`, `Filter2`, `Mute` and `Cue`; **Toggle All** includes these. A controller with missing feedback may have blocked pickup/toggles rather than moving AP blindly. Leave `profiles/launch-control-3.json` on its DAW-port selectors; if OS port names differ, use `--list` and edit the selectors. A mode is active only after the LC3 reports it. Browser needs AP CONTROL MODE.

Launch Control 3: [printable control guide](LAUNCH-CONTROL-3-GUIDE.pdf) · [offline HTML guide](LAUNCH-CONTROL-3-GUIDE.html).

FLX4: [printable control guide](CONTROL-GUIDE.pdf) · [offline HTML guide](CONTROL-GUIDE.html). These retain legacy FLX4 Pilot naming and a removed `--direct-tempo` reference; use the setup instructions above and [MAPPING.md](MAPPING.md) for current behaviour. For pickup, pads, sampler steps and troubleshooting, see the [user guide](USER-GUIDE.md).

Custom assignments belong in the JSON profiles. The bridge never changes AP audio settings or firmware.

## Build from source

No prebuilt binary is published on [GitHub Releases](https://github.com/Thevetat/ap-midi-osc-bridge/releases) yet. Install [Rust](https://rustup.rs/) and your platform build tools (Windows: MSVC/C++ Build Tools + Windows SDK; macOS: Xcode command-line tools; Linux: C compiler, `pkg-config`, ALSA development headers), then:

```sh
git clone https://github.com/Thevetat/ap-midi-osc-bridge.git
cd ap-midi-osc-bridge
cargo build --release --locked
```

The executable is `target/release/ap-midi-osc-bridge` (`.exe` on Windows). **Copy `profiles/` into `target/release/` before running that release binary**, or keep the folder beside the executable when moving it. Debug builds run from the repository can use the repository's `profiles/` as a development fallback. The manual **Build AP MIDI OSC Bridge** workflow packages Windows x64, macOS ARM64/Intel and Linux x64 artifacts; it does not publish a GitHub Release. Binaries are unsigned/unnotarized, and Linux builds need compatible system ALSA/libc. See the [developer reference](DEVELOPMENT.md) for packaging/CLI details and [release status](STATUS.md) for validation gaps.

`--list` enumerates ports without opening MIDI connections; `--monitor INPUT` logs one input only, with no MIDI output or OSC; `--verbose` shows MIDI input and OSC sends (not proof of an AP state change). Optional `settings.json` beside the executable can override only the default endpoints, for example `{"osc":"127.0.0.1:9090","feedback":"127.0.0.1:9096"}`. If omitted, those defaults apply.

## Limits

- AP's `TempoSlider` input **and feedback** must be bipolar **−1..+1** (0 = native tempo); old 0–1 APIs are unsupported. Missing feedback blocks normal tempo pickup. `Filter1`/`Filter2` full input paths are inferred from an AP change note while its published mixer table still uses older names; confirm against your AP version.
- FLX4 jog/scratch, repeat scrubbing, crossfader and FX are not mapped. Clip/sampler coverage differs from track/arrangement; see [MAPPING.md](MAPPING.md). Hardware/OS compatibility and FLX4+LC3 joint operation have not been established across all systems. OSC is unauthenticated UDP; network loss/crashes cannot guarantee release delivery.

[Editable profile format](PROFILES.md) · [Adding another controller](CONTROLLER-PORTING.md) · [MIT license](LICENSE). Independent project, not affiliated with AlphaTheta/Pioneer DJ, Novation or AUTO/PILOT.
