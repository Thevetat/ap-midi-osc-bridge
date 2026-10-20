# BCR32 → Syndicate: source-reviewed mapping draft (not a working profile)

Status: **source-reviewed starter, not hardware-verified**. No BCR32 is available here; its factory MIDI assignments, OS port name, preset state and host-to-device LED protocol have not been established. The draft profile is at [`drafts/bcr32.json`](drafts/bcr32.json), outside the auto-loaded `profiles/` directory. Do not copy an existing controller's SysEx/LED output to the BCR32.

## Device facts and setup boundary

Behringer's [BCR32 User Manual, pp. 16–17, 22–29](https://cdn-media.empowertribe.com/611d8696b95f80f8c46dbecd816d0b65/Manual_BE_0715-AAW_BCR32_A4_WW_2026-04-06_Rev1.pdf) describes **CTRL** and **ZAQ** modes, 32 rotary encoders that can also be pressed, assignable CTRL-mode buttons, and a user-programmable MIDI mapping. In CTRL mode hold **EDIT** while operating the chosen control to program it; **LEARN** can assign from incoming MIDI. Use **STORE** and the manual's permanent-save step if the preset must survive power-off (p. 22). The manual does not establish a universal CC/note number for each physical control in an arbitrary stored preset. Do not assume that an existing preset matches the numbers proposed below.

The [manual's Global Settings and Controller Settings, p. 17](https://cdn-media.empowertribe.com/611d8696b95f80f8c46dbecd816d0b65/Manual_BE_0715-AAW_BCR32_A4_WW_2026-04-06_Rev1.pdf) distinguish USB, standalone DIN and mixed operation, plus CTRL output routing. A USB-based bridge profile needs USB MIDI enabled and the controller's messages routed to USB. Use the USB **power/data** port (p. 25), not the power-only port. Check the actual input name with `ap-midi-osc-bridge --list` before choosing a profile selector. Do not change global settings or saved presets without the owner's consent.

The [manual's Mode & Options, pp. 24–25](https://cdn-media.empowertribe.com/611d8696b95f80f8c46dbecd816d0b65/Manual_BE_0715-AAW_BCR32_A4_WW_2026-04-06_Rev1.pdf) lists CC encoder **Absolute** (7-bit), **ABS_14**, multiple **REL** forms, and button **Tog On/Tog Off/Inc**. Start with *Absolute 7-bit CC*, MIDI channel 1, full 0–127 range for continuous controls, and *Tog Off* (momentary, 127 on press/0 on release) for buttons. This is a **preset to program**, not a statement of factory defaults. Other modes need byte-level confirmation before mapping them to this bridge's decoder. The manual's CC setup tables (p. 28) are not a reliable source of a fixed factory assignment, and the extracted PDF table columns need checking against the original if using advanced encodings.

## Proposed fixed four-channel mixer preset

A **design choice**, not a BCR32 hardware fact: assign one physical row of eight encoders per main mixer channel (the manual numbers encoders 25–32, 33–40, 41–48, 49–56 respectively). Read the four rows as CH1–CH4. Program unique MIDI-channel-1 CCs 16–47, one per encoder in row order; these CC numbers are *ours*, not Behringer defaults. The bridge would use `absolute` with confirmed feedback/pickup for rows 1–7. Assign the eighth to FX Macro 1 with `absolute_dry_home` rather than ordinary absolute: the first report is a baseline, and the user must turn physically to exact 0/dry before the control is armed. No Syndicate macro feedback/pickup is documented.

| Encoder in each channel group | CH1 / CH2 / CH3 / CH4 example CC | Syndicate route for CHn | Notes |
|---|---|---|---|
| 1 | 16 / 24 / 32 / 40 | `/Syndicate/Mixer/CHn/Volume` | Confirmed feedback required for pickup |
| 2 | 17 / 25 / 33 / 41 | `/Syndicate/Mixer/CHn/Trim` | Same |
| 3 | 18 / 26 / 34 / 42 | `/Syndicate/Mixer/CHn/EQ/High` | Same |
| 4 | 19 / 27 / 35 / 43 | `/Syndicate/Mixer/CHn/EQ/Mid` | Same |
| 5 | 20 / 28 / 36 / 44 | `/Syndicate/Mixer/CHn/EQ/Low` | Same |
| 6 | 21 / 29 / 37 / 45 | `/Syndicate/Mixer/CHn/Filter1` | Same |
| 7 | 22 / 30 / 38 / 46 | `/Syndicate/Mixer/CHn/Filter2` | Same |
| 8 | 23 / 31 / 39 / 47 | `/Syndicate/Mixer/CHn/FX/Macro/1` | Plugin wet/dry, manual dry-home |

These Syndicate routes and the Macro 1 wet/dry setup follow this project's `MAPPING.md` (FX setup and LC3 FX layer) and `PROFILES.md` (declarative JSON v2); the macro only does wet/dry after the user maps that macro to a plugin parameter in an **enabled** insert chain. Macro 2 is a separate plugin-enable assignment (0 off/1 on; bypass may require inversion). The bridge does not configure the plugin or toggle the chain.

**Proposed buttons:** use assignable CTRL-mode buttons 5–8 for CH1–4 Macro 2 **ON** (CC 48–51), and buttons 9–12 for CH1–4 **OFF** (CC 52–55). Program each as MIDI-channel-1 CC, Tog Off, min 0/max 127. `macro_button` sends OSC Float 1.0 or 0.0 once per press; no state/colour inference, no press-to-toggle. Other buttons/encoder presses remain unassigned initially; a later pass could use Mute/Cue only with confirmed Syndicate feedback. These button/CC choices are ours, not factory defaults. The CTRL-mode assignable button range is documented in the BCR32 manual, p. 22.

## What can be implemented with today's bridge

The draft [`drafts/bcr32.json`](drafts/bcr32.json) uses the existing **declarative** adapter: 32 channel-1 encoder CC bindings plus 8 button CC `macro_button` bindings, and `outputs:[]`. In Syndicate, subscribe to CH1–4 Volume, Trim, EQ High/Mid/Low and Filter1/Filter2 feedback (or use Toggle All) so ordinary absolute controls can capture via pickup. FX macro feedback is undocumented. No ring LEDs, MIDI output or device initialization is required in this first version. To use it, program and permanently save the specified CTRL preset on the BCR32, run `ap-midi-osc-bridge --list` to get its unique USB MIDI input name, replace `REPLACE_WITH_UNIQUE_BCR32_USB_INPUT` with a unique port selector, and then copy the JSON into `profiles/bcr32.json` beside the executable and restart the bridge. Profiles there are autoloaded; do not copy the draft unchanged. If physical encoders do not match the intended rows, swap the CC assignments in the preset or the JSON, not both.

**Open evidence:** actual OS/connection and USB port name, programmed CC values and press/release behavior on the user's firmware, a reproducible shareable preset export (the manual's CTRL SysEx export is on p. 22), and whether hardware feedback/ring LEDs are wanted. There is no proof yet that BCR32 host output echoes or LED handling matches the declarative feedback renderer; keep output disabled until its exact protocol is sourced. No live captures, device changes or tests were performed.
