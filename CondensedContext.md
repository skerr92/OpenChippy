# CondensedContext

## Context Freshness

- Context ID: FRESH-001
- Last Verified Commit: `34d386a9396635b6569f0e372bbd23e17027d521`
- Current HEAD: context-only successor of `34d386a9396635b6569f0e372bbd23e17027d521`
- Generated: 2026-08-11
- Status: partial; the committed v46 baseline is verified, while the FEOL correction and first v47 full-die/perimeter-I/O slice are validated but uncommitted.
- Files requiring verification: all currently modified working-tree files; see `git status --short`.
- Full semantic memory: `CondensedContext.CCF1`

## Current Focus

- Context ID: ACTIVE-001
- Confidence: High for the current working tree and recorded qualification results; source remains authoritative.
- Physical IR v47 extends the v46 density baseline with full-usable-area die bounds, process-defined perimeter rings/pads, physical-pin site metadata, and routed schematic GPIO anchors.
- FEOL field reservation now consumes that same report, so existing active/poly geometry counts toward total preferred density instead of being mistaken for missing dummy fill and forcing false tapeout-height failures.
- New tapeout direction: unless a process explicitly requests content-fit behavior, the top-level die uses the complete usable tapeout area. Keep compact core-placement bounds separate from die bounds; represent large repetitive fill as scalable tiled/array regions rather than millions of independent IR rectangles.
- Process tapeout contracts now own ordered edge power/ground rings and named power, ground, and GPIO pads. Projects persist unique role-compatible bindings from schematic digital input/output components to process GPIO pads; assigned GPIO routing reaches the fixed pad. Templates with redundant VDD/GND pads additionally emit two full-height, 20 µm top-metal trunks per supply, stitch those trunks through vias to the matching pads/rings, and use collision-aware routing to close the core supply islands.
- Known GF180 project snapshots that predate perimeter metadata are migrated on load: an empty ring or pad collection is replaced with the packaged 2920 × 3520 µm, two-ring, 42-pad contract. This also changes the effective project digest so incomplete cached v47 layouts are not reused.
- `.chippyblock` is now a versioned file envelope. Legacy bare definitions are treated as v0, missing revision zero is normalized to revision one, the newest interface-compatible project/library definition wins, and the adjacent file is rewritten in the current format. Future formats and incompatible pin-interface upgrades remain explicit errors.
- Adjacent block libraries are loaded atomically: every file is parsed and revision-resolved on a staging project before the complete dependency graph is validated. Parent filenames may sort before nested leaf dependencies without producing transient `missing block definition` failures.
- Save As to a different destination now forks the project UUID. Definitions authored by the original project retain their original source identity and become imports in the copy, preventing edits to the copied circuit from refreshing or overwriting the original block library.
- Chippyblock interfaces evolve additively: a newer revision may add named input/output/power/ground pins while preserving every existing pin name and role. Parent instances retain their wires and expose added pins as initially unconnected terminals; removal or role changes remain rejected until an explicit pin-migration mechanism exists.
- Block-library revisions now require electrically complete internal terminals before displacing an embedded definition. Invalid newer files are preserved for repair but cannot poison hierarchy flattening; opening a genuine block-source project can recover stale Save-As ownership from matching top-level pin UUIDs and regenerate its library definition. Intentional Save-As copies carry an explicit origin marker and are excluded from that recovery.
- The EDU 5M template treats Metal 5 as external-pad/top metal, places PDN rings on M4/M3, and emits bounded 10 × 10 µm M5 dummy islands on electrically inert GDS datatype 4. It evaluates explicit 400 × 400 µm density windows stepped 200 µm and applies a 60 µm cross-layer ring keepout. This is an educational rule contract, not a foundry-qualified recipe.
- Density decks can specify independent X/Y evaluation-window steps; omitted steps retain the half-window default, while validation requires paired, positive, on-grid steps no larger than the evaluation window. Reports and the 3D viewer expose both window and step dimensions.
- The packaged GF180 deck now follows published dummy-metal geometry: 2 × 2 µm M1–M5 tiles, 1.2 µm dummy spacing, 2.0 µm dummy-to-circuit spacing, and 200 × 200 µm windows stepped 100 µm. Its thick 3 µm MetalTop option uses the published 2.0 µm dummy spacing.
- Pad placement model: process templates define stable pad-site IDs with exact side/offset or coordinates, geometry, layer, role/capabilities, and optional fixed signal identity. Caravel-style sites are immutable; generic sites are generated. The schematic owns only the assignment from a digital input/output component to an eligible site, edited through a perimeter view or inspector dropdown and persisted by component UUID plus site ID.
- Generic fallback template interprets the requested baseline as a 1000 × 1000 µm die (1 mm²), not literal 1000 µm². Default user GPIO is five sites on each of two opposite edges (10 total); VDD/GND pads occupy the other opposite edges and connect to process-defined rings. Template tiers may scale GPIO capacity 10→40→160 as die area scales 1→4→16 mm², while power-pad count/width scales from current/perimeter rules rather than blindly with GPIO count.
- Existing circuit/device spacing remains a hard fill keepout. Dedicated critical-net, clock/power, antenna, and coupling-aware keepout classes are intentionally deferred until the process contract can express non-invented distances.
- The committed v45 exact legacy GF180 4B baseline remains native DRC/connectivity/LVS and official geometry/process DRC clean; v46 still needs exact-design regeneration and official-deck correlation before inheriting that external qualification claim.

