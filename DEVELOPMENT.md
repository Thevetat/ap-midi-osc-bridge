# Developer reference

[Quick start](README.md) · [User guide](USER-GUIDE.md) · [Mappings](MAPPING.md) · [Profile schema](PROFILES.md) · [Adding a controller](CONTROLLER-PORTING.md)

The bridge loads version-2 device profiles from `profiles/` beside the release executable. MIDI devices have independent sessions; AP OSC feedback, route ownership and held-action cleanup are coordinated across them. FLX4 has a dedicated hardware capability and editable pad bindings; declarative profiles handle documented Note/CC controllers. JSON changes need a bridge restart. Do not assume a successful compilation establishes device or AP compatibility.

## Build and package

Install stable Rust and native build tools: Windows needs the MSVC C++ Build Tools/Windows SDK, macOS the Xcode command-line tools, and Linux a C compiler, `pkg-config` and ALSA development headers (Debian/Ubuntu: `build-essential pkg-config libasound2-dev`). From the repository root:

```sh
cargo build --release --locked
```

The resulting binary is `target/release/ap-midi-osc-bridge` (`.exe` on Windows). Copy `profiles/` next to it **before running**. `cargo run --release --locked` starts a release binary from `target/release/`; it likewise needs that adjacent folder. Debug builds launched from the repository can fall back to repository profiles. To make a native archive after building for a target:

```powershell
cargo build --release --locked --target x86_64-pc-windows-msvc
powershell -NoProfile -File scripts/package.ps1 -Target x86_64-pc-windows-msvc
```

Use `pwsh` and the corresponding target on other platforms. The manual **Build AP MIDI OSC Bridge** workflow builds Windows x64, macOS ARM64/Intel and Linux x64 artifacts; it uploads workflow artifacts, not a GitHub Release. `scripts/package.ps1` stages the binary, profiles, docs, both control guides, MIT license and dependency notices in a temporary directory, then writes the archive and `.sha256` checksum into `dist/`. The checksum detects corruption, **not** publisher identity. Windows uses the static MSVC runtime; Linux needs compatible system ALSA/libc. Binaries are unsigned/unnotarized. See [STATUS.md](STATUS.md) for evidence and gaps.

## Runtime and CLI

```text
ap-midi-osc-bridge --list
ap-midi-osc-bridge --monitor "#0"
ap-midi-osc-bridge --verbose
```

`--list` enumerates ports without opening MIDI connections; `--monitor INPUT` opens only one input by name or `#index` and sends no MIDI/OSC; `--verbose` logs traffic and pickup counters during normal operation. Only `--list`, `--monitor INPUT` and `--verbose` are supported. Legacy `--input`, `--output`, `--config`, `--export-config`, `--osc`, `--feedback`, `--mixer` and `--direct-tempo` options were removed. `#index` is not permitted for automatic profile matching because it can change after hotplug. `settings.json` beside the executable can override OSC destination and feedback bind addresses, e.g. `{"osc":"127.0.0.1:9090","feedback":"127.0.0.1:9096"}`; absent values use these defaults. Addresses must use the same IP family. OSC is unauthenticated, unencrypted UDP; the bridge accepts feedback only from the configured destination IP.

The bridge polls MIDI inventory about every two seconds, detaches sessions when their selected ports disappear/change, and retries failed attachments after about ten seconds or an inventory change. Per-device errors do not normally stop other sessions. A separate AP feedback socket is shared across sessions; only subscribed, registered routes feed each capability. Reconnect is not an AP-state recovery guarantee. Profile edits are not hot-reloaded.

## AP wire contract and safety

| Item | Contract |
|---|---|
| Destination / feedback | Default UDP `127.0.0.1:9090` / `127.0.0.1:9096`; AP feedback must be enabled and subscribed. |
| Deck player types | Track, arrangement, clip, sampler and empty have different capabilities; unknown/empty types block unsupported deck actions. |
| TempoSlider | AP input **and feedback** use float −1..+1 (0 is native tempo); bridge pickup stores `(feedback + 1) / 2`. Mixer position is 0..1, without global tempo scaling. |
| Missing state | Missing/invalid feedback blocks controls that need pickup or confirmed toggles. Never infer a writable input from a feedback-only route. |
| Holds | Resolve physical press destinations before layer/deck changes; retain that destination until release. Shared AP actions and last-held slip loops must not be released prematurely by another device. |
| Failure | Detach and Ctrl+C attempt releases and device shutdown; UDP loss, crashes or force-kills cannot guarantee AP receives them. |

For selective subscriptions, use [MAPPING.md#feedback-profiles-and-safety](MAPPING.md#feedback-profiles-and-safety). The FLX4's 14-bit tempo fader sends `2 * physical_position - 1`, with its two central MIDI codes mapped to exact zero; declarative absolute controls use the appropriate MIDI resolution and opt into bipolar mapping explicitly. `Filter1`/`Filter2` input paths remain inferred from an AP change note, not confirmed against all AP versions. AP may require a control to be visible.

## Change boundaries

- `src/main.rs`: CLI, executable-relative profile/settings lookup; `src/runtime.rs`: discovery, port ownership, sessions and OSC loop; `src/ap.rs`: AP feedback, shared ownership and held-action cleanup. Hardware capabilities and schema/validation live in the other `src/` modules; inspect the current source before changing a control.
- `profiles/flx4.json` and `profiles/launch-control-3.json`: supplied hardware selectors and editable default assignments. `PROFILES.md` documents the version-2 schema, inline FLX4 pad banks and unsupported protocols. Keep device-specific output and keepalive off unrelated hardware.
- `MAPPING.md`, `USER-GUIDE.md`, printable HTML/PDF sheets and `STATUS.md`: synchronize changed controls, compatibility claims and validation evidence. Update `scripts/package.ps1` whenever shipped docs change.

Do not modify AP mappings/settings, install firmware, publish a release or probe hardware as a side effect of editing this project. Establish AP input semantics separately from feedback semantics, and preserve normalized pickup, physical release identity, bounded queues and OSC processing. A new capability should document unresolved routes instead of guessing them.

## References

- [AlphaTheta DDJ-FLX4 MIDI message list](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_MIDI_message_List_E1.pdf), pp. 1–3 and p. 5 footnote 3: messages and SYNC timing.
- [AP OSC guide §13.4](https://autopilot.audio/user-guide/index.html?c=c13-4): OSC routes/feedback; §4.6 covers Browser CONTROL MODE.
- [Novation LC3 DAW-mode guide](https://userguides.novationmusic.com/hc/en-gb/articles/33742551451666-Launch-Control-3-programmer-s-DAW-mode), “Mode report and select” and “Colouring the surface”: device-specific mode and feedback protocol.
