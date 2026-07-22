# OpenChippy Alpha v1 → Milestone 5 Implementation Plan
**Title:** Occupancy-Aware Physical Canvas, Hierarchical Routing, and Progressive Build Experience

## Overview

The current physical design flow has reached a point where the architecture is proving itself, but several limitations remain from the original flattened routing model.

Recent work moving toward hierarchical blocks has already reduced DRC violations (113 → 98), which strongly suggests that solving placement and routing hierarchically is the correct long-term direction.

The next milestone should **not** focus on making the router "smarter" first.

Instead, it should introduce a **single authoritative Physical Canvas** that all placement and routing operations consult before committing geometry.

This milestone should also improve the user experience by introducing an asynchronous physical build process with a loading screen while layouts are generated.

## Implementation Status

- [x] **Phase 1A:** Add a Rust-owned `PhysicalCanvas` with layer occupancy,
  process-rule spacing halos, spatial neighbor queries, legality checks,
  incremental commit, and removal.
- [x] **Phase 1B:** Use `PhysicalCanvas` for physical-preview metal/via conflict
  scoring so route-order selection no longer relies on a separate fixed-clearance
  overlap implementation.
- [x] **Phase 2:** Index every occupied interval by process layer, dominant
  orientation, and each manufacturing-grid track it spans while retaining the
  uniform-grid spatial index for two-dimensional collision queries.
- [x] **Phase 3:** Generate the manufacturing-grid-snapped diffusion, poly, and
  contact footprint immediately after each candidate placement; atomically
  reserve it in PhysicalCanvas or retry deterministic nearest sites within the
  row before routing.
- [x] **Phase 4:** Move detailed and rendered-preview metal/via admission behind
  one atomic `PhysicalCanvas` routing commit API; rejected geometry is reported
  and never enters the corresponding physical IR.
- [x] **Phase 5:** Rebuild the selected placement's complete device occupancy
  before each detailed-routing attempt; rendered preview admission likewise
  reserves fixed device geometry before local power and signal shapes.
- [x] **Phase 6:** Commit segments incrementally through the Phase 7 spatial
  index, searching bounded neighboring tracks and compatible metal layers before
  escalating; commit connectors, landing metals, and complete via transitions
  atomically so rejected candidates leave no partial geometry.
- [x] **Phase 7:** Reuse the canvas-owned uniform-grid spatial index for DRC
  neighbor candidate generation and point/hit queries, including unchecked
  indexing of illegal IR that must remain diagnosable.
- [x] **Phase 8:** Materialize reusable instances as immutable physical block IR
  with owned devices/private nets/shapes, local DRC status, bounds, and promoted
  interface pins; exclude private block nets from top-level global routing.
- [x] **Phase 9–10:** Complete categorized origin statistics and asynchronous
  build progress UX.

The canvas is introduced incrementally so device occupancy is not enforced until
placement can retry another legal site; rejecting every existing dense candidate
without legalization would hide rather than resolve the defect.

---

# Goals

- Eliminate geometry collisions before they are committed
- Make placement and routing occupancy-aware
- Preserve existing global routing architecture
- Prepare for hierarchical block routing
- Improve responsiveness of the UI while large layouts generate
- Lay the groundwork for future DRC/LVS/timing improvements

---

# Current Architecture

Current flow:

```
Logical IR
    ↓
Physical Placement
    ↓
Global Routing
    ↓
Detailed Routing
    ↓
Physical IR
    ↓
Renderer
```

Current issues:

- Placement does not know actual generated geometry.
- Detailed routing only resolves route-vs-route conflicts.
- Device geometry is not treated as routing obstacles.
- No single occupancy representation exists.
- DRC primarily discovers illegal geometry after generation.

---

# Proposed Architecture

```
Logical IR
      ↓
Physical Placement
      ↓
Generate Device Geometry
      ↓
Build Physical Canvas
      ↓
Reserve Device Occupancy
      ↓
Power Routing
      ↓
Reserve Power Occupancy
      ↓
Signal Routing
      ↓
Incremental Occupancy Updates
      ↓
Physical IR
      ↓
Renderer
      ↓
Final DRC Verification
```

---

# Phase 1 - Physical Canvas

## Create PhysicalCanvas

Introduce a new module:

```
PhysicalCanvas
```

Responsibilities:

- Maintain authoritative physical occupancy
- Layer-aware geometry tracking
- Fast spatial queries
- Collision testing
- Incremental commits
- Future DRC helper

Suggested structure:

```rust
PhysicalCanvas
├── layers
├── occupancy
├── spatial_index
├── design_rules
├── can_place()
├── can_route()
├── commit()
├── remove()
└── query_neighbors()
```

---

# Phase 2 - Layer Occupancy

Introduce occupancy by process layer.

Example:

```rust
occupancy[layer][track]
```

Each occupied interval stores:

```rust
OccupiedInterval {
    start,
    end,
    owner,
    net,
    obstruction_type,
}
```

Possible obstruction types:

- Device
- Metal
- Poly
- Diffusion
- Contact
- Via
- Power Rail
- Keep-Out
- Block Boundary
- Reserved Routing Channel

---

# Phase 3 - Device Reservation

Immediately after placement:

Generate physical device footprints.

Reserve them inside PhysicalCanvas.

Placement should become:

```
Candidate placement

↓

Generate footprint

↓

Expand spacing halo

↓

Query occupancy

↓

Commit

OR

Try another site
```

