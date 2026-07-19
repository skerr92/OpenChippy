# CondensedContext

## Context Freshness

- Context ID: FRESH-001
- Last Verified Commit: `4d97d30`
- Current HEAD: `4d97d30`
- Generated: 2026-07-19
- Status: partial; Milestones 3.1–3.5, Milestone 3.6 stage 1, and nested Milestone 3.7 hierarchy are implemented but uncommitted.
- Verify: `README.md`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/src/technology.rs`, `src-tauri/src/model.rs`, `src-tauri/src/history.rs`, `src-tauri/src/simulation.rs`, `src-tauri/src/physical_layout.rs`, `src-tauri/src/validation.rs`, `src-tauri/src/lib.rs`, `src/types.ts`, `src/backend.ts`, `src/App.tsx`, `src/SchematicViewport.tsx`, `src/PhysicalViewport.tsx`, `src/Viewport.tsx`, `src/WaveformView.tsx`, `src/styles.css`, `docs/roadmap.md`, `CondensedContext.md`, `CondensedContext.CCF1`
- Full Semantic Memory: `CondensedContext.CCF1`

## Current Focus

- Context ID: ACTIVE-001
- Confidence: High; verified against commit `4d97d30`.
- Milestone 3.6 now renders a compact Rust-generated preview independent of schematic placement/routes. Stage 1 normalization is complete; topology-aware network recognition and diffusion-sharing order remain next.

## Handoff

- Context ID: HANDOFF-001
- Last known state: Milestones 0–2 are functional. Milestone 2 includes the Rust switch solver, five-state visualization, truth tables, diagnostics, and a bounded GTKWave-inspired waveform view.
- User acceptance: NAND behavior and truth-table UI were manually confirmed; the long-duration waveform sizing issue was fixed before the Milestone 2 commit.
- Milestone 3.2: format-v1 YAML supports concise `vt`, `ron`, `reference_width`, and `reference_length` aliases; requires a validated `max_metal_layers` routing ceiling; rejects incomplete, unknown, unsupported, and non-physical data; and embeds the validated technology snapshot in `.chippy` projects. Older embedded technologies default to five metals.
- UI: with no schematic selection, the circuit inspector shows the active technology and provides Load YAML and Use built-in controls. Loading/resetting is dirty-tracked and undoable; older projects receive the built-in technology.
- Milestone 3.3: NMOS/PMOS width and length are editable in the inspector, saved with the component, undoable, and backward-compatible. Rust derives technology-specific effective on-resistance plus gate/diffusion capacitance.
- Milestone 3.4: HIGH/LOW map to the active supply/ground rails; NMOS/PMOS compare gate voltage to polarity-specific threshold; active paths accumulate effective device resistance. Simulation inspection shows rail voltage, output drive resistance, and per-device Vg/Vt/Ron.
- Milestone 3.5: each net accumulates connected gate and diffusion capacitance; the active drive path supplies R; timed outputs use `0.69 × R × C`, with fractional-nanosecond waveform samples. Static simulation exposes load and estimated delay.
- Milestone 3.7: transistor circuits and circuits composed from existing blocks can be captured behind promoted input/output/VDD/GND pins as shared definitions. Arbitrary-depth instances deterministically flatten for all analyses and physical generation; hierarchy DRC covers nested interfaces, missing dependencies, removed pins, and direct/indirect recursion. Dedicated descend-and-edit canvas navigation remains a Milestone 4 refinement.
- Block capture UX: Save as Block opens an in-app modal with a prefilled name, progress state, and inline Rust validation errors; transistor placement lives only in the scrollable library to keep top navigation compact.
- Block symbol convention: promoted inputs are labeled beyond the left edge, outputs beyond the right, VDD above the top, and GND below the bottom; same-role pins are evenly spaced and the instance reference sits above the VDD label band.
- Block library persistence: saving a `.chippy` project mirrors embedded definitions into sibling `chippyblocks/*.chippyblock`; manual export writes the complete dependency set there, and project open imports sorted valid block files while deduplicating by definition ID or name and rejecting an invalid dependency graph.
- Hierarchical simulation settling: provisional `FLOATING` internal gates remain open while upstream block stages resolve; explicit `UNKNOWN`/contended gates remain conservative. This prevents early unresolved NAND stages from poisoning shared VDD nets. DRC now reports each unconnected promoted block pin against its parent instance.
- Milestone 3.6 preview: Rust chooses a near-square row-bank geometry for large device arrays, emits placement-independent physical bounds and layer rectangles, and staggers folded-row drops while distributing signals across process metals. The dedicated 3D renderer consumes only these shapes.
- Physical routing: Rust reserves a central channel between PMOS/NMOS banks, gives every logical signal a distinct Y track, uses the top available metal for horizontal trunks, and separates power/PMOS/NMOS drops across lower directional roles when the process permits. Channel pitch is `0.28` landing + `0.04` clearance = `0.32`, independent of unchanged transistor pitch; transitions receive landing pads and adjacent-layer via stacks.
- 3D navigation: view switching, top-fit reset, IR counts, layer visibility, and interaction help live in a scrollable physical-layout sidebar; floating simulation/navigation overlays are suppressed in 3D.
- Scalable navigation: schematic zoom spans 2–4096 design units, `F`/Home fits all placed geometry, Shift-arrows pan faster, and 3D MapControls permit substantially wider zoom-out and fine-detail zoom-in while Top view / Fit restores generated bounds.
- Project framing: create/open operations issue a schematic fit revision so loaded geometry is visible immediately; ordinary edits, undo/redo, and simulation updates preserve the user's camera.
- Waveform reachability: the setup sidebar and signal workspace are height-constrained independent scrollers; growing signal lanes always remain vertically reachable while the time axis retains horizontal scrolling.
- Waveform inspection: click or drag the plot to scrub a vertical time cursor; every sticky signal label displays its cursor value. A second sticky boundary badge shows `1`, `0`, or `X` for the trace value at the leftmost visible time where the scrolled waveform meets the label block, vertically aligned with the trace level.
- Roadmap: Milestone 11.1 adds live physical signal-path visualization after routed connectivity equivalence, including HIGH/LOW/floating/contended/unknown net coloring, conducting-channel illumination, net isolation, and waveform-time scrubbing without regenerating geometry.
- Roadmap: Milestone 4.1 adds process-owned physical DRC rules and 3D violation overlays for grid, width, spacing, contacts/vias, enclosure, wells, metal geometry, taps, rails, and pin access. It is explicitly a step toward signoff-ready generation; foundry-qualified correlation remains Milestone 9.
- Roadmap: Milestone 4.2 runs 2–3 deterministic topology, compactness, and congestion/pin-aware placement-routing candidates, rejects non-DRC-clean results, and scores legal area/routing cost before a bounded aspect-ratio tie-breaker. Milestone 5.1 adds timing/fanout/physical-RC costs so critical designs may choose a larger or less-square candidate when justified.
- Roadmap: Milestone 4.3 adds persisted grouped waveform buses with explicit bit ordering, binary/hex radix, cursor-time values in sticky labels, interval values, conventional crossed `X` change boundaries, scalar expand/collapse, and `X` propagation for unresolved members.
- Next useful step: Implement Milestone 3.6 stage 2 complementary pull-up/pull-down network recognition and deterministic transistor ordering.
- Validation: `cargo test` passes 52 tests, including near-square folding of a 128-MOS array, recursive nested flattening, indirect-cycle rejection, cascaded reusable-NAND settling, adjacent block-library round-trip/deduplication, and hierarchy diagnostics; `npm run build` and `npx tauri build` pass. The existing Three.js chunk-size advisory remains.

## Recent Changes

| Date | Tags | Change | Commit | Remote |
| --- | --- | --- | --- | --- |
| 2026-07-19 | roadmap, milestone-4.3, waveform-buses | Mapped persisted grouped traces with ordered bits, binary/hex display, cursor and interval values, crossed change edges, and unresolved-value propagation. | Uncommitted | Not confirmed |
| 2026-07-19 | waveform, sticky-edge-values | Replaced static binary level markers with sticky `1`/`0`/`X` badges showing each trace value where the horizontally scrolled waveform meets its signal block. | Uncommitted | Not confirmed |
| 2026-07-19 | waveform, cursor, signal-values | Added click/drag time scrubbing, a vertical trace cursor, exact cursor time, and color-coded values in every signal label. | Uncommitted | Not confirmed |
| 2026-07-19 | editor, open-fit, waveform-scroll | Auto-fit newly created/opened schematics without disrupting editing cameras and made large waveform lane sets vertically scrollable. | Uncommitted | Not confirmed |
| 2026-07-19 | roadmap, milestone-4.2, milestone-5.1, multi-pass-routing | Mapped deterministic 2–3 candidate placement/routing with area-first legal scoring, bounded aspect preference, and later timing-aware selection. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.6, router, channel-compaction | Derived routing-lane pitch from via landing plus clearance, reducing empty channel height without changing transistor spacing or allowing same-layer overlap. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.6, router, collision-isolation | Reserved a central signal channel, assigned unique track coordinates, separated drop/trunk layer roles, and rebuilt transition via stacks to prevent cross-net routing collisions. | Uncommitted | Not confirmed |
| 2026-07-19 | roadmap, milestone-4.1, physical-drc, signoff | Mapped versioned process-rule decks, headless Rust geometry checks, and selectable 3D violation overlays as a step toward signoff-ready generation. | Uncommitted | Not confirmed |
| 2026-07-19 | roadmap, milestone-11.1, signal-visualization | Mapped simulation-aware 3D path highlighting after physical connectivity equivalence. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.6, milestone-3.8, compact-placement, navigation | Folded large MOS arrays into aspect-ratio-aware row banks and expanded schematic/3D fit, zoom, and pan controls. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.7, nested-blocks, hierarchy | Added arbitrary-depth block composition, deterministic recursive flattening, dependency-safe export/load, nested-pin DRC, and direct/indirect cycle rejection. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.7, hierarchy-simulation, drc | Fixed provisional floating-gate poisoning across cascaded blocks and added parent-level unconnected block-pin warnings. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.7, block-library, persistence | Added automatic sibling `chippyblocks/` save, export, discovery, validation, and deduplication. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.7, block-symbol, pin-labels | Added labeled edge-specific reusable-block pins with VDD on top and GND on the bottom. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.7, block-dialog, navigation | Replaced the unreliable webview prompt with an in-app block-save dialog and removed transistor placement from the top bar. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.7, hierarchy, readme | Added shared reusable blocks, promoted pins, compact instances, portable export, hierarchy-aware analysis/DRC/physical flattening, and current capability documentation. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.2, milestone-3.6, process-layers | Added an explicit YAML routing-layer ceiling, five-metal educational default, bounded multi-layer routing, and dynamic 3D layers. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.6, router-isolation | Reworked preview routing around directional M1 drops/M2 signal trunks and added same-layer cross-net overlap regression coverage. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.6, router, metal2 | Restored conflict-aware M1/M2 assignment and physical endpoint vias in the Rust preview router. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.6, compact-preview, 3d-sidebar | Replaced schematic-derived 3D with Rust physical shapes and moved layout controls into a sidebar. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.6.1, physical-ir | Added deterministic Rust connectivity normalization and a visible 3D IR status boundary. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-3.1–3.7 | Added the educational technology/R/C timing stack and mapped compact physical synthesis plus reusable hierarchical blocks. | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-2, waveform-fix | Kept long-duration waveforms bounded to fixed signal lanes. | `4d97d30` | `origin/alpha-v1` |
| 2026-07-19 | milestone-2.5, waveform | Added timed Rust sampling, Simulate dropdown, and Waveforms view. | `4d97d30` | `origin/alpha-v1` |
| 2026-07-19 | milestone-2.4, diagnostics | Added UNKNOWN stimulus and linked simulation inspection. | `4d97d30` | `origin/alpha-v1` |
| 2026-07-19 | milestone-2.3, truth-table | Added production-solver truth tables plus NOR/transmission-gate acceptance. | `4d97d30` | `origin/alpha-v1` |
| 2026-07-19 | milestone-2.2, visualization | Added 2D/3D state colors, active switches, inputs, and outputs. | `4d97d30` | `origin/alpha-v1` |
| 2026-07-19 | milestone-2.1, simulation | Added the headless Rust switch-level solver and typed API. | `4d97d30` | `origin/alpha-v1` |

## Open Threads

- Keep Milestone 3’s RC model explicitly educational; SPICE/nonlinear analog behavior is out of scope.
- Explicit persisted net identity, editable physical layout, and full PDK design-rule support remain future architecture work.
- Group movement, direct wire-to-wire branching, and scalable routing beyond the temporary two-metal heuristic remain editor work.
- The Three.js bundle still triggers Vite’s 500 kB advisory; optimize when profiling justifies it.
