# CondensedContext

## Context Freshness
Context ID: FRESH-001
Last Verified Commit: 949319ffdc17b6acd7941182b6f6584ec617c177
Current HEAD: 949319ffdc17b6acd7941182b6f6584ec617c177
Generated: 2026-07-19
Status:
- Partial; Milestone 0 foundation files are implemented and uncommitted.
Files requiring verification:
- None for the current foundation.

## Purpose
Durable, compact memory for agent work in this repository.

## Current Focus
Context ID: ACTIVE-001
Confidence: High; verified against source and frontend build.
- Milestone 0 is complete. Milestone 1 now includes EDA-style dangling routes; close it by validating the inverter and NAND examples.

## Handoff
Context ID: HANDOFF-001
Confidence: High.
- Last known state: Wire routing remains active across repeated grid clicks, persists each click as an orthogonal waypoint, and completes when a terminal is selected; Escape leaves the current endpoint dangling.
- Next useful step: Build the inverter and NAND acceptance circuits, visually verify free routing and inferred layer assignment, and run DRC before closing Milestone 1.
- Validation: `npm run build`, `cargo test` (15 tests), and `npx tauri build` passed after dangling-route support.

## Stable Facts
Context ID: FACTS-001
Confidence: High; verified from the roadmap.
- OpenChippy is an education-first, desktop-first VLSI design studio with future WebAssembly support.
- Rust owns the project model and application services; the web frontend owns interactive visualization.

## Decisions
Context ID: DECISIONS-001
Confidence: High.
- Begin with a narrow vertical slice that can place, save, reload, undo, and redo a placeholder component.
- Keep the serializable project model and history independent from UI code; expose Tauri commands as a thin adapter.
- Keep the plugin boundary in-process until a future trust and dynamic-loading model is defined.
- Treat New and Open as history boundaries; undo cannot cross between projects.
- Keep file path, saved snapshot, dirty state, and history availability authoritative in the Rust workspace session.
- Use one persisted component model across schematic and 3D representations; view mode is ephemeral UI state.
- CMOS transistors and their connectivity are the primary product objects; resistors are secondary characteristic/passive components.
- Persist wires as terminal references in the Rust project model so movement and view switching preserve connectivity.
- Milestone 0 accepted complete: desktop shell, Rust/React/Three.js architecture, project lifecycle, persistence, undo/redo, and plugin skeleton are functional.
- Schematic navigation convention: wheel zoom, middle-button drag pan, Shift-click toggles multi-selection.
- Junctions model branch points through repeated connections to their `node` terminal; output probes use `in`, and net labels use `node`.
- Component names, including net labels, are editable in the inspector and enforced unique by the Rust model.
- Basic DRC is read-only and Rust-owned; diagnostics have stable codes, error/warning severity, messages, and affected component IDs.
- Equal net-label names participate in DRC connectivity even when their wire segments are not drawn together.
- Wire endpoints remain bound to terminals; `routeX` stores the editable central orthogonal channel and is backward-compatible when absent.
- Navigation convention: ordinary wheel/trackpad deltas pan, pinch or Ctrl/Command-wheel zooms, empty-canvas drag pans, and arrow keys pan.
- Junction-on-wire placement is a topology edit: the matched orthogonal wire is removed and replaced by two connections sharing the junction `node`.
- 3D visualization targets a Magic/PDK-style cell layout, not isolated physical transistor sculptures: layer paint and routing geometry form one standard-cell view.
- Current layout dimensions are abstract technology-relative units inferred from placed-object extents; they are not fixed physical nanometer dimensions.
- Metal wires render as rectangular Metal 1 polygons with contact/via cuts at endpoints; future technology definitions will choose layer stack, widths, spacing, and via rules.
- 3D navigation uses map controls: primary drag pans, secondary drag rotates, wheel/pinch zooms, and arrow keys pan.
- The initial 3D camera is orthographic and top-down, centered on populated-cell geometry; users can orbit into perspective afterward.
- Camera framing uses the post-generation Three.js bounding box and viewport aspect ratio; schematic-coordinate bounds are not trusted for final fit.
- Orthographic fitting uses a normalized screen frustum and calculated zoom instead of mutating frustum world size, keeping physical-center projection stable.
- Circuit/project names are mutable model data; blank names are rejected and changes participate in persistence and history.
- Schematic device geometry uses screen-stable stroke weights so symbols remain legible while zooming.
- Schematic junctions do not imply a layer transition. They produce no layout contact/via; physical contacts are emitted only at Metal 1-to-device-layer endpoints.
- Temporary physical routing inference groups wires by shared terminals, junction nodes, and equal-name net labels, then colors conflicting nets onto Metal 1 or Metal 2.
- M2 routes receive explicit M1–M2 vias where they meet device contacts, rails, or I/O pins; schematic junctions and labels do not create physical vias.
- Wires persist an optional destination terminal, free endpoint, and ordered waypoint list. Existing wire JSON remains compatible; an active route accepts repeated bends until a terminal completes it or Escape leaves it dangling.
- DRC represents a free endpoint as part of its source net and emits a `dangling_wire` warning rather than a malformed-reference error.

