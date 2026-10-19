# Developer reference

[Setup](README.md) · [Controller operation](MAPPING.md) · [Profile schema](PROFILES.md)

## Build and package

Install stable Rust and native build tools: Windows MSVC C++ Build Tools/Windows SDK; macOS Xcode command-line tools; Linux C compiler, `pkg-config` and ALSA headers (Debian/Ubuntu: `build-essential pkg-config libasound2-dev`). From the repository root:

```sh
cargo build --release --locked
```

The binary is `target/release/ap-midi-osc-bridge` (`.exe` on Windows). Copy `profiles/` beside it **before running**, including when using `cargo run --release`. Debug builds launched from the repository can use source profiles if none are beside the executable.

To package a target-specific build:

```powershell
cargo build --release --locked --target x86_64-pc-windows-msvc
powershell -NoProfile -File scripts/package.ps1 -Target x86_64-pc-windows-msvc
```

Use `pwsh` and the corresponding target on other platforms. The manual **Build AP MIDI OSC Bridge** workflow targets Windows x64, macOS ARM64/Intel and Linux x64; it uploads artifacts, not GitHub Releases. `scripts/package.ps1` stages the binary, profiles, reference documents including the controller-porting guide, both HTML/PDF guides, MIT license and dependency notices, then writes an archive and SHA-256 checksum into `dist/`. It replaces published files only after archive creation succeeds.

Windows uses the static MSVC runtime; Linux needs compatible system ALSA/libc. Binaries are unsigned/unnotarized. A checksum detects corruption, **not** publisher identity. Historical `dist/` files are not evidence of the current source's build or hardware compatibility.

### Printable guides

HTML is the editable source; PDFs are rendered artifacts. With Chrome or Chromium installed, render both guides from the repository root:

```powershell
powershell -NoProfile -File scripts/render-guides.ps1 -Browser 'C:\Program Files\Google\Chrome\Application\chrome.exe'
```

On other platforms use `pwsh` and the browser executable's path. The script uses a fresh temporary browser profile, prints without browser headers/footers, and stages both PDFs before replacing either. It does not use your normal browser session or start the bridge. Rendering is not a visual or hardware verification; browser version and installed fonts may affect output.

## Architecture and runtime

| Source | Responsibility |
|---|---|
| `src/main.rs` | CLI, executable-relative profiles/settings lookup, exit handling |
| `src/midi.rs` | Port inventory/matching and read-only list/monitor tools |
| `src/runtime.rs` | Discovery, port ownership, per-device lifecycle records and OSC dispatch |
| `src/ap.rs` | Confirmed AP feedback, shared ownership, held actions and slip timing |
| `src/profile.rs` | Device identity, declarative configuration and pure preflight |
| `src/adapters/mod.rs` | Typed adapter variants, one-time loading/validation and lifecycle factory |
| `src/adapters/declarative.rs` | Note/CC bindings, reported hardware modes, shared software layers, dry-home FX and feedback |
| `src/adapters/flx4/` | FLX4 decoding, configuration and pad banks; `fx.rs` owns optional insert-macro FX gestures and dry-home safety |

Each runtime device record owns its profile, optional live session, retry timestamp and status. The bridge polls MIDI inventory about every two seconds and retries failures after about ten seconds or a port change. It resolves competing claims before attachment, detaches stale sessions before replacements, and normally isolates per-device errors. Runtime requests one fresh AP snapshot after an attachment batch; adapters request snapshots for relevant selection changes. Profiles parse and validate once at process start, not on hotplug. The `adapter.type` selects a typed configuration; attachment/reconnection clones that configuration without reparsing JSON or rerunning preflight. AP registration takes an explicit session owner and still rejects conflicting feedback scales across attached devices.

OSC packets are processed into the shared cache before notifying adapters. Each interested live device receives one batch of unique lowercase addresses, including invalid reports that cleared cached state. Adapters invalidate affected captures first, then refresh once; deck/mode/layer changes still refresh directly. Rendering borrows configuration instead of cloning whole feedback/LED lists. `feedback received` counts addresses processed by successful batches, not packets, MIDI writes or AP acknowledgments. A packet can contain only one message: batching does not promise a complete deck snapshot or a measured speedup.

