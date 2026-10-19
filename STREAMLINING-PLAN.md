# Repository streamlining plan

> **Historical implementation record, not current setup or API guidance.** The phases and evidence below describe earlier revisions (including older AUTO/PILOT route names, guide-rendering state and a superseded proposal to absorb the standalone porting guide). For current Syndicate roots, mappings, schema and printable guides, use [README.md](README.md), [MAPPING.md](MAPPING.md), [PROFILES.md](PROFILES.md), [CONTROLLER-PORTING.md](CONTROLLER-PORTING.md) and the HTML/PDF guides. Historical claims below have not been retroactively rewritten.

## Objective

Reduce duplicated rules, state and processing while preserving controller assignments, independent device lifecycles and shared AUTO/PILOT ownership. Prefer straightforward deletion and small local helpers over frameworks. Success is fewer independent code paths and clearer ownership, not an arbitrary LOC target.

Baseline: commit `8750ac9`, 3,906 Rust lines including comments and whitespace. One executable, seven direct dependencies, two adapters, editable JSON profiles.

| Area | Lines | Responsibility |
|---|---:|---|
| FLX4 adapter/config/pads | 1,038 | Hardware decoding and editable assignments |
| Declarative adapter | 776 | LC3 and generic Note/CC controllers |
| Profile schema | 642 | JSON types and validation |
| Runtime | 573 | Discovery, connections, hotplug and dispatch |
| Shared AP | 568 | OSC, feedback, ownership, holds and slip timing |
| Entry point, MIDI utilities, adapter interface | 309 | Startup and plumbing |

The current adapter boundaries are useful. Another directory reshuffle would not address the remaining duplication.

## Constraints

- Keep one application and the existing dependencies; no new crates, plugins, async infrastructure or controller framework.
- Keep assignments editable and preserve the bundled FLX4 and LC3 controls, including four-deck LC3 hot cues, independent hardware modes/software layers, and held-release destinations across selection changes.
- No backward-compatibility readers, aliases, migration framework or fallback formats. When changing the JSON shape, update bundled profiles and documentation together.
- Preserve strict unknown-field validation, safe port selection, ambiguous/conflicting-port rejection, bounded queues/OSC processing, independent disconnect cleanup and dependency-license packaging.
- Do not remove generic capabilities just because the bundled profiles do not use them; that would be a separate feature-scope decision.
- Preserve both controllers' HTML and PDF guides. Fix stale instructions rather than removing the guides or adding more caveats about them.
- Preserve `CONTROLLER-PORTING.md` as a standalone, detailed adaptation walkthrough. The user requested its restoration after phase 1 compressed it into `DEVELOPMENT.md`; link to it rather than folding it away again.
- Do not touch AP settings, the installed application, running devices, firmware or saved user mappings. Do not commit or push without a separate request.
- No subagents. No tests or test-like probes unless explicitly requested. Compilation and formatting are allowed; compilation is not hardware/AP verification.

## Phase 1 — Low-risk cleanup and documentation

Status: complete (source inspection, native compile check and PDF rendering; no hardware verification).

### Source cleanup

- [x] Remove unused `Serialize` derives; profiles are read, not exported.
- [x] Remove FLX4's `valid_route()` wrapper around `valid_address()`.
- [x] Share the declarative adapter's repeated hardware-mode-confirmed predicate.
- [x] Consolidate clearing of affected control state during deck/layer changes, without merging their distinct output-cache/echo rules or clearing held owners.
- [x] Let runtime request `/AutoPilot/Feedback/Resend` once after attachments; remove duplicate adapter startup requests. Retain requests for selection changes and toggle recovery.
- [x] Move fixed FLX4 pad channels `[7,8,9,10]` out of editable configuration. Keep actual assignments and bank mappings editable.

### Documentation consolidation

Seven Markdown documents plus both HTML/PDF pairs currently repeat setup, controls and limitations. Give each reference one purpose:

- [x] `README.md`: prominent beta/experimental-use warning, setup and quick orientation.
- [x] `MAPPING.md`: complete controller operation; fold in useful `USER-GUIDE.md` operation/troubleshooting content without repeating control tables.
- [x] `PROFILES.md`: current JSON reference.
- [x] `DEVELOPMENT.md`: architecture, packaging and controller-porting guidance; absorb `CONTROLLER-PORTING.md`.
- [x] Fold the useful evidence/limitations from `STATUS.md` into existing documents; remove the redundant documents after updating references and packaging.
- [x] Remove stale window-title claims (`MAPPING.md`) and the removed `--direct-tempo` instruction (`CONTROL-GUIDE.html`). Update old guide branding.
- [x] Add one reproducible PDF-rendering command and regenerate affected printable guides. Rendering an output artifact is not a browser walkthrough or hardware check.