Do **not** allow overlapping device geometry to ever enter the Physical IR.

---

# Phase 4 - Shared Routing API

Replace direct geometry creation with a common legality interface.

Instead of:

```rust
polygons.push(...)
```

Use:

```rust
canvas.can_route(candidate)

↓

canvas.commit(candidate)

↓

Physical IR
```

Every placement and routing operation should use the same legality rules.

---

# Phase 5 - Build Occupancy Before Routing

Pipeline:

```
Placement

↓

Generate Device Geometry

↓

Reserve Device Geometry

↓

Generate Local Power

↓

Reserve Power

↓

Detailed Routing

↓

Commit Each Segment

↓

Generate Physical IR
```

The router should never attempt to route through occupied geometry.

---

# Phase 6 - Incremental Routing

When routing:

1. Generate candidate segment
2. Expand required spacing
3. Query occupancy
4. If legal:
    Commit immediately
5. Otherwise:
    Search another track
6. If no legal track:
    Escalate back to router

Do **not** generate entire routes before checking legality.

---

# Phase 7 - Spatial Index

PhysicalCanvas should own a spatial acceleration structure.

Possible implementations:

- Uniform Grid
- R-Tree
- Quadtree

Used for:

- Neighbor queries
- Collision detection
- UI hit testing
- Future parasitic extraction
- DRC acceleration

Current debug target carried beyond this phase: route fragments must eventually
commit and clean up complete landing/via-stack units so a rejected neighboring
segment cannot leave an orphan via. Same-net redundant cuts may be merged or
removed, but manufactured via-to-via spacing remains enforced.

Phase 6 now prevents new partial detailed-route transitions and removes preview
vias whose two enclosure metals did not survive admission. Continue auditing
legacy preview generation and redundant same-net cut arrays as layouts scale.

---

# Phase 8 - Hierarchical Block Routing

Treat ChippyBlocks as physical implementation boundaries.

Each block owns:

- Placement
- Local routing
- Local power rails
- Local DRC
- Bounding box
- Interface pins

Top-level routing should only connect exposed pins.

Do **not** flatten blocks into transistors during normal implementation.

Current implementation keeps leaf transistor realization inside each frozen
physical block for local DRC and layer visualization, while top-level planning
and routing consume only the block boundary and exposed/shared nets. This is the
migration boundary for later cached definition-level macro reuse.

Future flow:

```
Chip

↓

Block

↓

Local Layout

↓

Verified Physical IR

↓

Frozen Macro

↓

Top-Level Integration
```

---

# Phases 9 + 10 - DRC Statistics and UI Build Experience (complete)

These phases ship together: the asynchronous physical build produces the categorized report consumed by the 3D build, failure, and inspection experience.

Categorize every violation.

Example:

```
Device overlap

Metal overlap

Minimum spacing

Via enclosure

Power collision

Routing congestion

Boundary violation
```

Also track where they originate:

```
Placement

Power Routing

Signal Routing

Geometry Generation

Import
```

These statistics should appear in the build report.

---

# UI Build Experience

Currently the UI waits synchronously for the entire physical layout generation process.

Replace this with an asynchronous build workflow.

## Desired UX

When the user selects:

```
3D View
```

Immediately switch to the 3D workspace.

Instead of blocking, display a build overlay.

Example:

```
-----------------------------------------

        🛠 Building Your Design...

    Generating Physical Layout

    ▓▓▓▓▓▓░░░░░░░░░░

    ✔ Logical Placement
    ✔ Device Generation
    ⏳ Routing Signals
    ⏳ Running DRC

    "Good silicon takes a little patience."

-----------------------------------------
```

When complete:

- Fade out overlay
- Automatically display finished design
- Keep camera position
- Allow interaction immediately

If build fails:

Display:

```
Build Failed

37 DRC Errors

Open Report
Retry
```

instead of showing a blank scene.

---

# Future Build Progress API

Introduce progress callbacks.

The current implementation moves synthesis onto a blocking worker, immediately opens the 3D workspace, and exposes honest whole-build activity, completion metrics, failure details, and retry. Fine-grained live callbacks remain the next API refinement; completed stage names are retained in the build report without presenting synthetic percentages.

Example:

```rust
BuildProgress {

    stage,

    percent,

    current_task,

    warning_count,

    error_count,

}
```

Stages might include:

- Initializing
- Placement
- Device Generation
- Local Routing
- Global Routing
- Physical IR
- DRC
- Rendering
- Complete

---

# Success Criteria

- No geometry is committed without legality checking.
- Placement reserves physical area before routing.
- Routing respects existing geometry.
- Device geometry becomes routing obstacles.
- Hierarchical blocks remain physically isolated.
- PhysicalCanvas becomes the authoritative geometry database.
- 3D View opens immediately with an interactive "Building Your Design" experience instead of freezing the UI.
- DRC becomes a verification pass rather than the primary collision detector.

---

# Long-Term Vision

The Physical Canvas should become the foundation for future capabilities including:

- Advanced DRC
- LVS
- RC extraction
- Timing analysis
- Analog layout
- Incremental rebuilds
- Interactive editing
- Multi-threaded routing
- Manufacturing export
- AI-assisted placement and routing

The long-term goal is for **Physical IR to always represent a legal design**, with DRC serving as an independent verification step rather than the mechanism that discovers routine placement and routing mistakes.
