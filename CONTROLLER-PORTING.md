# Adapting the bridge for another controller

For developers and AI agents adding a controller other than the supplied DDJ-FLX4 and compact Launch Control 3. Start with the exact controller's documentation, not a search-and-replace of FLX4 note numbers.

**This is an adaptation guide, not a list of supported devices.** Use an editable declarative profile when the existing decoder supports the protocol; add a small Rust adapter when it does not. Never send FLX4 keepalive, LC3 DAW initialization or another model's LED/SysEx messages to an unrelated device.

Read the [Syndicate wire contract](DEVELOPMENT.md#ap-wire-contract-and-safety), [profile schema](PROFILES.md) and [current mappings](MAPPING.md). Keep the shared wire/safety model while adapting device-specific decoding/output. Preserve both existing controllers and their independent sessions unless the user explicitly requests otherwise.

## 1. Establish the target and scope

Use information already supplied; ask only for missing details that affect implementation:

- Exact manufacturer/model, hardware revision, relevant firmware version and operating system. Similar appearance or model names do not establish compatibility; compact LC3 and LCXL3 are distinct devices.
- Operating mode and connection: USB MIDI, DIN MIDI through an interface, HID or proprietary transport. Identify the correct physical/virtual input and output ports.
- Desired controls and deck layout: two strips switching among four decks, four dedicated strips, a pad-only device, etc. Do not assign fixed track/clip/sampler roles to deck numbers.
- Syndicate version/API availability and any custom mappings to preserve. Read private mapping files only with permission; do not publish settings or treat saved feedback subscriptions as proof of writable APIs.
- Which controls need feedback, mode switching, display output, pickup or held actions. Unknown features may remain unassigned rather than delaying a useful minimal mapping.

Research and code inspection come first. Hardware capture, live operation, tests and publication require explicit authorization for that work. Do not change AP settings, install drivers, switch persistent device modes or write firmware as a side effect.

## 2. Collect evidence before assigning controls

Find the exact model's **official operating manual and MIDI implementation/programmer's reference**. Read modes, channels, message formats, feedback, initialization and shutdown. A maintained implementation such as a Mixxx or Bitwig extension for that exact model can corroborate hardware details; read both routing declarations and callbacks.

For each fact, record a source URL plus page/section or repository revision/function. Fetch and read the source: search snippets, adjacent models and recollections are not protocol evidence. Check the original PDF table when extraction loses column alignment. Keep **documented**, **observed with permission**, **inferred** and **unknown** separate.

Build an inventory before editing:

| Physical control / mode | Device → host message | Meaning / decoding | Host → device feedback | Evidence / status |
|---|---|---|---|---|
| Button, shifted button, pad, knob, encoder, fader… | Port; status/channel; note/CC or other encoding; values | Press/release, absolute position or relative delta | Address, range, colour/blink/mode effect, or none | URL + page/function; observations and gaps |

Include these commonly missed details:

- **Numbering and direction:** decimal versus hex; manual channels 1–16 versus wire nibbles 0–15. Define device→host and host→device explicitly instead of trusting ambiguous “MIDI IN” headings.
- **Buttons:** Note On/Off, zero-velocity Note On, release velocity, shifted notes/channels and firmware-generated short/long events. Do not invent a hold timer when firmware already classifies the gesture.
- **Continuous controls:** 7-bit CC, paired 14-bit CC, pitch bend, NRPN or another encoding; message order, endpoints, direction and centre detent. The declarative decoder does not support every MIDI encoding.
- **Encoders/jogs:** relative encoding, acceleration, touch messages and mode dependence. Different encoders on one device can use different formats.
- **Output:** LED/meter addresses need not match inputs. Record whether output changes hardware mode, future input messages, physical encoder positions or produces input echoes.
- **Initialization:** handshake/keepalive, interval, position snapshots and safe shutdown. Unknown hardware output is a reason to omit LEDs/meters, not try another device's SysEx. AP feedback for pickup remains necessary even without MIDI output.

Reference implementations establish hardware behavior, not AP input routes. Respect their licenses; this project's MIT license does not relicense copied code.

### How the existing adapters use that evidence

Use these as worked examples of boundaries, not as universal controller behavior:

| Evidence / design step | Current implementation to inspect |
|---|---|
| FLX4 MIDI implementation and native SYNC timing | `src/adapters/flx4/mod.rs`: `input`, `cc`, `physical_key`; `profiles/flx4.json`: button behaviors. Native short/long reports are not replaced by a bridge hold timer. |
| FLX4 output protocol, corroborated by the Mixxx implementation | Same module: `KEEPALIVE`, `tick`, `mode_feedback`, `refresh_leds`, `cleanup`. Inspect startup and exit output as well as periodic messages. |
| AP player-specific semantics | `src/ap.rs`: `Player::route`; FLX4 `route()` and player feedback. Tempo scaling is separate from normalized mixer/pickup state. |
| Compact LC3 DAW reports, position feedback and displays | `profiles/launch-control-3.json`: `adapter.modes`, `layers`, `deck_feedback`, `bindings`, `feedback`; declarative `changed_mode`, `emit`, `refresh`. The FX layer uses layer arrays and `absolute_dry_home` without macro feedback; see [schema](PROFILES.md#declarative-json-version-2). |
| Deck/bank/layer choices | [Controller mappings](MAPPING.md): FLX4 Shift+LOAD deck switching; LC3 independent cue target and button layer. These are bridge design choices, not universal MIDI gestures. |

The [protocol references](PROFILES.md#protocol-references) retain the official FLX4 MIDI list, Mixxx callbacks, Novation DAW guide, compact-LC3 Bitwig references and AP OSC guide, with relevant sections/functions. This provenance is **not hardware validation**; see [evidence limits](DEVELOPMENT.md#evidence-and-limits).

## 3. Map gestures to supported AP actions

Write `physical gesture → decoded event → selected deck/channel → AP input route/value → required feedback` for each control. Begin with useful, established operations; add performance banks only where the device and AP support them.

- Establish each AP input's exact route, argument type/range, units and trigger/toggle/momentary semantics. Establish feedback separately: a feedback path does not prove an input is writable.
- Account for track, arrangement, clip and sampler differences. FLX4 has `PlayerType`-aware routing; **declarative profiles substitute `{deck}` but do not inherit FLX4's player-route translation or pad overrides**. Choose explicit routes with documented scope; additional content-aware behavior needs an intentional implementation.
- For shared strips, choose an unambiguous selection gesture and visible indication. Keep deck selection, bank selection, held Shift and software layers distinct. Selection must not accidentally load, stop or change tempo master.
- Use confirmed feedback for toggles; prefer explicit on/off operations when appropriate and no confirmed state exists. Do not infer cue occupancy, sampler step state or colours from unrelated or stale feedback.
- Leave unsupported operations unassigned and explain why. A documented MIDI jog does not establish an AP scratch API. Waveform seeking is not a substitute for scratching or moving a held repeat window; “signed” alone does not establish fractional precision.

For wire scales and unresolved AP routes, use the [AP contract](DEVELOPMENT.md#ap-wire-contract-and-safety). Record gaps rather than guessing from neighboring controls or bypassing pickup.

## 4. Choose the smallest implementation

### A declarative profile when the existing decoder fits

Add `profiles/<device>.json`, using the [current schema](PROFILES.md#declarative-json-version-2). Root fields describe identity/ports; adapter settings live under `adapter:{"type":"declarative",...}`. Use stable unique selectors; `#index` is only for monitoring. Start with `outputs:[]` if output is unestablished.

The existing adapter supports Note/CC buttons (ordinary Int or FX-macro `macro_button` Float 0..1 on latched press), confirmed toggles, 7/14-bit absolute controls, FX-macro-only 7-bit dry-home controls, two's-complement relative controls, recentering encoders, deck selection, hardware-reported modes, software layers (including bindings shared by a layer-name array) and documented lifecycle/output hooks. Verify the particular encoding against the device; matching the word “encoder” is insufficient. Feedback settings use a nested `render` object. See the schema rather than copying FLX4's different hardware configuration.

Profiles parse and validate at startup, before MIDI attachment; reconnect reuses typed configuration. Restart after edits. Do not add a controller-specific launcher or application for a device the declarative adapter already covers.

### A dedicated adapter when protocol behavior is missing

| Location | What to inspect or adapt |
|---|---|
| `src/adapters/mod.rs`: `Adapter`, `Config`, `load`, `create` | Add a typed configuration variant and dispatch for a genuine new protocol. Validate once before ports open. No raw JSON blobs or alternate compatibility formats. |
| `src/adapters/<device>.rs` or a small device directory | Own decoding, physical-key normalization, hardware modes, initialization, keepalive, output and shutdown. Use the FLX4 adapter as an example, not a source of guessed constants. |
| `src/profile.rs` | Reuse declarative capabilities when they genuinely fit; do not expose an unsupported encoding through a guessed field. Keep unknown-field rejection and preflight bounds. |
| `src/runtime.rs` | Port matching, conflicts, per-device lifecycle, bounded callbacks and OSC batches are shared infrastructure—not per-controller replacements. |
| `src/ap.rs` | Shared feedback, ownership, holds and slip timing. Register routes with the explicit session owner and correct feedback scale. |
| `profiles/` | Keep assignments editable. Distinguish configurable assignments from hardware facts that have only one valid value. |

The adapter lifecycle is `start`, `input`, `tick`, `feedback`, `status`, `cleanup`. Feedback arrives as interested, unique lowercase addresses from one already-processed OSC packet, including invalid reports that cleared cached values. Invalidate affected captures before one hardware refresh; do not rebuild a full surface once per address. Preserve direct refreshes for local selection changes.

Keep the implementation isolated so FLX4 and LC3 continue to coexist. No speculative plugin framework, new crate or duplicate OSC client is needed. Unsupported transports such as HID require an explicit transport design, not a port-selector change. An input-only adapter must tolerate no outputs throughout startup, feedback and cleanup.

### Safety invariants

1. **Physical identity survives modifiers.** A press held across Shift/mode/layer/deck changes must release its original resolved action. Do not merge distinct controls; include device/session and input-port identity. Do not clear held owners merely because a layer changes.
2. **Decode before scaling.** Use the hardware's actual range, ordering, inversion, centre and pickup tolerance. FLX4's reusable MSB and a declarative control's fresh-MSB requirement are distinct; relative controls are not absolute positions.
3. **Normalize once.** Absolute position/pickup is internally 0–1. AP TempoSlider is −1..+1 in both directions; mixer values remain 0–1. FLX4 centre codes 8191/8192 are not universal.
4. **Pickup needs real feedback and movement.** Initial reports establish baselines, not permission to jump. Re-arm on relevant target/layer changes; never reuse stale pairing/history for a new destination. Missing targets stay blocked. Ordinary delayed Syndicate feedback does not revoke capture or identify mouse-only writers. **FX dry-home is different:** no documented macro feedback exists, so a physical exact 0/dry gesture is required after baseline; do not describe this as confirmed pickup.
5. **State belongs to its destination.** Preserve independent deck selection, per-deck banks/timing, confirmed toggles, last-held-repeat priority and shared release ownership. Do not reset another device's state during cleanup. Hardware mode and software layer are different state dimensions.
6. **Output is not confirmation.** Sending a selection hook does not confirm a hardware mode; sending OSC does not confirm AP acted. Position-sync and echo suppression are device-protocol decisions, not general permission to treat host output as human movement.
7. **Failure stays bounded.** Retain queue-overflow handling, OSC processing limits, feedback-source filtering, ambiguous-port rejection, independent detach/retry and best-effort releases. Do not promise recovery from force-kills, packet loss or AP restarts.

## 5. Resolve remaining unknowns only with permission

Do not use the normal bridge as a discovery tool on unsupported hardware. Do not send guessed SysEx, sweep LED addresses or change firmware experimentally. Missing handshake documentation is a blocker, not permission to experiment.

When the user authorizes inspection/capture, the current CLI separates port listing from input-only monitoring:

```sh
ap-midi-osc-bridge --list
ap-midi-osc-bridge --monitor "#0"
```

Choose the input index from the list; `#0` is only an example. Listing does not open MIDI connections. Monitoring opens one input but sends no MIDI output, keepalive or OSC; omitting `--monitor INPUT` starts the live bridge. Some devices need initialization before sending controls, so silence from passive capture is inconclusive.

Within the approved scope, record the device/mode, raw bytes and exact gesture: press/release, Shift transitions, encoder direction or fader endpoint. Resolve one unknown at a time. Passive-capture permission is not permission for output experiments or a performance session.

Without testing permission, use source inspection and compilation only. `cargo build --release --locked` does not launch the bridge; `cargo run` does. Do not add/run tests, hardware probes or manual QA by default. If blocked, report sources consulted, attempted paths, the unresolved fact and the smallest additional evidence needed.

## 6. Deliver a clearly scoped addition

- Provide the profile/adapter, protocol notes, intended controls and explicit unsupported operations or unresolved questions.
- Preserve existing FLX4/LC3 assignments and concurrent ownership. Keep one executable with automatic profiles; do not silently substitute a new controller's protocol for an existing one.
- Update profile documentation, mappings, setup instructions and printable controls to match the actual device. Keep Syndicate settings changes as user instructions unless separately authorized.
- Record **source-reviewed**, **compiled**, **hardware-observed** and **Syndicate end-to-end observed** separately, with revision/target/device/mode/Syndicate version as applicable. Do not present proposed checks as completed or inherit another device's validation claims.
- Include documentation in packages and update links and dependency notices. No publication, firmware changes or machine-specific deployment as incidental work.

The deliverable is a small, explained mapping with explicit gaps—not a claim that every control works because the binary compiled.