Moving files into `docs/` alone is not a simplification. This plan is a working implementation record, not another shipped user manual.

## Phase 2 — Simpler JSON and retained typed configuration

Status: complete (source/profile diff inspection, native compile check and guide rendering; no runtime or hardware verification).

### Delete handwritten deserialization through a simpler shape

- [x] Nest FLX4 button variant fields under `behavior`, matching declarative bindings.
- [x] Nest declarative feedback variant fields under `render`, e.g. `"render":{"kind":"led","on":5,"off":4}`.
- [x] Replace the two handwritten deserializers with ordinary strict Serde derives. Preserve rejection of unknown fields and invalid variant fields.
- [x] Update both profiles and the schema reference in the same change. Do not accept the previous flattened format.

### Load and validate once

At baseline, discovery validated the common profile, parsed/validated FLX4 configuration and discarded that typed result. Attachment validated and parsed again.

- [x] Retain validated, typed adapter configuration at discovery and pass it to attachment/reconnection.
- [x] Reduce reliance on protocol strings plus an untyped `serde_json::Value` where a small explicit adapter variant suffices.
- [x] Preserve preflight before opening MIDI ports and cross-device AP feedback-scale conflict detection.
- [x] Keep this a local configuration boundary, not a general plugin/compilation framework. Clarity matters more than promising a large LOC reduction.

## Phase 3 — Lifecycle state and batched feedback

Status: complete (source inspection and native compile check; no runtime/hardware verification or performance measurement).

### Per-device lifecycle record

- [x] Combine the parallel profile-entry, live-session and retry-timestamp collections into one per-device record.
- [x] Reduce index coordination and argument lists in `reconcile()` and `event_loop()`.
- [x] Replace ambient `Ap.registration_owner` state with an explicit owner argument to feedback registration.
- [x] Preserve resolving all claims before attachment, detaching stale sessions before replacements, retry timing and per-device failure isolation.

### Batched rendering

At baseline each accepted OSC address invoked an adapter callback that refreshed the entire controller surface; the declarative refresh also cloned its feedback list.

- [x] Accept an OSC packet, invalidate affected captures, then refresh each interested controller once for that batch.
- [x] Keep full refreshes on mode/deck/layer changes.
- [x] Avoid unnecessary whole-list cloning where straightforward field borrowing or a small collected output list suffices; do not introduce pervasive reference-counted configuration solely to avoid a clone.
- [x] Preserve interest filtering, feedback accounting, invalid-feedback rearming and LC3 output echo handling.

Output caching already suppresses many duplicate MIDI messages. The expected gain is less scanning/cloning/value lookup; do not claim measured performance improvements without evidence.

## Phase 4 — Shared pickup policy, isolated from structural changes

Status: pending; highest-risk phase.

The FLX4 `cc()` and declarative `absolute()` paths duplicate decisions about another writer, pending confirmation, missing targets, crossing/tolerance and capture latching.

- [ ] Extract only that shared pickup policy into one small implementation.
- [ ] Retain capture through ordinary delayed feedback. Do not reinstate comparisons that revoke capture merely because feedback differs from the latest physical position.
- [ ] Keep FLX4 MIDI decoding, reusable MSB behavior and central tempo detent separate.
- [ ] Keep declarative fresh-MSB handling separate.
- [ ] Keep LC3 encoder synchronization, echo suppression and device-specific rearming separate.
- [ ] Preserve initial-baseline suppression, normalized target values, known bridge takeover handling and the limitation that AP feedback does not identify mouse-only writers.

This phase must not be bundled into configuration/lifecycle restructuring. Review the preceding work first through source inspection and compilation, then make pickup changes as a distinct pass. If its behavior cannot be defended through the permitted evidence, stop and record the specific blocker rather than guessing or silently running tests.

## Things to leave intact

- Shared AP ownership, held-release arbitration and last-held slip-loop behavior.
- Independent disconnect/failure cleanup and best-effort shutdown.
- Strict profile validation and ambiguous-port rejection.
- Bounded MIDI queues and OSC processing.
- Separate hardware modes and software layers.
- Dependency-license packaging and both printable guides.
- Generic capabilities that are not used by the supplied profiles but remain intentional supported features.