## Handoff

- Context ID: HANDOFF-001
- Last known state: v46 density closure is committed at `34d386a`; its FEOL fix, v47 tapeout perimeter slice, and GF180 vertical top-metal PDN stitching are validated and uncommitted.
- Next useful step: add scalable full-area density arrays and explicit core bounds, then add binding-specific DRC before generic fallback template tiers/perimeter editor.
- Validation: complete Rust suite passes 193/193 on 2026-08-11; focused tests prove all example process decks pass physical DRC and GF180 emits 42 pads, two rings, and four full-height connected M5 supply trunks.

## Recent Changes

| Date | Tags | Change | Commit | Remote |
| --- | --- | --- | --- | --- |
| 2026-08-11 | simulation, flattening, nand, block-integrity | Prevented incomplete newer block revisions from overriding valid embedded logic, added stale-source recovery, and locked hierarchical NAND `HIGH,HIGH → LOW` into the latch regression. | Uncommitted | Not confirmed |
| 2026-08-11 | chippyblock, interface, revision, hierarchy | Allowed additive block-pin revisions so existing parent designs automatically gain new terminals without losing established wiring. | Uncommitted | Not confirmed |
| 2026-08-11 | save-as, project-identity, blocks, ownership | Made Save As create an independent project identity while preserving original block authorship as imports, so altered copies save without mutating the source library. | Uncommitted | Not confirmed |
| 2026-08-11 | chippyblock, dependency-graph, compatibility, loader | Made multi-file block-library loading atomic and independent of filename order, fixing parent-before-leaf missing-definition failures such as `2_1_MUX` before `TRANSM_GATE`. | Uncommitted | Not confirmed |
| 2026-08-11 | chippyblock, migration, versioning, library | Added automatic legacy block-file migration and newest-compatible revision synchronization when adjacent block libraries are loaded. | Uncommitted | Not confirmed |
| 2026-08-11 | gf180, migration, cache, viewer | Hydrated missing GF180 rings/pads from the packaged deck during project load and invalidated physical caches generated from the incomplete embedded snapshot. | Uncommitted | Not confirmed |
| 2026-08-11 | gf180, pdn, top-metal, power-routing | Added four full-height 20 µm M5 supply trunks (two VDD/two GND), assigned supply nets to matching pads/rings, added ring-crossing via stacks, and collision-aware core-to-grid closure. | Uncommitted | Not confirmed |
| 2026-08-11 | physical-ir-v47, tapeout, pads, gpio, viewer | Added full-die mode, validated process rings/pads, persistent schematic GPIO-site assignments, perimeter-aware routing anchors, rendered ring/pad geometry, and viewer counts. | Uncommitted | Not confirmed |
| 2026-08-11 | physical-ir-v47, top-metal, dummy-fill | Made M5 the EDU external-pad layer, added inert dummy GDS mappings and sparse full-field M5 preview fill, ordered explicit-template fill after die/perimeter establishment, and excluded inert fill from fine DRC spatial indexing. | Uncommitted | Not confirmed |
| 2026-08-11 | simulation, sequential-state, edu-yaml | Added digest-scoped interactive simulation memory and waveform carry-forward, distinguished unresolved feedback from true floating nets, added reusable-NAND latch coverage, and aligned the EDU dummy mapping with its sole generated M5 fill material. | Uncommitted | Not confirmed |
| 2026-08-11 | tauri, async-io, shortcuts, clipboard | Moved project/technology/RTL/physical/GDS/LEF/report/block file work onto blocking workers with snapshot-and-commit locking; added platform-aware save/open and schematic copy/paste shortcuts with offset geometry and preserved internal wires. | Uncommitted | Not confirmed |
| 2026-08-11 | density-fill, ring-keepout, edu | Reduced EDU top-metal dummy tiles from 160 µm to 40 µm and added a validated per-fill-layer cross-layer ring keepout, configured to 60 µm for the EDU perimeter. | Uncommitted | Not confirmed |
| 2026-08-11 | density-fill, gf180, edu, sliding-window | Added explicit X/Y density-window steps; aligned packaged GF180 metal fill with published 2 µm tiles, spacing, and 200/100 µm window cadence; gave EDU a bounded 10 µm-tile, 400/200 µm educational contract. | Uncommitted | Not confirmed |
| 2026-08-11 | performance, density-fill, drc, spatial-index | Replaced quadratic full-IR scans during fill admission and fill-spacing DRC with conservative coarse spatial queries followed by unchanged exact rule checks; retained deterministic neighbor order. | Uncommitted | Not confirmed |
| 2026-08-11 | ihp, gf180, density-fill, process-yaml | Kept GF180 on its published 2 µm/1.2 µm/200 µm/100 µm metal-fill contract and provisionally aligned IHP M1–M5 to it; repaired the strict IHP PWell mapping and added process/validation fingerprint linkage coverage. | Uncommitted | Not confirmed |
| 2026-08-11 | blocks, source-link, save, simulation | Added stable project authorship plus block source ID/digest/revision; Save refreshes authored blocks, legacy snapshots are safely adopted by matching boundary UUIDs, dependent loads accept newer interface-compatible revisions, and parent saves cannot overwrite imported source blocks. | Uncommitted | Not confirmed |
| 2026-08-11 | tauri, acl, dialogs, save, physical-cache | Granted the main window dialog message permission used by ask/error UI, and made `.ochippy` saves persist circuit/manifest successfully without forcing physical regeneration when the cache is stale; a matching physical artifact remains included when available. | Uncommitted | Not confirmed |
| 2026-08-11 | gf180, caravel, perimeter, pads, rings | Added a full 2920 × 3520 µm GF180 Caravel-sized template with M4/M3 PDN rings and 42 M5 pads: 38 GPIO split across left/right edges, two top VDD, and two bottom GND. Fine GF180 density remains core-scoped pending repeated-array IR, avoiding millions of viewer rectangles while preserving exact local fill rules. | Uncommitted | Not confirmed |
| 2026-08-11 | physical-ir-v46, density, feol, bugfix | Made FEOL feasibility use achieved total global/worst-window density instead of comparing dummy-only area with the total target, preventing false tapeout-height failures. | Uncommitted | Not confirmed |
| 2026-08-11 | physical-ir-v46, density, sliding-window | Added process-configurable global/window minima, preferred targets and maxima; boolean-union measurement; edge-anchored half-window closure; persisted audit/viewer reporting; and validation coverage. | `34d386a` | Not confirmed |
| 2026-08-04 | physical-ir-v45, gf180, m1-neck | Repaired short zero-gap route-to-landing junctions without introducing concave-corner violations; exact GF180 4B official geometry/process DRC is clean. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v44, density-fill, legacy-gf180 | Required fill on every configured material, exposed per-layer counts and viewer controls, and consistently migrated narrowly identified legacy GF180 snapshots. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v43, generation-contract, cache | Applied GF180 manufacturing migration at generation/digest boundaries and rejected configured processes that emit no dummy fill. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v42, topology-drc | Made terminal-obligation DRC understand topology-internal shared active and hydrated missing canonical GF180 manufacturing contracts. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v41, internal-access, viewer | Removed routing/access stacks for nets fully closed inside proven shared-active islands and exposed synthetic top-metal fill in 3D. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v40, cell-boundary | Restricted shared diffusion/poly transformations to devices in the same explicit leaf standard-cell instance. | `2b9a6ca` | Not confirmed |

