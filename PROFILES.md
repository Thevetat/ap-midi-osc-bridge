# Controller profiles

Put one `*.json` per controller in `profiles/` beside `ap-midi-osc-bridge.exe` (or the non-Windows executable). The app loads profiles automatically and matches their MIDI `inputs`/`outputs` selectors to currently available ports; absent controllers are skipped. `--list` prints input and output ports. Use an exact port name or unique, case-insensitive name substring in a profile; **`#index` is only for `--monitor`**, never automatic matching because indices can shift on hotplug. Ambiguous selectors or multiple profiles claiming the same port are reported, not guessed. Attach/detach is handled while the app runs; failures are identified by device. **Restart after editing a profile** to reload it. Keep the console open for status/errors.

The included [`profiles/flx4.json`](profiles/flx4.json) and [`profiles/launch-control-3.json`](profiles/launch-control-3.json) hold the default AP assignments. Editing a binding does not require changing Rust. **Do not send FLX4 keepalive or LC3 DAW output to another device.** Start from the exact device's published MIDI specification; unknown messages should stay unassigned.

## Declarative JSON (version 2)

```json
{
  "version": 2,
  "name": "Example MIDI controller",
  "protocol": "declarative",
  "inputs": ["YOUR UNIQUE MIDI INPUT"],
  "outputs": [],
  "bindings": [
    {"input":0,"kind":"note","channel":1,"number":36,
     "behavior":{"type":"button","address":"/AutoPilot/Deck/{deck}/PlayPause"}},
    {"input":0,"kind":"cc","channel":1,"number":7,
     "behavior":{"type":"absolute","address":"/AutoPilot/Mixer/CH1/Volume"}}
  ]
}
```

Required fields: `version:2`, `name`, `inputs` (1–8 selectors). Optional: `protocol` (defaults to `declarative`), `outputs` (0–8 selectors), `modes`, `layers`, `deck_feedback`, `bindings`, `feedback`, `startup`, `periodic`, `period_ms` (default 200), `shutdown`. Input/output array indices are **zero-based**, while declarative MIDI channels are **1–16** and Note/CC numbers and values **0–127**. Routes begin `/AutoPilot/` and may use `{deck}`; a feedback route does **not** prove an AP input with that path is writable. `deck_side` defaults to 0; `select_deck` chooses deck 1–4 for that side.

