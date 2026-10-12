# Using the controllers

[Setup](README.md#get-started) · [FLX4 mapping](MAPPING.md) · [Controller profiles](PROFILES.md) · [Printable FLX4 guide](CONTROL-GUIDE.pdf) · [Printable Launch Control 3 guide](LAUNCH-CONTROL-3-GUIDE.pdf)

## Start, stop and reconnect

Keep the bridge console open. Run only one instance: both instances would compete for MIDI ports and OSC feedback port 9096. Turn off the controller's direct MIDI input in AUTO/PILOT (AP), but leave your saved AP mappings and audio setup alone. To return to direct MIDI control, stop the bridge and re-enable the device in AP.

The bridge discovers matching profiles at startup and watches for attached/detached MIDI ports while running. It releases a disconnected device's held actions on a best-effort basis and tries to reconnect it; failed attachments retry after about 10 seconds or a port change. Reconnection does **not** reload edited JSON: restart the bridge for profile changes. AP restarts or uncertain state may also warrant a bridge restart to refresh feedback. Ctrl+C attempts to release held controls and stop bridge-owned repeat timing. A crash, forced termination or UDP loss cannot guarantee that AP received releases.

## Why a knob or fader may wait

Pickup prevents a jump when the physical position differs from AP's reported position. Move the control to AP's displayed value, then continue past it. FLX4 mixer/tempo controls re-arm on deck selection; the shifted filter control re-arms when Shift changes. LC3 endless EQ/filter encoders also depend on AP position feedback. Missing feedback blocks pickup rather than assuming a target. Use AP's OSC Feedback **Toggle All**, including TempoSlider and the required mixer routes; see [MAPPING.md](MAPPING.md#feedback-profiles-and-safety).

After pickup, ordinary delayed AP feedback does not undo it. Another bridge control taking the same route, a destination/layer change, or invalid feedback can re-arm it. AP feedback cannot identify whether a mouse moved the control, so mouse-only edits do not automatically revoke a captured control. `--verbose` reports traffic and pickup counters; OSC sends are not proof of AP state changes. There is no `--direct-tempo` override in the current bridge.

## FLX4 pads and decks

Left Shift+LOAD selects Deck 1↔3; right Shift+LOAD selects Deck 2↔4. These selections are independent and do not stop the other decks. Normal LOAD loads into the selected deck. A mode button chooses its normal bank; Shift+mode chooses its alternate bank, then release Shift. Shift+pad invokes the bank's shifted action, **not** another deck. The console shows deck, content type, bank and repeat timing. Mode-button lights identify banks, not deck numbers.

| Bank | What its pads do |
|---|---|
| HOT CUE | Track/clip cues 1–8; on arrangements, markers 1–5, Grid Start and previous/next marker page. Shift+pads are unassigned. |
| PAD FX | Hold slip-loop lengths 1/32, 1/16, 1/8, 1/4 and 1/2 **bars** on pads 1–5; pad 6 is unassigned; pads 7/8 toggle dotted/triplet. |
| BEAT JUMP | ±1/2/4/8 beats; while holding Shift, ±16/32/64/128 beats. |
| SAMPLER | Select sampler lanes 1–8; these are not sample-hit triggers. |
| Shift+HOT CUE | Pitch steps/reset and formant controls. |
| Shift+PAD FX | Loop movement and IN/OUT (halve/double while looping). |
| Shift+BEAT JUMP | Deck tools; sampler steps 1–8 on sampler decks. |
| Shift+SAMPLER | Navigation/locks; sampler steps 9–16 on sampler decks. |

Dotted and triplet are exclusive; tap the active one again for straight timing. Each deck remembers its timing. While two repeat-length pads are held, the latest wins; releasing it resumes the earlier pad by retriggering, which may not preserve the earlier loop region. Mouse D/T edits are not tracked.

To edit a sampler pattern: select a lane with SAMPLER, select Shift+BEAT JUMP for steps 1–8 or Shift+SAMPLER for steps 9–16, **release Shift**, then tap pads to enable steps or hold Shift+pad to disable them. PLAY runs/pauses the pattern. Save in AP to retain edits. These pads do not play individual hits or display step state.

For all assignments and content-specific limitations, use [MAPPING.md](MAPPING.md). Pitch needs pitch preservation enabled; formant needs Formant Mode set to Manual. The bridge does not change these AP settings.

## Launch Control 3 (compact)

Connect through the **DAW USB ports**, not Main MIDI or the XL 3 ports. Select Mode → Function (DAW) on the device. Its reported DAW Mixer/Control mode determines whether encoders address CH1–4 EQ or Filter1/Filter2. Track ◀/▶ changes its Hot Cue target from Deck 1 to 4 independently of the FLX4; Function changes bottom buttons between Hot Cues and CH1–4 Mute/headphone Cue. The LC3 screen shows deck/layer. Cue RGB colours mark slots, **not** whether a cue exists. Mute/Cue toggles need confirmed AP feedback. See [LC3 mapping](MAPPING.md#launch-control-3-compact-daw-usb-ports) for all assignments.

## Troubleshooting

| Symptom | What to check |
|---|---|
| One action occurs twice | Disable that physical controller's direct MIDI input in AP. |
| No matching controller / port busy | Run `ap-midi-osc-bridge --list` and compare exact/unique names with `profiles/*.json`; close other apps owning its MIDI ports. Edit selectors, then restart the bridge. |
| Profile conflict or ambiguous selector | Use unique input/output names. The bridge will not guess which device/profile should own a port. |
| Knob/fader does nothing | Move it through AP's reported position; check OSC feedback subscriptions and watch console warnings. LC3 also needs a confirmed DAW mode report. |
| Tempo stays blocked | Enable TempoSlider feedback; both AP input and feedback must use the bipolar −1..+1 API, not the legacy 0..1 API. |
| FLX4 deck type is Unknown; deck buttons do nothing | Check AP's feedback destination IP/port (127.0.0.1:9096) and PlayerType subscription; use four-deck view for decks 3/4. |
| Browser turn does nothing | Set AP Browser Mode to CONTROL MODE and show the Browser. |
| OSC feedback port busy | Stop the other bridge instance using port 9096. |
| Sampler pads do not play sounds | They select lanes or edit steps; PLAY runs the sequencer. |

Jog/scratch, repeat scrubbing, crossfader and FX are not mapped on FLX4. Cue taps are not held previews; supported actions vary with player type. AP may ignore controls that are not visible. Network or hardware loss can leave AP state uncertain; stop/restart as needed. See [MAPPING.md](MAPPING.md#unsupported--unassigned) for the full boundary.