## Known Constraints
Context ID: CONSTRAINTS-001
Confidence: High.
- Preserve the user's uncommitted `README.md` and `docs/roadmap.md`.
- Do not implement later schematic or simulation milestones in this foundation pass.

## File Map
Context ID: FILEMAP-001
Confidence: High; files verified during implementation.
- `docs/roadmap.md`: product direction and milestone acceptance criteria.
- `src/`: React application, desktop bridge, Three.js viewport, and styling.
- `src/SchematicViewport.tsx`: SVG-based 2D schematic grid, symbols, selection, and snapped placement coordinates.
- `src-tauri/src/model.rs`: versioned serializable project model.
- `src-tauri/src/history.rs`: snapshot undo/redo service and unit tests.
- `src-tauri/src/lib.rs`: persistence and project-state Tauri commands.
- `src-tauri/src/plugins.rs`: initial internal plugin trait and registry.
- `src-tauri/src/validation.rs`: basic connectivity graph and Milestone 1 DRC diagnostics.
- `src-tauri/icons/`: generated source icon plus Tauri desktop/mobile icon variants.

## Validation Memory
Context ID: VALIDATION-001
Confidence: High.
- `npm run build`: TypeScript check and Vite production build; passed 2026-07-19.
- `cd src-tauri && cargo test`: passed 13 tests on 2026-07-19 using Rust 1.97.1.
- `npx tauri build`: passed on 2026-07-19; release executable produced successfully.

## Coverage
Context ID: COVERAGE-001
Confidence: High.
- Files indexed: all authored application, configuration, and documentation files.
- Coverage notes: Generated dependency lockfile not semantically indexed.