| Feature | Mapping fields |
|---|---|
| Modes | `modes:[{"name":"eq","reports":[{"input":0,"kind":"cc","channel":7,"number":30,"value":1}]}]`; bindings/feedback can set `mode:"eq"`. Multiple modes each need reports. Startup output does not confirm a mode; only the hardware report does. |
| Layers | `layers:[{"name":"hot_cues","on_enter":[{"output":0,"bytes":[176,37,5]}]},{"name":"mixer"}]`. The first is the connection default; `behavior:{"type":"cycle_layer"}` advances through them. Bindings/feedback can set `layer`; it combines with `mode`, rather than replacing hardware-mode reports. Layer changes leave unscoped encoders alone and preserve held releases. |
| Selection indicators | `deck_feedback:[{"deck":1,"deck_side":0,"messages":[{"output":0,"bytes":[176,103,0]}]}]` sends local deck indicators. Selected-deck hooks and the current layer's `on_enter` hooks are reapplied after selection or confirmed hardware-mode changes. They are not AP state feedback. Up to 16 layers and 16 unique side/deck entries; each hook has the same 32-message/64-byte SysEx bounds as lifecycle hooks. |
| Buttons | `behavior:{"type":"button","address":"/AutoPilot/...","press":1,"release":0}`; omit `release` for a trigger. `toggle` requires confirmed 0/1 AP feedback; `select_deck` has `deck:1..4`; `step_deck` has `delta:-1` or `1` and stops at Deck 1/4. An optional binding-level `press_value:127` accepts only that nonzero button value (useful to exclude LED-colour echoes); omitted means any nonzero press. Held releases retain their original resolved destination across layer changes. |
| Continuous | `absolute` (7-bit CC), `absolute_14` (MSB CC 0–31 plus LSB number+32), `relative` (two's-complement CC, `up`/`down` routes) and `recenter` (`up`/`down`, `output`, optional `center` default 64). Absolute uses `address`, optional `pickup` (default true) and `bipolar` (false = AP 0..1; true = AP −1..+1 with matching feedback normalized). Enable bipolar for an AP route such as TempoSlider only when its wire scale requires it. Missing feedback blocks pickup. Recenter's first non-echo report after changing layer is a baseline and can consume the first turn. |
| Feedback | `address`, `output`, `midi_kind` (`note`/`cc`), `channel`, `number`, optional `mode`, `layer`, `deck_side`, `echo_input`, then `kind:"led"` with optional `on`/`off`/`unknown`, or `kind:"position"` for AP-synchronized encoders. For a single input, echo port 0 is inferred. Echo suppression matches expected values for up to 500 ms; identical physical reports may be dropped, and it is not an acknowledgment. |
| Hooks | `startup`, `periodic`, `shutdown` arrays of `{"output":0,"bytes":[176,37,0]}`. Use only documented three-byte channel messages or bounded SysEx. Periodic messages require `period_ms` 20–60000. |

Declarative `protocol_config` must be absent/null. There are no dynamic scripts or user-code plugins. Raw SlipLoop/Dotted/Triplet routes requiring bridge-owned timing are rejected in declarative mappings; unsupported HID, NRPN and jog/scratch protocols need a reviewed capability rather than guessed messages.

## FLX4 and legacy pads

`profiles/flx4.json` uses `protocol:"flx4"` and `protocol_config` with `buttons`, `analog`, `browser_encoder`, `leds`, `pad_wiring`, and `pads`. Its **hardware channels are zero-based MIDI nibbles** (unlike declarative 1–16). Its ordinary AP routes, controls and eight pad banks are editable; keep the hardware wiring consistent with the DDJ-FLX4. `pads` can embed a version-1 FLX4 pad layout **or name a version-1 pad JSON file relative to the containing profile**. Existing pad files can therefore be reused without converting them. The pad layout has eight banks of eight actions/nulls; an action has `address`, integer `press`, optional integer `release`; optional `shifted_pads` and `for_player` (track/arrangement/clip/sampler) replace layouts. `slip_timing` supports `straight`, `dotted`, `triplet`, `selected` for SlipLoop holds. Old version-1 **generic controller** JSON is not a version-2 profile and requires manual migration.

Both devices share one AP OSC client (default input `127.0.0.1:9090`, feedback `127.0.0.1:9096`), but have separate physical holds, pickup and device-specific MIDI output. Optional `settings.json` accepts only `osc`/`feedback` socket addresses. Release builds read it beside the executable; debug builds using repository profiles read it from that repository directory. Bindings must have matching AP feedback subscriptions for pickup, switch toggles and hardware position. The compact LC3 profile uses the DAW USB input/output (not Main MIDI or XL 3), DAW Mixer/Control reports, encoder positions and LED colors without Components remapping; see the [controls overview](README.md#supported-mappings).

## Protocol references

- [Official DDJ-FLX4 MIDI message list](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_MIDI_message_List_E1.pdf), pp. 1–3 and p. 5 footnote 3: FLX4 MIDI controls and SYNC reports. [Mixxx DDJ-FLX4 implementation](https://github.com/mixxxdj/mixxx/blob/main/res/controllers/Pioneer-DDJ-FLX4-script.js), `sendKeepAlive`, `padModeKeyPressed`, `vuMeterUpdate`: hardware-protocol reference, not an AP mapping.
- [Novation Launch Control 3 programmer's DAW-mode guide](https://userguides.novationmusic.com/hc/en-gb/articles/33742551451666-Launch-Control-3-programmer-s-DAW-mode), “DAW mode control”, “The surface in DAW mode”, “Mode report and select”, “Colouring the surface”, “Absolute and relative encoder modes”: DAW ports, messages, encoders and LEDs. [Novation user guide v1.1](https://fael-downloads-prod.focusrite.com/customer/prod/downloads/launch_control_3_user_guide_v1.1_pdf_en.pdf), pp. 8, 11–12, 14–16, 29: operating modes and hardware layout.
- [Bitwig compact-LC3 implementation](https://github.com/bitwig/bitwig-extensions/blob/main/src/main/java/com/bitwig/extensions/controllers/novation/launchcontrolxlmk3/layer/LcMixerLayer.java), `bindNavigation`: Function uses `MUTE_SELECT_MODE`, Track arrows navigate the selected track. [CC constants](https://github.com/bitwig/bitwig-extensions/blob/main/src/main/java/com/bitwig/extensions/controllers/novation/launchcontrolxlmk3/CcConstValues.java): Function 66, Track Left 103 / Right 102; buttons use channel 1. Novation's DAW-mode guide “Controlling the screen” specifies text/display messages used by the profile.
- [AP OSC guide §13.4](https://autopilot.audio/user-guide/index.html?c=c13-4): OSC input/feedback semantics; §4.6 describes Browser CONTROL MODE. Its recent-change note names `Filter1`/`Filter2` while the mixer route table still lists older names: treat their full input paths as unconfirmed.
