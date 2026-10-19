# Controller profiles

Put one `*.json` per controller in `profiles/` beside `ap-midi-osc-bridge.exe` (or the non-Windows executable). The app loads profiles automatically and matches their MIDI `inputs`/`outputs` selectors to currently available ports; absent controllers are skipped. `--list` prints input and output ports. Use an exact port name or unique, case-insensitive name substring in a profile; **`#index` is only for `--monitor`**, never automatic matching because indices can shift on hotplug. Ambiguous selectors or multiple profiles claiming the same port are reported, not guessed. Attach/detach is handled while the app runs; failures are identified by device. **Restart after editing a profile** to reload it. Keep the console open for status/errors.

The bundled [`profiles/flx4.json`](profiles/flx4.json) and [`profiles/launch-control-3.json`](profiles/launch-control-3.json) hold the default Syndicate assignments. Editing a binding does not require changing Rust. **Do not send FLX4 keepalive or LC3 DAW output to another device.** Start from the exact device's published MIDI specification; unknown messages should stay unassigned.

## Profile structure

Required top-level fields: `version:2`, `name`, `inputs` (1–8 named selectors), and `adapter`. Optional `outputs` defaults to an empty array (0–8 selectors). The adapter is an object with an explicit `type`: `declarative` or `flx4`, followed by that adapter's settings. All objects reject unknown fields; there are no alternate or backward-compatible profile shapes. Device identity and port selectors stay outside the adapter.

Profiles are parsed into typed configuration and validated at discovery, before opening MIDI ports. Reconnects reuse that configuration; only restarting reloads file edits.

## Declarative JSON (version 2)

```json
{
  "version": 2,
  "name": "Example MIDI controller",
  "inputs": ["YOUR UNIQUE MIDI INPUT"],
  "outputs": [],
  "adapter": {
    "type": "declarative",
    "bindings": [
      {"input":0,"kind":"note","channel":1,"number":36,
       "behavior":{"type":"button","address":"/Syndicate/Deck/{deck}/PlayPause"}},
      {"input":0,"kind":"cc","channel":1,"number":7,
       "behavior":{"type":"absolute","address":"/Syndicate/Mixer/CH1/Volume"}}
    ]
  }
}
```

Inside `adapter`, `type:"declarative"` is required. Optional settings: `modes`, `layers`, `deck_feedback`, `bindings`, `feedback`, `startup`, `periodic`, `period_ms` (default 200), `shutdown`. Lists default to empty. The table below describes these adapter settings. Input/output array indices are **zero-based**, while declarative MIDI channels are **1–16** and Note/CC numbers and values **0–127**. Profile routes must use canonical `/Syndicate/` and may use `{deck}`; outbound addresses use the configured OSC root. A feedback route does **not** prove a Syndicate input at that path is writable. `deck_side` defaults to 0; `select_deck` chooses deck 1–4 for that side.

