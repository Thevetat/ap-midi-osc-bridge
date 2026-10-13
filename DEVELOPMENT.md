# Developer reference

[Setup](README.md) · [Controller operation](MAPPING.md) · [Profile schema](PROFILES.md)

## Build and package

Install stable Rust and native build tools: Windows needs the MSVC C++ Build Tools/Windows SDK; macOS needs Xcode command-line tools; Linux needs a C compiler, `pkg-config` and ALSA development headers (Debian/Ubuntu: `build-essential pkg-config libasound2-dev`). From the repository root:

```sh
cargo build --release --locked
```

The binary is `target/release/ap-midi-osc-bridge` (`.exe` on Windows). Copy `profiles/` beside it **before running**, including when using `cargo run --release`. Debug builds launched from the repository can use source profiles if none are beside the executable.

To package a target-specific build:

```powershell
cargo build --release --locked --target x86_64-pc-windows-msvc
powershell -NoProfile -File scripts/package.ps1 -Target x86_64-pc-windows-msvc
```

Use `pwsh` and the corresponding target on other platforms. The manual **Build AP MIDI OSC Bridge** workflow targets Windows x64, macOS ARM64/Intel and Linux x64; it uploads artifacts, not GitHub Releases. `scripts/package.ps1` stages the binary, profiles, four reference documents, both HTML/PDF guides, MIT license and dependency notices, then writes an archive and SHA-256 checksum into `dist/`. It replaces published files only after archive creation succeeds.

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
| `src/adapters/declarative.rs` | Note/CC bindings, hardware modes, software layers and feedback |
| `src/adapters/flx4/` | FLX4 decoding, configuration and pad banks |

Each runtime device record owns its profile, optional live session, retry timestamp and status. The bridge polls MIDI inventory about every two seconds and retries failures after about ten seconds or a port change. It resolves competing claims before attachment, detaches stale sessions before replacements, and normally isolates per-device errors. Runtime requests one fresh AP snapshot after an attachment batch; adapters request snapshots for relevant selection changes. Profiles parse and validate once at process start, not on hotplug. The `adapter.type` selects a typed configuration; attachment/reconnection clones that configuration without reparsing JSON or rerunning preflight. AP registration takes an explicit session owner and still rejects conflicting feedback scales across attached devices.

OSC packets are processed into the shared cache before notifying adapters. Each interested live device receives one batch of unique lowercase addresses, including invalid reports that cleared cached state. Adapters invalidate affected captures first, then refresh once; deck/mode/layer changes still refresh directly. Rendering borrows configuration instead of cloning whole feedback/LED lists. `feedback received` counts addresses processed by successful batches, not packets, MIDI writes or AP acknowledgments. A packet can contain only one message: batching does not promise a complete deck snapshot or a measured speedup.

CLI: `--list` lists ports without opening connections; `--monitor INPUT` passively reads one named input or `#index` without MIDI output/OSC; `--verbose` logs traffic and pickup counters. `#index` is not accepted for automatic matching. See [PROFILES.md](PROFILES.md) for `settings.json` and profile lookup rules.

## AP wire contract and safety

| Item | Contract |
|---|---|
| Transport | UDP to `127.0.0.1:9090`, feedback on `127.0.0.1:9096` by default; matching IP families required. Only the configured destination IP's feedback is accepted; UDP is unauthenticated/unencrypted. |
| Player types | Track, arrangement, clip and sampler routes differ; FLX4 blocks unsupported deck actions for unknown/empty types. |
| Scaling | TempoSlider input/feedback are −1..+1, normalized internally to `(value + 1) / 2`; mixer positions are 0..1. FLX4's two central tempo codes send exact zero. |
| Pickup | Initial reports establish a baseline, not movement. Missing/invalid feedback blocks capture; delayed feedback must not undo a captured control. AP does not identify mouse-only writers. |
| Ownership | Multiple devices may share a route. Confirmed switch feedback drives toggles; held releases retain the original resolved destination across Shift/mode/layer/deck changes. |
| Failure | Preserve bounded MIDI queues, bounded OSC processing, per-device cleanup and best-effort releases. Reconnect cannot guarantee recovery from UDP loss, AP restarts, crashes or force-kills. |

See [MAPPING.md](MAPPING.md) for exact assignments, subscriptions and unsupported controls. Feedback addresses alone do not prove writable inputs. `Filter1`/`Filter2` full input routes remain inferred from an AP change note, not confirmed across AP versions; AP may require controls to be visible.

## Adding another controller

1. **Establish the exact device.** Record model/revision, firmware, OS, USB/DIN MIDI versus HID, hardware mode, desired layout and AP version. Similar product names do not establish protocol compatibility. Do not inspect private AP mappings, change persistent modes, install drivers/firmware or probe hardware without authorization.
2. **Gather protocol evidence.** Read the exact model's official implementation/programmer's reference. A maintained implementation can corroborate hardware details, not AP mappings. Record URL plus page/section or repository revision/function, and distinguish documented, observed, inferred and unknown facts. Check PDF tables directly if extraction loses columns; respect licenses.
3. **Inventory both directions.** For each control/mode, record input port/channel/number/value and press/release semantics, output bytes/effects, and source/confidence. Check channel numbering (1–16 versus wire 0–15), decimal/hex, Note Off/zero-velocity Note On, shifted/short/long reports, CC resolution/MSB order, relative encoding, output echoes, mode confirmation and keepalive. Unknown output means omit that feedback—not borrow another controller's SysEx.
4. **Use the smallest implementation.** Documented Note/CC controls belong in a declarative profile with unique named selectors. Start input-only if output is unestablished. Use modes/layers/hooks only where justified; leave unknown gestures unassigned. Unsupported transports/decoders need a reviewed Rust adapter, not invented JSON fields or a speculative plugin system. Do not send FLX4 keepalive or LC3 DAW messages to unrelated hardware.
5. **Establish AP semantics separately.** Record `gesture → MIDI event → selected target → AP input route/value → required feedback`. Verify argument scale and trigger/toggle/momentary behavior. Account for track/arrangement/clip/sampler differences. An undocumented route stays unassigned; do not pretend waveform seek provides scratching or unrelated feedback proves cue occupancy.
6. **Preserve the contracts above.** Physical inputs must not collide across devices; release destinations survive selection changes. Keep last-held-repeat priority, real-feedback pickup, echo suppression and independent cleanup. Modes and layers remain distinct. Generic features unused by bundled profiles are not automatically obsolete.
7. **Deliver with evidence.** Update profiles, mappings, schema reference and printable guides together. Distinguish source inspection, compilation and observed hardware/AP behavior. Do not publish releases or modify installations as a side effect. If blocked, record sources, attempted paths, the unresolved fact and the smallest input needed.

`--list` does not open devices. Use `--monitor INPUT` only for authorized capture; it does not initialize hardware, so silence is inconclusive. Never run the normal bridge to discover an unsupported device's protocol.

## Evidence and limits

The workflow defines four build targets; that is not proof that each target builds at every revision. A successful local compile applies only to its source revision/target, not controller behavior. Current FLX4/LC3 joint hardware operation and AP end-to-end behavior have not been established by observation. Do not inherit coverage from earlier binaries or similarly named devices.

Keep experimental-use and upstream bug-reporting instructions prominent in [README.md](README.md). Record build/render evidence separately from hardware observations; none of the former establishes the latter. Preserve both guides and dependency notices when changing packaging.

## Protocol references

The exact-device and AP references are maintained in [PROFILES.md](PROFILES.md#protocol-references); detailed AP route references accompany [MAPPING.md](MAPPING.md#protocol-references).
