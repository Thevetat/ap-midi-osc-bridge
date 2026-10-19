# Controller mappings

These defaults come from [`profiles/flx4.json`](profiles/flx4.json) and [`profiles/launch-control-3.json`](profiles/launch-control-3.json); see [PROFILES.md](PROFILES.md) for editing and [OSC-root rules](PROFILES.md#osc-settings). Syndicate 0.1.901 retired learned/profile OSC assignments and its dedicated FLX4 preset; this independent bridge uses the fixed API without changing Syndicate mappings.

`D` is the FLX4-selected deck; `CHD` its main mixer channel. Routes below have canonical `/Syndicate/` prefix (or configured outgoing root).

## Start, stop and reconnect

Follow [setup](README.md#get-started); run only one bridge (MIDI/feedback port 9096 conflict). To resume direct MIDI, stop it and re-enable those Syndicate MIDI inputs without changing saved mappings or audio. MIDI hotplug retries failed devices after about 10 seconds or port changes; reconnect does not reload JSON, so restart after profile edits or uncertain app state. Ctrl+C attempts releases; UDP loss/crashes/forced termination cannot guarantee delivery. Syndicate feedback is change-only; [takeover/resend and fail-closed rules](PROFILES.md#osc-settings) do not infer success from time or a no-op write.

## Deck selection

| Side | Startup deck | Gesture | Selection |
|---|---|---|---|
| Left | 1 | Hold left Shift, tap left LOAD, release Shift | 1 ↔ 3 |
| Right | 2 | Hold right Shift, tap right LOAD, release Shift | 2 ↔ 4 |

Selection is independent per side. It changes the bridge's target, not AP's playback state, content or tempo-master assignment. All four decks can continue playing. Use AP's four-deck view for decks 3/4; the bridge does not change that view.

Supported deck actions and pads follow the selection. Mixer controls address its corresponding **main channel**: selecting Deck 3 makes the left channel fader control `Mixer/CH3/Volume`, not an arrangement's internal track trim. LEDs/meters follow the selected deck/channel. Normal LOAD targets the selected deck; Shift+LOAD never loads.

The console is the deck-selection reference. Hardware mode lights identify pad banks, not deck numbers. Selection requests fresh feedback and re-arms mixer/tempo pickup; controls can wait until the hardware crosses the reported value. Per-deck pad banks and repeat timing are recalled when returning to a deck. Existing held actions retain their original release destination.

Once picked up, a physical control stays latched through ordinary AP feedback, which can lag behind movement. Another control in this bridge taking over the same route, a relevant deck/layer change, or invalid feedback re-arms pickup. AP feedback has no writer identity, so mouse-only changes do not automatically release the latch. Normal console output reports selection/bank changes; pickup counters require `--verbose`.

Restart selects decks 1/2. **Shift+mode changes a bank, not the selected deck.** Browser navigation and headphone MIX remain shared.

## Fixed controls

| Physical control | OSC route | Behaviour |
|---|---|---|
| Left / right Shift+LOAD | Bridge-only | Select 1↔3 / 2↔4; never load |
| LOAD | `Browser/LD` | Load selected item into selected deck |
| PLAY | `Deck/D/PlayPause` | Trigger |
| CUE | `Deck/D/Cue` | Press 1, release 0 |
| Shift+CUE | `Deck/D/GridStart` | Trigger |
| SYNC tap / hold | `Deck/D/TempoSync`, `TempoMaster` | Native short/long messages; Shift+SYNC unassigned |
| IN / OUT | `Deck/D/LoopIn`, `LoopOut` | Set points; halve/double while looping |
| 4BEAT/EXIT | `Deck/D/Loop` | Toggle/exit loop, including a cue-region loop |
| Tempo fader | `Deck/D/TempoSlider` | 14-bit MIDI → bipolar float −1..+1; 0 native tempo; pickup |
| Channel fader / trim | `Mixer/CHD/Volume`, `Trim` | 14-bit normalized, pickup |
| High / Mid / Low | `Mixer/CHD/EQ/High`, `Mid`, `Low` | 14-bit normalized, pickup |
| Filter / same-side Shift+filter | `Mixer/CHD/Filter1`, `Filter2` | 14-bit normalized, pickup; inputs documented in AP §14.9 (behavior depends on filter type; hardware unverified) |
| Headphone CUE | `Mixer/CHD/Cue` | Toggle from confirmed feedback |
| Headphone MIX | `Header/CueMix` | Shared/direct, not deck-switched |
| Browse rotation | `Browser/Up`, `Down` | Signed relative steps; counterclockwise up |
| Browse press / Shift+press | `Browser/Enter`, `Back` | Trigger |

Browse rotation uses channel 7 CC64 signed relative counts, not the mixer-knob decoder. AP documents hardware navigation under **Settings → General → Browser Mode → CONTROL MODE** (§4.6).

Each side tracks its own Shift. Filter1/Filter2 pickup resets on Shift press/release; the next full knob report establishes a baseline. Enable both feedback subscriptions. Mixer knobs address main mixer channels, not arrangement-internal track trims. AP may require the corresponding controls to be visible.

**Tempo contract:** requires AP's bipolar `TempoSlider` input and **both `TempoSlider` and `TempoSliderEnabled` feedback**: **−1 minimum, 0 native tempo, +1 maximum**. Output maps physical 0–1 to `2 * position - 1`, with the two central MIDI codes sending exact 0. Feedback maps back to `(value + 1) / 2` for pickup; mixer values remain 0–1. Non-finite/out-of-range tempo feedback is rejected and re-arms pickup. Unknown or false enabled state blocks continuous tempo writes even with pickup disabled; disable/enable transitions re-arm pickup, but repeated unchanged availability snapshots do not. Invalid availability reports also notify adapters. No legacy 0–1 tempo API support.

### FLX4 FX macros

The physical **FX CH SELECT** positions 1 / 2 / 1&2 target the currently selected left deck (1 or 3), right deck (2 or 4), or **both selected decks**—not all four. Startup selection is unknown until matching left and right hardware selector reports have been received; changing selector/deck target sends no OSC and re-arms LEVEL/DEPTH. FX SELECT pushbutton and BEAT arrows are unassigned.

Configure an enabled Syndicate VST3 insert chain on every channel you intend to control. Assign **Macro 1** to the same plugin wet/dry parameter (0 dry, 1 wet) and **Macro 2** to a mappable plugin enable (0 off, 1 on) across all four channels; the plugin must expose it, and a bypass mapping may need inversion in the app. The chain itself must remain enabled: this bridge never writes `Mixer/CHn/FX` and does not configure plugins. The bundled profile maps **LEVEL/DEPTH** (14-bit, 0–1) to `Mixer/CHn/FX/Macro/1`, and **FX ON/OFF** to explicit `Mixer/CHn/FX/Macro/2` **1 on**; either Shift+FX ON/OFF sends **0 off**. These are not toggles; both selected channels receive identical values, and enabling does not change the wet amount. Other macros (3–8) are unassigned.

No macro feedback is documented, so the wet control cannot use feedback pickup: after activation or retargeting, the first complete knob report only establishes a baseline. Then move the knob physically to **exact 0/dry** to send 0 and arm it (if already dry, nudge away and back); subsequent movement sends wet values. Another known bridge writer of the same route re-arms dry-home; mouse/external changes cannot be identified without feedback. No FX OSC writes occur at startup/shutdown, and sends are not confirmation that the plugin acted. The FX LED is **not driven** and does not indicate on/off state. See [Syndicate guide §14.9, Mixer FX/Macro](https://autopilot.audio/user-guide/index.html?c=c14) and [AlphaTheta FLX4 MIDI list p.2 EFFECT, p.5 footnote 4](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_MIDI_message_List_E1.pdf).

## Pad banks

Pads 1–4 are the top row; 5–8 the bottom. Press a mode button for its **normal bank**, or Shift+mode for its **alternate bank**, then release Shift. Each logical deck remembers its bank. Shift+pad uses that bank's shifted layer, described below.

### Track / arrangement layout

| Selector | Bank | Top row: pads 1–4 | Bottom row: pads 5–8 |
|---|---|---|---|
| HOT CUE | Hot cues | Cue 1, 2, 3, 4 | Cue 5, 6, 7, 8 |
| PAD FX | Slip loops / timing | Hold 1/32, 1/16, 1/8, 1/4 bars | Hold 1/2 bar; unassigned; toggle Dotted; toggle Triplet |
| BEAT JUMP | Beat jumps | −1, +1, −2, +2 beats | −4, +4, −8, +8 beats |
| SAMPLER | Sampler lanes | Inactive unless sampler loaded | Inactive unless sampler loaded |
| Shift+HOT CUE | Pitch / formant | Pitch −12, −1, +1, +12 | Pitch reset, formant −, reset, + |
| Shift+PAD FX | Loop tools | Move −1/+1 beat, −1/+1 bar | Move −4/+4 bars, IN/halve, OUT/double |
| Shift+BEAT JUMP | Deck tools | Hold half speed, double speed, reverse; Grid Start | Hold phase −, +; phase reset; normal speed |
| Shift+SAMPLER | Navigation / locks | Zoom out/in; marker page previous/next | Slip ON/OFF; pitch preservation ON/OFF |

- **Arrangement HOT CUE override:** pads 1–5 are the five visible markers; pad 6 Grid Start; pads 7/8 previous/next marker page. This replaces the eight-cue layout only while `PlayerType=arrangement`.
- **Normal HOT CUE bank, hold Shift+pads 1–8:** on **track decks only**, launch cue-region loops 1–8 using `Deck/D/SetCueLoop` with integer cue 1–8. Normal cue taps are unchanged; Shift+HOT CUE **mode button** still selects the pitch/formant bank. Arrangement shifted marker pads remain unassigned; clip/sampler/empty/unknown types cannot launch these loops. The launch is quantized, from that cue to the next cue in time or track end; an empty cue does nothing. It is **not** a fixed beat-length loop. Exit with 4BEAT/EXIT (`Deck/D/Loop`). Silent cue positioning is not mapped.
- **Beat Jump held-Shift layer:** top −16,+16,−32,+32; bottom −64,+64,−128,+128 beats. Release Shift to return. No remembered range cycling.
- Other shifted pads reuse their bank's normal actions, except the explicitly unavailable sampler banks below.
- Pitch needs P (pitch preservation) enabled; formant needs **Formant Mode → Manual**. The bridge does not change AP settings or automatically enable P. Clip pitch transposes MIDI notes; formant pads are blocked on clips.
- IN/OUT halve/double only while looping; otherwise they set loop endpoints. Loop-move amounts are signed whole beats/bars. These pads move the loop; they do not set an absolute loop length.
- Marker paging is arrangement-only. Zoom commands only work where AP exposes the waveform controls.
- Speed/reverse pads set on while held and off on release; they do not restore an unreported pre-existing speed/reverse state. Avoid simultaneous conflicting speed pads or mouse edits during a hold. Normal speed explicitly sets `Speed=1`.

### Slip-loop timing and ownership

PAD FX pads 1–5 use the deck's **selected timing**. Pad 7 toggles Dotted; pad 8 toggles Triplet. Selecting either clears the other; pressing the active one again selects straight timing. Pad 6 is unassigned, including Shift+pad 6. Holding Shift does not change this bank's pad actions.

Timing starts straight and is remembered independently per logical deck until changed or the bridge restarts. It stays selected after releasing all repeat pads and across bank/deck selection. The bridge sends explicit `SlipDotted` / `SlipTriplet` switch values. It clears both before setting either and reasserts its timing when starting a repeat. Console output shows the selected repeat timing.

The **latest held length wins per logical deck**: release the previous loop before starting a new one. Releasing the latest restarts the previous still-held loop, now using the current timing. Releasing an older pad does not interrupt the active one. Changing D/T while holding a PAD FX length stops/retriggers that loop with the new timing. Releasing the final length stops repeating but **does not reset D/T**.

Custom profiles can assign fixed-timing slip loops. A fixed-timing hold temporarily overrides D/T, restoring the selected timing when released (or the previous held loop's timing when resuming it). During that hold, D/T toggles update the selection for subsequent normal repeats without changing the fixed-timing loop.

The bridge owns this state; AP provides no documented D/T feedback, so mouse/other-controller edits are not tracked. A resumed loop is retriggered, not guaranteed to preserve its earlier region. Graceful exit attempts loop releases and clears D/T for every deck whose timing it owns, including latched modes with no pad held. UDP loss, content changes and force-kills cannot guarantee delivery.

### Sampler layout

| Selector | Top row | Bottom row |
|---|---|---|
| SAMPLER | Select lanes 1–4 | Select lanes 5–8 |
| Shift+BEAT JUMP | Unavailable (pads 1–4 unassigned) | Unavailable (pads 5–8 unassigned) |
| Shift+SAMPLER | Unavailable (pads 1–4 unassigned) | Unavailable (pads 5–8 unassigned) |

AP 0.1.901 removed per-step sampler OSC/mapping controls. Both bundled sampler overrides have eight null pads **and** eight null shifted pads: selecting either former step bank cannot edit steps; no replacement route is assumed. SAMPLER pads still select lanes, **not sample-hit triggers**. No velocity editing, lane mute, direct hits or step LEDs are assigned.

## Content-aware routing

No deck has a prescribed role. `PlayerType` determines both compatible OSC routes and optional pad-layout overrides.

| Player type | Coverage |
|---|---|
| Track | Generic deck routes; cue taps and Shift+pad cue-region loops in the normal HOT CUE bank, slips, jumps, pitch/formant, loops, speed/phase, zoom/locks |
| Arrangement | Track-style routes plus five-marker/page Hot Cue layout; no arrangement-internal mixing |
| Clip | PLAY, CUE, Grid Start, eight cue taps, pitch and phase via `Deck/D/Clip/…`; unsupported pads blocked |
| Sampler | PLAY and lane selection via `Deck/D/Sampler/…`; former two step banks are explicitly unassigned. Custom profiles can also use translated Speed/Quantize/FaderStart/PhaseBend/PhaseReset |
| Empty / unknown | Deck actions blocked; mixer/browser remain available |

Clip `Loop` means restart-at-end, not a track loop; the bridge does not map it to the loop button. Clip/sampler SYNC/master/tempo are unsupported. The fixed `HotCue` held-preview command is not mapped on either controller.

## Unsupported / unassigned

- `CueMomentary` can make track hot cues play only while held, but bundled `Cue1`–`Cue8` are quick taps; fixed multi-argument `HotCue` held preview is unmapped. No documented fixed cue-delete route or silent cue-positioning action.
- Direct sampler hits and arrangement-deck internal trims/routing have no established input routes here. AP §14.8 documents lane mute through multi-argument `Sampler/LaneSet`, which this bridge does not yet support. Internal feedback addresses alone are not evidence of input support.
- Jog/scratch, repeat scrubbing, crossfader, Shift+browse rotation, loop-call arrows, FX SELECT pushbutton/BEAT arrows, Smart CFX/Fader and other shifted transport buttons are unassigned. FLX4 FX CH SELECT, LEVEL/DEPTH and FX ON/OFF instead control mixer insert macros (above).
- No EQ/filter-panel toggle or mouse automation.
- No legacy 0–1 tempo API support, hardware deck-number indicator or pad-occupancy/step LEDs; sampler step editing was removed by AP 0.1.901.

## Feedback, profiles and safety

In Syndicate OSC Feedback, use **Toggle All** (unused values are ignored). If selective, subscribe for decks 1–4 to `PlayerType`, `Playing`, `Sync`, `Loop`, `TempoSlider`, `TempoSliderEnabled`; for mixer CH1–4: `Volume`, `Trim`, `EQ/High`, `EQ/Mid`, `EQ/Low`, `Filter1`, `Filter2`, `Cue`, `Meter`, plus `Mute` for LC3. Play/Sync/Loop/Cue LEDs and meters follow selected decks. Deck changes request a snapshot before pickup/type-dependent actions resume; console status lists both sides' deck, player type, effective bank (including marker/unavailable sampler overrides) and repeat timing.

Held actions keep their original resolved address across bank/deck selection. Content changes, app restarts, network loss and force-kills remain boundaries: stop/restart the bridge when state is uncertain. Pad customization and `slip_timing` are documented in [PROFILES.md](PROFILES.md#flx4-pads).

## Launch Control 3 (compact, DAW USB ports)

The compact `profiles/launch-control-3.json` uses the **DAW USB ports** (not Main MIDI, XL 3 or a Components Custom Mode). Select **Mode → Function (DAW)** to switch DAW Mixer/Control; startup requests DAW Mixer, but no controls activate until the hardware reports a mapped mode.

| Control | DAW Mixer (Mode 1) | DAW Control (Mode 2) |
|---|---|---|
| Top encoders 1–6 (Hot Cues/Mixer layers) | CH1 Low/Mid/High; CH2 Low/Mid/High | CH1 Filter1/Filter2; CH2 Filter1/Filter2; 5–6 unassigned |
| Bottom encoders 1–6 (Hot Cues/Mixer layers) | CH3 Low/Mid/High; CH4 Low/Mid/High | CH3 Filter1/Filter2; CH4 Filter1/Filter2; 5–6 unassigned |
| Top encoder 8 | Browser Down clockwise / Up counterclockwise | Same, including FX layer |
| Function | Cycle Hot Cues → Mixer → FX → Hot Cues | Same |
| Track ◀ / ▶ | Select Hot Cue target Deck 1–4, stopping at ends | Same; FX channels stay fixed |
| Buttons 1–8, Hot Cues layer | Cue taps 1–8 on selected deck | Same |
| Buttons 1–4 / 5–8, Mixer layer | Mute CH1–4 / headphone Cue CH1–4 | Same |
| Top encoders 1–4, FX layer | Fixed CH1–4 Macro 1 wet, 7-bit dry-home | Same; replaces EQ/filter encoders |
| Buttons 1–4 / 5–8, FX layer | Fixed CH1–4 Macro 2 explicit ON=1 / OFF=0 | Same; not toggles |
| Other FX-layer encoders | Unassigned (except Browser) | Same |

Startup/reconnect selects **Deck 1 / Hot Cues**; the screen shows deck/layer (in FX: CH1–4 and ON/OFF roles). Function is purple in Hot Cues and FX, cyan in Mixer; use the screen to distinguish layers. Track navigation changes only the LC3 cue target, independently of FLX4: EQ/filter/FX and Mixer buttons always address **main mixer CH1–4**, not arrangement-internal trims. Hardware EQ/filter mode and software layer are independent. EQ/filter controls work in Hot Cues and Mixer layers; switching between them preserves unchanged encoder captures. Entering/leaving FX or a confirmed hardware-mode change re-arms relevant controls.

Hot Cue buttons tap `Deck/{deck}/Cue1`–`Cue8`, not FLX4's shifted cue-region loops or held previews; existing CueMomentary/Slip settings can affect taps. Track cues can set an empty slot; arrangement decks interpret these routes as marker buttons, not clip/sampler cues. Fixed **Candy Pop** slot colours (not occupancy): `#FF5D73`, `#FF9F43`, `#FFE66D`, `#72EFB5`, `#48CAE4`, `#5E8CFF`, `#A875FF`, `#FF66C4`. The RGB 0–127 `hot_cues` `on_enter` messages do not follow later palette/per-cue changes; there is no populated-slot guard.

Mixer-layer buttons use confirmed `Mute`/`Cue` feedback for toggles and red/cyan LEDs; unknown state remains grey and blocks toggling. Position feedback synchronizes endless EQ/filter encoders; missing feedback blocks pickup. Browser top encoder 8 recenters over DAW output in every layer, and the first non-echo report after a mode switch may be consumed as baseline. Select Browser CONTROL MODE; subscribe to CH1–4 EQ/Filter1/Filter2/Mute/Cue (or Toggle All). Syndicate §14.9 documents filter inputs; end-to-end hardware operation remains unverified.

**FX layer:** Map each CH1–4 enabled VST3 insert chain's Macro 1 to plugin wet/dry (0 dry, 1 wet), Macro 2 to a mappable plugin enable (0 off, 1 on; invert bypass if necessary). Top encoders 1–4 send normalized **7-bit** `Mixer/CHn/FX/Macro/1`; the bottom row and top 5–7 are unassigned. Macro feedback is undocumented: the first FX-encoder report after activation/retarget is baseline only, then you must physically reach exact **0/dry** to send 0 and arm it (nudge away/back if already dry). Known bridge writes by another control, including FLX4, re-arm dry-home; mouse/external writes cannot be detected. Buttons 1–4 send `Mixer/CHn/FX/Macro/2` OSC **Float 1.0 ON**, 5–8 send **Float 0.0 OFF** (latched press, no release write). Static green/red button colours label actions, **not** state; the screen labels FX CH1–4 and ON/OFF. No macro feedback/pickup, no chain `/FX` toggle, no wet change on enable and no startup/shutdown macro writes. OSC sends do not confirm plugin state. See [Syndicate §14.9, Mixer FX/Macro](https://autopilot.audio/user-guide/index.html?c=c14).

## Troubleshooting

| Symptom | What to check |
|---|---|
| One action occurs twice | Disable that controller's direct MIDI input in AP. |
| No matching controller / port busy | Use `ap-midi-osc-bridge --list`; compare names with `profiles/*.json` and close other apps owning the ports. Restart after editing selectors. |
| Profile conflict or ambiguous selector | Use unique input/output names; the bridge will not guess port ownership. |
| EQ/fader does nothing | Move through the reported position; enable feedback subscriptions. LC3 also needs a confirmed DAW mode report. |
| Tempo stays blocked | Enable TempoSlider and TempoSliderEnabled feedback; enabled must be 1, and AP input/feedback must use the bipolar −1..+1 API. |
| FLX4 player type is Unknown; deck buttons do nothing | Check AP's feedback destination (default 127.0.0.1:9096) and PlayerType subscription; use four-deck view for decks 3/4. |
| Browser does nothing | Set AP Browser Mode to CONTROL MODE and show the Browser. |
| OSC feedback port busy | Stop the other instance using port 9096. |
| Sampler pads do not play sounds | Pads select lanes; former step-edit banks are unassigned since 0.1.901. PLAY runs the sequencer. |
| FX macro knob/buttons do nothing | Confirm the chain is enabled, macros 1/2 are assigned to wet/enable, and FLX4 FX selector has reported or LC3 is in the FX layer with confirmed DAW mode. Bring wet knobs physically to exact 0/dry after baseline; LEDs/OSC sends do not confirm plugin action. |

`--verbose` shows traffic/pickup counters, not confirmation Syndicate acted. For suspected upstream bugs, follow the [README warning](README.md).

## Protocol references

- [AlphaTheta MIDI list pp.2,5](https://downloads.support.alphatheta.com/software_info/dj-controllers/DDJ-FLX4/DDJ-FLX4_MIDI_message_List_E1.pdf): Shift+LOAD notes 104/122; native SYNC reports on release; p.2 EFFECT defines the FX controls, and p.5 footnote 4 describes FX ON/OFF LED blink/steady behavior.
- [AP guide §§14.2–14.3, 14.5–14.10 and recent changes 0.1.901–0.1.903](https://autopilot.audio/user-guide/index.html?c=c14): fixed routes (§14.6 `SetCueLoop`, §14.9 mixer `FX/Macro/{1-8}`), root, filter inputs, TempoSliderEnabled, change-only feedback and sampler-step removal. §§6.6/6.9 describe loop and marker behaviour. The 0.1.903 changelog establishes `/Syndicate/` for this 0.1.903+ bridge contract.

For editing profiles, see [PROFILES.md](PROFILES.md).