CLI: `--list` lists ports without opening connections; `--monitor INPUT` passively reads one named input or `#index` without MIDI output/OSC; `--verbose` logs traffic and pickup counters. `#index` is not accepted for automatic matching. See [PROFILES.md](PROFILES.md) for `settings.json` and profile lookup rules.

## AP wire contract and safety

| Item | Contract |
|---|---|
| Transport/root | UDP defaults: input `127.0.0.1:9090`, feedback `127.0.0.1:9096`; IP families must match. Canonical/default root `/Syndicate/`; outgoing routes are translated to configured `osc_root`, feedback only from that root (case-insensitive) and configured destination IP. UDP is unauthenticated/unencrypted. Syntax and old-profile migration: [OSC settings](PROFILES.md#osc-settings). |
| Player types | Track, arrangement, clip and sampler routes differ; FLX4 blocks unsupported deck actions for unknown/empty types. |
| Scaling/availability | TempoSlider input/feedback are −1..+1, normalized internally to `(value + 1) / 2`; mixer positions are 0..1. FLX4's two central tempo codes send exact zero. Subscribe to TempoSlider and TempoSliderEnabled; unknown/false availability blocks tempo writes even without pickup. Disable/enable transitions re-arm pickup; adapters are notified only on availability value transitions or invalid reports, not repeated unchanged snapshots. |
| Pickup | Initial reports establish a baseline, not movement. Missing/invalid feedback blocks capture; delayed feedback must not undo a captured control. AP does not identify mouse-only writers. With change-only AP feedback, a pending continuous write triggers `Feedback/Resend` only if another physical control has tried to take over and the original write remains unanswered for at least 250 ms; globally at most once per second. Uncontested writes/output-only controls do not poll; no timeout guesses success and no-op writes cannot depend on an echo. |
| Ownership | Multiple devices can share a route; confirmed feedback drives toggles, and held releases retain their original destination across Shift/mode/layer/deck changes. FX wet controls use no macro feedback: first report baselines, physical exact 0/dry arms, relevant target/layer/mode or known-writer changes re-arm. FX enable uses explicit Macro 2 OSC Float 1.0/0.0, not chain `/FX` or a toggle. Declarative `macro_button` latches Note/CC presses and registers Unit-scale known writers to re-arm competing dry-home controls; it does not invent feedback. No startup/shutdown macro writes or state LEDs. |
| Failure | Preserve bounded MIDI queues, bounded OSC processing, per-device cleanup and best-effort releases. Reconnect cannot guarantee recovery from UDP loss, AP restarts, crashes or force-kills. |

[MAPPING.md](MAPPING.md) owns exact assignments and subscriptions; feedback routes alone do not establish writable inputs. Syndicate documents `Filter1`/`Filter2` and mixer `FX/Macro/{1-8}` in [§14.9](https://autopilot.audio/user-guide/index.html?c=c14), track-only `SetCueLoop` in [§14.6](https://autopilot.audio/user-guide/index.html?c=c14), and the root change in [recent changes 0.1.903](https://autopilot.audio/user-guide/index.html?c=c17). Controls may require visibility. Version 0.1.901 retired learned/profile OSC assignments and the dedicated FLX4 preset; this independent bridge still sends fixed OSC routes.

## Adding another controller

Use the standalone [controller-porting guide](CONTROLLER-PORTING.md) for the evidence-gathering workflow, protocol inventory, declarative-versus-adapter decision, implementation locations, safety invariants and permission-scoped capture instructions. Keep it as the authoritative adaptation walkthrough rather than duplicating or compressing it into this architecture reference.

## Evidence and limits

The workflow defines four build targets; that is not proof that each target builds at every revision. A successful local compile applies only to its source revision/target, not controller behavior. Current FLX4/LC3 joint hardware operation and Syndicate end-to-end behavior have not been established by observation. Do not inherit coverage from earlier binaries or similarly named devices.

Keep experimental-use and upstream bug-reporting instructions prominent in [README.md](README.md). Record build/render evidence separately from hardware observations; none of the former establishes the latter. Preserve both guides and dependency notices when changing packaging.

## Protocol references

The exact-device and AP references are maintained in [PROFILES.md](PROFILES.md#protocol-references); detailed AP route references accompany [MAPPING.md](MAPPING.md#protocol-references).
