# Adding another controller

[Developer contract](DEVELOPMENT.md) · [Profile schema](PROFILES.md) · [Default mappings](MAPPING.md) · [Release evidence](STATUS.md)

This is a research and implementation guide, **not** a claim that any MIDI device works merely because it has a JSON profile. Version-2 declarative profiles map supported MIDI Note/CC events, optional mode reports and documented device output. FLX4 uses a dedicated hardware capability. HID, NRPN, jog/scratch and undocumented device handshakes are not provided by filling in arbitrary numbers. Never send FLX4 keepalive or LC3 DAW output to a different model.

## 1. Establish scope

Record the exact manufacturer/model and revision, operating system, firmware and device mode; USB or DIN MIDI vs HID; the desired deck/mixer layout; and the user's AP version. Decide whether existing FLX4/LC3 behaviour must be preserved. A similarly named model or adjacent manual does not establish protocol compatibility. Ask before inspecting private AP mappings or changing persistent device modes. Do not change AP settings, install drivers or firmware, or run live MIDI probes without the user's authorization.

## 2. Gather protocol evidence

Start with the **exact model's official MIDI implementation/programmer's reference** and operating manual. Read its input/output directions, MIDI ports, mode reports, control messages, feedback, initialization and shutdown. A maintained implementation for that device may corroborate protocol details, but its application mappings do not establish AP routes. Record source URL, page/section or repository revision/function, and separate **documented**, **observed**, **inferred** and **unknown** facts. Check PDF tables directly if text extraction loses columns. Respect other projects' licenses.

Before assigning controls, inventory:

| Physical control / mode | Device → host bytes and meaning | Host → device bytes and effect | Source / confidence |
|---|---|---|---|
| Button, pad, encoder, fader, mode report… | Port, channel, number, value/press/release encoding | LED, mode, position, handshake or none | Exact reference and unresolved questions |

Check 1–16 channel numbering versus 0–15 wire nibbles, decimal versus hex, zero-velocity Note On versus Note Off, shifted messages, native short/long press, CC resolution and paired-MSB order, absolute versus relative encoder encoding, and distinct input/output addresses. Confirm whether output is echoed as input, whether a mode changes future MIDI messages, and whether keepalive is required. Unknown output is a reason to omit LED/meter feedback, **not** to try another device's SysEx.

Use `ap-midi-osc-bridge --list` to inspect port names without opening devices. `--monitor INPUT` passively reads one selected input only, without MIDI output or OSC; use it only when hardware capture is authorized. Some devices require initialization before emitting controls, so silence from passive monitoring is inconclusive. Do not use a normal running bridge as a discovery tool for unsupported hardware.

## 3. Choose the smallest implementation

For documented Note/CC controls supported by the declarative schema, add a new version-2 JSON in `profiles/` using [PROFILES.md](PROFILES.md#declarative-json-version-2). Give input/output selectors unique, stable names; `#index` is only for monitoring. Prefer a minimal input-only profile when output is unestablished. Use supported `bindings`, `modes`, `layers`, `feedback` and bounded lifecycle hooks only where the actual device protocol justifies them. Unknown gestures stay unassigned. Profiles are loaded on process start, so restart after edits.

A controller with unsupported transport, message encoding, state management or device-specific output needs a reviewed Rust capability, not a guessed JSON field. Separate its decoder/output from the shared AP OSC and session logic; preserve other devices' independent sessions and their profile compatibility. Inspect the current `src/` layout before modifying it. Do not make a speculative general-purpose plugin system.

For each binding, document `gesture → MIDI event → selected deck/channel → AP input route/value → required AP feedback`. Confirm route input argument, scale and trigger/toggle/momentary semantics with AP documentation or direct permission-based evidence. Feedback addresses alone do not prove an input is writable. Distinguish track/arrangement, clip and sampler capabilities; avoid pretending a jog can scratch through an unrelated waveform-seek route. If an AP route is unconfirmed, document the gap and leave it unassigned.

## 4. Preserve safety behaviour

- **Physical identity:** a press held through Shift, mode, layer, deck selection or disconnect must release its original resolved AP address. Different physical inputs and different devices must not collide.
- **Scaling and pickup:** decode the hardware's actual resolution and direction before normalizing to 0..1. Use real AP feedback and movement to capture an absolute control; missing feedback must not imply zero. AP TempoSlider is bipolar −1..+1 in both directions, unlike normalized mixer values. A mode/layer or destination change invalidates a stale pickup baseline; delayed feedback should not undo an already captured control.
- **Shared state:** confirmed switch feedback is needed for toggles. Multiple devices can target the same AP route; preserve bridge-level ownership and last-held-repeat priority. Do not infer cue occupancy, sampler step state or LED colour from unrelated feedback.
- **Failure:** maintain bounded MIDI queues and OSC processing, feedback-source filtering, detach cleanup and best-effort releases over UDP. Do not promise recovery from force-kills or packet loss.

## 5. Deliver with evidence

Update profile documentation, mapping, user instructions, guides and archive contents for any newly supported device. Record what was only **source-reviewed**, what was **compiled**, and what was **observed on hardware/AP**, separately; do not inherit another device's validation status. Review [STATUS.md](STATUS.md) before making compatibility claims. Do not publish releases or change the user's machine as an incidental step. If a required protocol or AP fact cannot be established, stop with the sources consulted and the smallest missing evidence needed.

## Starting references

- [AlphaTheta DDJ-FLX4 MIDI message list](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_MIDI_message_List_E1.pdf), pp. 1–3 and p. 5 footnote 3: FLX4 messages and SYNC timing. [Mixxx DDJ-FLX4 implementation](https://github.com/mixxxdj/mixxx/blob/main/res/controllers/Pioneer-DDJ-FLX4-script.js), `sendKeepAlive`, `padModeKeyPressed`, `vuMeterUpdate`: a hardware-protocol reference, not an AP mapping.
- [Novation Launch Control 3 programmer's DAW-mode guide](https://userguides.novationmusic.com/hc/en-gb/articles/33742551451666-Launch-Control-3-programmer-s-DAW-mode), “Mode report and select”, “Colouring the surface” and “Absolute and relative encoder modes”: ports, mode and feedback formats.
- [AP OSC guide §13.4](https://autopilot.audio/user-guide/index.html?c=c13-4): AP OSC input/feedback semantics; verify exact routes for the installed version.