## Open Threads

- Extend sequential simulation beyond the completed state-seeded NAND-latch behavior: add explicit DFF waveform fixtures, metastability/recovery semantics, and time-aware charge decay only when the model can distinguish storage nodes from genuinely open combinational nets.
- Complete the top-level tapeout contract: explicit core bounds, scalable fill arrays, binding-specific DRC, generic fallback template tiers, and graphical perimeter assignment. Full-die mode, process rings/pads, persisted inspector bindings, GPIO pad-access routing, and redundant-pad top-metal PDN stitching now exist.
- Extend Physical IR/GDS/viewer with repeated density arrays, then switch GF180 `cover_full_usable_area` to true and qualify full-reticle 2 × 2 µm fill without materializing several million independent preview rectangles.
- Add process-owned spacing classes for critical nets, clocks/power, antenna risk, and coupling-sensitive geometry; existing generic circuit/device keepouts remain enforced.
- Requalify exact GF180 4B v46 output with native DRC/connectivity/LVS, deterministic GDS, persisted density metrics, and the official deck.
- Revisit redundant route branches, transition stacks, and path-length compaction only with DRC/connectivity/LVS/determinism gates intact.
- The broader roadmap still defers full RTL synthesis/tool integration, linked multi-module RTL, and production signoff claims; use `docs/roadmap.md` and `docs/manufacturing_roadplan.md` for scope.