## Iteration and verification

1. Work through the phases in order. Re-read affected call sites and preserve current user edits.
2. For each phase, inspect the diff and profile changes for accidental mapping/behavior changes.
3. Use `cargo fmt --all`, `cargo check --locked`, and `git diff --check` where relevant. Do not run tests, launch the bridge or probe hardware.
4. Record completed items and actual evidence here. Distinguish source inspection, compilation, artifact rendering and observed hardware behavior.
5. Do not assign a LOC quota. Removing hundreds of lines appears plausible, but halving the program would probably mean removing features or safeguards.
6. If blocked, record the attempted change, evidence, blocker and smallest input needed. Do not weaken a safety boundary merely to finish a checklist.

## Work log

- Plan created from the holistic repository review.
- Phase 1 completed. Removed unused serialization and redundant route/startup paths; factored mode confirmation and affected-control rearming. Fixed FLX4 pad channels now live in the adapter; the bundled profile drops only that constant field, with no assignment changes.
- Consolidated seven reference documents into four, updated packaging/references, corrected stale guide instructions and retained both HTML/PDF pairs. Added `scripts/render-guides.ps1`; Windows Chrome successfully rendered both PDFs using isolated temporary browser profiles. No visual QA or hardware execution was performed.
- Evidence: `cargo fmt --all`, `cargo check --locked` (native Linux) and `git diff --check` succeeded. Source/profile diff inspected; removed-document links and stale guide wording no longer occur in the maintained references. Rust count is now 3,890 lines, down 16; most phase-1 savings are reduced documentation duplication and fewer maintained rules, not large source deletions.
- Phase 2 completed. Profiles now separate root identity/port selectors from `adapter:{"type":"flx4"|"declarative",...}`. Removed the protocol string/raw JSON configuration blob, both handwritten deserializers and repeated constructor preflight. Discovery retains the typed variant; reconnect clones only its adapter configuration. Cross-device AP scale registration is unchanged.
- FLX4 button variants now live under `behavior`; declarative feedback variants under `render`. Fieldless tagged variants use empty struct variants so strict Serde derives reject extra fields instead of silently accepting them. Both bundled profiles and references were updated directly; no compatibility parser was added. Whitespace-insensitive profile diffs show assignments, numeric values, hooks and palette preserved.
- Phase-2 evidence: `cargo fmt --all`, native Linux `cargo check --locked` and `git diff --check` succeeded. Source inspection confirms profile parsing/preflight is only in discovery and no handwritten deserializers/raw configuration blobs remain. Updated the LC3 guide's configuration path and rendered both PDFs. Rust count is 3,768 lines: 122 fewer than phase 1, 138 fewer than the baseline.
- Phase 3 completed. Runtime now stores profile, live session, retry timestamp and status in each `Device`; shared failure/detach helpers replace repeated cleanup paths. Claim resolution still precedes all attachments, stale devices detach first, and inventory changes/reset timings retain the previous retry behavior. AP registration now takes its session owner explicitly, with no ambient registration state.
- Feedback dispatch now filters one packet's unique normalized addresses per live device and calls that adapter once. Adapters finish invalidation before rendering, including invalid reports that cleared AP cache values. Successful-batch counters still count addresses, not packets/writes; failed devices are cleaned up independently. Full selection refreshes and MIDI echo suppression remain intact. Both adapters render by borrowing configuration rather than cloning entire feedback/LED lists; declarative feedback invalidation no longer clones binding routes into an intermediate list.
- Phase-3 evidence: source diff inspected for registration ownership, attach/detach order, retry resets, counters and batch ordering. `cargo fmt --all`, native Linux `cargo check --locked` and `git diff --check` succeeded. Rust count is 3,737 lines: 31 fewer than phase 2, 169 fewer than baseline. No performance or hardware claims follow from compilation.
- No tests, runtime/hardware probes, subagents or deployment during phases 1–3. Pickup crossing math, tolerances, MIDI decoding and tempo detents were not changed in phase 3. Phases 1–3 were subsequently committed and pushed as `1133979` at the user's request; phase 4 remains pending.
- The user requested the lost controller-adaptation document. Restored standalone `CONTROLLER-PORTING.md` using the detailed original from `832c7a6`, updated for typed profiles, current adapters, batched feedback and CLI. Replaced the compressed development section with a link, restored README discoverability and package inclusion. This supersedes phase 1's decision to fold that guide into `DEVELOPMENT.md`; it is not a backward-compatibility code path.