## Changes
| Date | Tags | Change | Files | Commit | Remote |
| --- | --- | --- | --- | --- | --- |
| 2026-07-19 | schematic, routing, waypoints, dangling-wires, eda | Added persistent multi-click routing sessions, saved orthogonal waypoints, later terminal completion, endpoint affordances, and dangling-route DRC warnings. | `src/`, `src-tauri/src/`, `CondensedContext.md` | Uncommitted | Not confirmed |
| 2026-07-19 | 3d, routing, metal2, vias, nand | Added net-aware crossing detection and inferred two-metal routing so unrelated NAND nets can cross without appearing shorted. | `src/Viewport.tsx`, `src/styles.css`, `CondensedContext.md` | Uncommitted | Not confirmed |
| 2026-07-19 | schematic, visibility, naming | Strengthened schematic symbol strokes and added persisted, undoable circuit renaming. | `src/`, `src-tauri/src/`, `CondensedContext.md` | Uncommitted | Not confirmed |
| 2026-07-19 | 3d, camera, map-controls, pan | Switched to normalized orthographic fitting and map-style drag/keyboard panning. | `src/Viewport.tsx`, `src/App.tsx`, `CondensedContext.md` | Uncommitted | Not confirmed |
| 2026-07-19 | 3d, camera, fit | Replaced origin-based framing with post-render physical-bounds fitting and added a Top view reset. | `src/Viewport.tsx`, `src/styles.css`, `CondensedContext.md` | Uncommitted | Not confirmed |
| 2026-07-19 | 3d, camera, centering, junctions | Centered an orthographic top-down initial layout view and separated schematic junctions from physical contacts/vias. | `src/Viewport.tsx`, `CondensedContext.md` | Uncommitted | Not confirmed |
| 2026-07-19 | 3d, pdk, standard-cell, layout, routing | Replaced device sculptures with an auto-framed Magic-style layered cell view and Metal 1 polygon/via routing. | `src/Viewport.tsx`, `src/styles.css`, `CondensedContext.md` | Uncommitted | Not confirmed |
| 2026-07-19 | junctions, wires, 3d, mosfet, camera | Added automatic junction wire splitting, wire deletion, layered educational MOSFET structures, and orbit camera navigation. | `src/`, `src-tauri/src/` | Uncommitted | Not confirmed |
| 2026-07-19 | wires, routing, navigation, macOS | Added selectable/persisted wire routing plus empty-canvas, trackpad, and keyboard panning with pinch zoom. | `src/`, `src-tauri/src/` | Uncommitted | Not confirmed |
| 2026-07-19 | drc, validation, layout | Added scrollable catalog/inspector layout and Run DRC with graph-based Milestone 1 diagnostics and linked results. | `src/`, `src-tauri/src/validation.rs`, `src-tauri/src/lib.rs` | Uncommitted | Not confirmed |
| 2026-07-19 | symbols, junctions, nets, probes | Corrected VDD; added persisted junctions, renameable net labels, output probes, distinct 3D forms, and toolbar rotation. | `src/`, `src-tauri/src/` | Uncommitted | Not confirmed |
| 2026-07-19 | milestone-0, milestone-1, selection, navigation | Marked M0 complete; added boxed selection, multi-select, rotate/delete, zoom/pan, shortcuts, and wire-safe transforms. | `src/`, `src-tauri/src/`, `CondensedContext.md` | Uncommitted | Not confirmed |
| 2026-07-19 | cmos, transistor, wiring, 3d, milestone-1 | Re-centered the editor on NMOS/PMOS, rails, inputs, movement, named terminals, and persisted connectivity across 2D/3D. | `src/`, `src-tauri/src/` | Uncommitted | Not confirmed |
| 2026-07-19 | schematic, resistor, 3d, milestone-1 | Added default 2D schematic editing, snapped resistor placement, and a shared-model 2D/3D view switch. | `src/`, `src-tauri/src/` | Uncommitted | Not confirmed |
| 2026-07-19 | foundation, lifecycle, persistence | Added authoritative workspace lifecycle state, Save As, dirty/history UI, compatibility checks, and discard prompts. | `src/`, `src-tauri/src/` | Uncommitted | Not confirmed |
| 2026-07-19 | assets, build | Added the OpenChippy application icon set and verified a complete Tauri release build. | `src-tauri/icons/` | Uncommitted | Not confirmed |
| 2026-07-19 | foundation, ui, rust | Added the Milestone 0 desktop vertical slice and development documentation. | `src/`, `src-tauri/`, project config, `README.md` | Uncommitted | Not confirmed |
| 2026-07-19 | foundation, context | Initialized durable project memory. | `CondensedContext.md` | Uncommitted | Not confirmed |

## Archived History
- None yet.

## Open Threads
Context ID: OPEN-001
Confidence: High.
- Manually build and persist a CMOS inverter using VDD, PMOS, NMOS, GND, and a digital input.
- Explicit stored net identity and per-segment route editing remain open; DRC currently derives graph connectivity from terminal-pair wires and equal label names.
- Automatic junction attachment currently selects the first orthogonal wire within a small placement tolerance.
- Layer geometry is currently inferred in the frontend and is not yet persisted or editable as layout paint; PDK technology files and a true layout model remain open.
- The temporary router has two layers and uses greedy net conflict coloring; dense, non-two-colorable routing conflicts need a future channel router, route detours, or persisted layout editing.
- DRC needs manual acceptance against realistic inverter/NAND examples before Milestone 1 is declared complete.
- Multi-selection transforms rotate/delete all selected devices; group movement is not yet implemented.
- Wires support arbitrary multi-click orthogonal waypoint paths; direct wire-to-wire branch creation without placing a junction remains for the next circuit-graph iteration.
- The initial Three.js bundle triggers Vite's advisory 500 kB chunk warning; optimize only when startup profiling makes it worthwhile.