| Feature | Mapping fields |
|---|---|
| Modes | `modes:[{"name":"eq","reports":[{"input":0,"kind":"cc","channel":7,"number":30,"value":1}]}]`; bindings/feedback can set `mode:"eq"`. Multiple modes each need reports. Startup output does not confirm a mode; only the hardware report does. |
| Layers | `layers:[{"name":"hot_cues","on_enter":[{"output":0,"bytes":[176,37,5]}]},{"name":"mixer"},{"name":"fx"}]`. The first is the connection default; `behavior:{"type":"cycle_layer"}` advances through them. A binding or feedback entry may use `layer:"fx"` or `layer:["hot_cues","mixer"]` (nonempty, unique names declared in `layers`, max 16). It combines with `mode`, not hardware-mode reports. Layer changes preserve held releases; unchanged bindings shared between layers keep their capture. |
| Selection indicators | `deck_feedback:[{"deck":1,"deck_side":0,"messages":[{"output":0,"bytes":[176,103,0]}]}]` sends local deck indicators. Selected-deck hooks and the current layer's `on_enter` hooks are reapplied after selection or confirmed hardware-mode changes. They are not AP state feedback. Up to 16 layers and 16 unique side/deck entries; each hook has the same 32-message/64-byte SysEx bounds as lifecycle hooks. |
| Buttons | Ordinary `behavior:{"type":"button","address":"/Syndicate/...","press":1,"release":0}` sends an OSC **Int**; omit `release` for a trigger. `toggle` requires confirmed 0/1 Syndicate feedback; `select_deck` has `deck:1..4`; `step_deck` has `delta:-1` or `1` and stops at Deck 1/4. Optional binding-level `press_value:127` accepts only that nonzero press value (e.g. to exclude LED-colour echoes); omitted accepts any nonzero press. Held releases retain their original destination across layers. |
| FX macro buttons | `behavior:{"type":"macro_button","address":"/Syndicate/Mixer/CH1/FX/Macro/2","value":1.0}` sends one OSC **Float** on a latched Note/CC press; release only clears the latch (no OSC). `value` is finite and normalized 0..1. Optional binding-level `press_value` works as above. Only `address` and `value` are behavior fields: no `release`, `pickup`, `bipolar` or other button/absolute options. Address must be a canonical `/Syndicate/Mixer/CH{1-4}/FX/Macro/{1-8}` route (`CH{deck}` placeholder allowed). It registers a Unit-scale known bridge writer for dry-home handoff, without assuming macro feedback. |
| Continuous | `absolute` (7-bit CC), `absolute_14` (MSB CC 0–31 plus LSB number+32), `relative` (two's-complement CC, `up`/`down` routes), `recenter` (`up`/`down`, `output`, optional `center` default 64), and `absolute_dry_home` (7-bit CC, e.g. `{"type":"absolute_dry_home","address":"/Syndicate/Mixer/CH1/FX/Macro/1"}`). Absolute accepts optional `pickup` (default true) and `bipolar` (false = 0..1, true = −1..+1 feedback normalized). Missing feedback blocks pickup; TempoSlider requires `TempoSliderEnabled=1` even with pickup disabled, and availability changes/invalid reports re-arm (not unchanged snapshots). Recenter's first non-echo report after layer change can establish a baseline and consume a turn. `absolute_dry_home` is separate local safety, **not** confirmed pickup: first report baselines; an intentional physical move to exact 0 sends dry and arms. It accepts no `bipolar`/`pickup`, no position-feedback rendering, and only canonical `/Syndicate/Mixer/CH{1-4}/FX/Macro/{1-8}` inputs (or `CH{deck}` placeholder). |
| Feedback | `address`, `output`, `midi_kind` (`note`/`cc`), `channel`, `number`, optional `mode`, `layer` (string or layer-name array), `deck_side`, `echo_input`, and required `render:{"kind":"led","on":127,"off":0}` (optional colours/unknown), or `render:{"kind":"position"}` for feedback-synchronized encoders **other than** `absolute_dry_home`. Position rendering rejects LED colours. Echo port 0 is inferred for one input; expected-value echo suppression lasts up to 500 ms, can drop identical physical reports and is not acknowledgment. |
| Hooks | `startup`, `periodic`, `shutdown` arrays of `{"output":0,"bytes":[176,37,0]}`. Use only documented three-byte channel messages or bounded SysEx. Periodic messages require `period_ms` 20–60000. |

FLX4-specific settings are not accepted in a declarative adapter. There are no dynamic scripts or user-code plugins. Raw SlipLoop/Dotted/Triplet routes requiring bridge-owned timing are rejected in declarative mappings; unsupported HID, NRPN and jog/scratch protocols need a reviewed capability rather than guessed messages.

## FLX4 pads

`profiles/flx4.json` uses `adapter:{"type":"flx4",...}` with required `buttons`, `analog`, `browser_encoder`, `leds`, `pad_wiring`, and `pads`. FLX4 requires exactly one input and one output; declarative settings are not accepted. Its **hardware channels are zero-based MIDI nibbles** (unlike declarative 1–16). Its ordinary AP routes, controls and eight pad banks are editable. Optional `fx:{"wet_macro":1,"enable_macro":2}` maps FLX4 FX to mixer insert macros; omitting `fx` or setting it to `null` disables this capability. The macro indices must be distinct integers 1–8; the bundled profile enables 1/2. `pad_wiring` contains `bank_notes` (eight MIDI note numbers in bank order); the physical pad channels are fixed by the adapter, not configurable.

Each FLX4 button has `channel`, `note`, optional `side` and a nested `behavior`, for example:

```json
{"channel":0,"note":12,"behavior":{"type":"action","action":{"address":"/Syndicate/Deck/{deck}/Cue","press":1,"release":0}}}
```

Behavior types are `action` (an `action` object), `toggle`/`browse` (an `address`), `bank` (`bank:0..7`), `shift` and `select_deck` (no extra fields). `side` is required for channel-6 deck-targeting controls. These fields are not flattened into the button's MIDI identity.

`pads` is a required inline object containing `banks`, with no separate version field or filename reference. There are eight banks of eight actions/nulls; an action has `address`, integer `press`, optional integer `release`; optional `shifted_pads` and `for_player` (track/arrangement/clip/sampler) replace layouts. The normal Hot Cue bank's bundled `shifted_pads` send `Deck/{deck}/SetCueLoop` with integer cue 1–8, allowed only on tracks. Arrangement shifted pads remain explicitly unassigned; the router blocks cue loops on other player types. Shift+HOT CUE mode selection still selects the pitch/formant bank. The two bundled sampler step-bank overrides explicitly contain eight null pads and eight null shifted pads each: AP 0.1.901 removed per-step OSC controls, so step editing is unavailable. Every SlipLoop action must explicitly specify `slip_timing` as `straight`, `dotted`, `triplet` or `selected`, together with `press:1` and `release:0`. Missing timing is rejected, not filled in. The bundled slip loops use `selected` to follow the deck's D/T selection.

## OSC settings

Both devices share one OSC client (defaults: input `127.0.0.1:9090`, feedback `127.0.0.1:9096`), but retain independent holds, pickup and MIDI output. Optional `settings.json` accepts `osc`, `feedback` and `osc_root` (default `/Syndicate/`; custom example `{"osc_root":"/Stage/Left/"}`). Release builds read it beside the executable; debug builds using repository profiles read it from the repository directory. Root syntax: `/segment[/segment...]/`, ASCII letters/digits/`-`/`_`/`.` only, at most 48 bytes; wire addresses <128 bytes. Profiles always use `/Syndicate/`; outbound addresses are translated to the configured root, and feedback is accepted only under that root, case-insensitively. Update old `/AutoPilot/` custom profiles explicitly; no automatic migration. Match Syndicate's OSC API root.

Subscribe to feedback for ordinary pickup, toggles and hardware position. Since 0.1.902 feedback is change-only: when another physical control tries to take over an unanswered pending continuous write ≥250 ms old, the bridge may request `Feedback/Resend` at most once per second globally. Uncontested/output-only controls do not poll; timeout and no-op output do not imply confirmation. **FX dry-home controls deliberately use no macro feedback.** See [MAPPING.md](MAPPING.md) for per-device subscriptions and behavior.

FLX4 FX CH SELECT targets the current left deck, right deck or both selected decks (not all four). It waits for both hardware selector reports at startup; selector/deck changes send no OSC and re-arm dry-home. LEVEL/DEPTH sends normalized 14-bit `Mixer/CHn/FX/Macro/{wet_macro}` after first-report baseline and physical exact 0/dry; FX ON/OFF sends explicit OSC Float `Mixer/CHn/FX/Macro/{enable_macro}` 1.0 (either Shift+button sends 0.0). No macro feedback, chain `/FX` toggle, startup/shutdown macro writes or reliable FX LED. Configure enabled insert chains and plugin wet/enable macros consistently; see [FLX4 FX operation](MAPPING.md#flx4-fx-macros).

## Protocol references

- [Official DDJ-FLX4 MIDI message list](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_MIDI_message_List_E1.pdf), pp. 1–3 and p. 5 footnotes 3–4: FLX4 MIDI controls, p. 2 EFFECT assignments, SYNC release reports and FX ON/OFF LED caveat. [Mixxx DDJ-FLX4 implementation](https://github.com/mixxxdj/mixxx/blob/main/res/controllers/Pioneer-DDJ-FLX4-script.js), `sendKeepAlive`, `padModeKeyPressed`, `vuMeterUpdate`: hardware-protocol reference, not an AP mapping.
- [Novation Launch Control 3 programmer's DAW-mode guide](https://userguides.novationmusic.com/hc/en-gb/articles/33742551451666-Launch-Control-3-programmer-s-DAW-mode), “DAW mode control”, “The surface in DAW mode”, “Mode report and select”, “Colouring the surface”, “Absolute and relative encoder modes”: DAW ports, messages, encoders and LEDs. [Novation user guide v1.1](https://fael-downloads-prod.focusrite.com/customer/prod/downloads/launch_control_3_user_guide_v1.1_pdf_en.pdf), pp. 8, 11–12, 14–16, 29: operating modes and hardware layout.
- [Bitwig compact-LC3 implementation](https://github.com/bitwig/bitwig-extensions/blob/main/src/main/java/com/bitwig/extensions/controllers/novation/launchcontrolxlmk3/layer/LcMixerLayer.java), `bindNavigation`: Function uses `MUTE_SELECT_MODE`, Track arrows navigate the selected track. [CC constants](https://github.com/bitwig/bitwig-extensions/blob/main/src/main/java/com/bitwig/extensions/controllers/novation/launchcontrolxlmk3/CcConstValues.java): Function 66, Track Left 103 / Right 102; buttons use channel 1. Novation's DAW-mode guide “Controlling the screen” specifies text/display messages used by the profile.
- [AP guide §§14.2–14.3, 14.6, 14.9–14.10; recent changes 0.1.901–0.1.903](https://autopilot.audio/user-guide/index.html?c=c14): fixed OSC input/feedback, root and wire rules, documented `Filter1`/`Filter2` input routes, sampler-step removal and change-only feedback. §4.6 describes Browser CONTROL MODE. Hardware behaviour remains unverified.
