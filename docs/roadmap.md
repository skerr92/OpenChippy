# OpenChippy Roadmap

> **Mission:**
> Build an open-source, educational-first silicon design studio that bridges the gap between transistor physics and manufacturable integrated circuits through an intuitive, unified design experience.

---

# Guiding Principles

OpenChippy is **not** intended to replace Cadence Virtuoso, Siemens Calibre, or Synopsys.

Instead, it aims to become the **Blender or KiCad of VLSI**, providing an integrated learning environment while remaining compatible with professional and open-source design flows.

Core principles:

* Education first
* Unified workflow
* Visual understanding
* Open architecture
* Extensible plugin system
* Compatible with existing EDA tooling
* Desktop-first with future WebAssembly support

---

# Long-Term Vision

OpenChippy should allow a designer to seamlessly navigate between every abstraction layer of chip design.

```text
Architecture
      ↓
RTL
      ↓
Logic Gates
      ↓
Standard Cells
      ↓
Individual Transistors
      ↓
Physical Layout
      ↓
3D Process Visualization
```

Every object should remain connected across these layers.

Clicking on a gate should allow the user to inspect:

* RTL implementation
* transistor implementation
* timing
* power
* routing
* layout
* waveform
* physical geometry

---

# Architecture

```
Frontend
├── Tauri Desktop
├── React
├── Three.js
└── Property Panels

Backend (Rust)
├── Project Model
├── Circuit Graph
├── Simulation Engine
├── Validation Engine
├── File Serialization
└── Plugin API

Future
├── WebAssembly
├── Mobile Viewer
└── Cloud Collaboration
```

---

# Milestone 0 — Project Foundation

## Goal

Create a stable desktop application architecture.

### Deliverables

* Tauri application
* Rust backend
* React frontend
* Three.js viewport
* Save / Load project
* Undo / Redo framework
* Plugin architecture skeleton

### Success Criteria

* Open application
* Place placeholder component
* Save project
* Reload project

---

# Milestone 1 — Transistor Schematic Editor

## Goal

Build a functional transistor editor capable of creating CMOS circuits.

### Components

* NMOS
* PMOS
* VDD
* GND
* Digital Input
* Output Probe
* Wire
* Junction
* Net Label

### Editor Features

* Grid snapping
* Orthogonal wire routing
* Rotate components
* Move components
* Delete
* Multi-select
* Undo / Redo
* Zoom / Pan

### Validation

Detect:

* Floating terminals
* Shorted power rails
* Duplicate names
* Disconnected nets
* Missing power

### Success Criteria

A user can build:

* CMOS inverter
* NAND gate

without editing any files manually.

---

# Milestone 2 — Switch-Level Simulation

## Goal

Simulate CMOS transistor networks using an educational switch model.

## Milestone 2.1 — Operational Model

Build and verify the headless Rust simulation core before adding visualization:

* Derive electrical nets from schematic connectivity and matching net labels
* Treat NMOS as conducting with a HIGH gate
* Treat PMOS as conducting with a LOW gate
* Resolve driven, floating, contended, and unknown net states
* Report output-probe and transistor switch states
* Verify inverter and NAND truth tables in automated tests

## Milestone 2.2 — Simulation Visualization

Add input controls and visualize conducting transistors, active paths, output
states, and problematic nets using the stable operational-model results.

## Milestone 2.3 — Truth Tables & Acceptance

Complete and exercise the initial switch-level feature set:

* Generate complete binary truth tables from the production Rust solver
* Display input/output combinations and convergence in the desktop UI
* Apply a truth-table row back to the live visualization
* Verify CMOS NOR behavior
* Verify transmission-gate pass and high-impedance behavior
* Bound truth-table enumeration to a practical number of inputs

## Milestone 2.4 — State Exploration & Diagnostics

Make non-binary simulation behavior understandable:

* Allow LOW, HIGH, and UNKNOWN input stimulus
* Summarize stable and non-converged simulation runs
* Explain floating, contended, and unknown output conditions
* Inspect resolved output, transistor, and net states
* Link simulation entries back to schematic components

## Milestone 2.5 — Waveform View

Add a GTKWave-inspired time-domain view:

* Open Waveforms from the Simulate menu or editor view switch
* Configure simulation length, clock period, and input-change interval
* Generate timed input stimulus in the Rust simulation backend
* Treat inputs named CLK or CLOCK as periodic clocks
* Drive remaining inputs as a deterministic binary sequence
* Plot input and output states with a shared nanosecond time axis
* Preserve HIGH, LOW, FLOATING, CONTENDED, and UNKNOWN states in traces

### Initial Model

NMOS

```
Gate HIGH
→ conducting
```

PMOS

```
Gate LOW
→ conducting
```

### Output States

* HIGH
* LOW
* FLOATING
* CONTENDED
* UNKNOWN

### Visualization

Highlight:

* conducting transistors
* active paths
* output nodes
* floating nets

### Success Criteria

Correct truth tables for:

* Inverter
* NAND
* NOR
* Transmission Gate

---

# Milestone 3 — Educational Device Models

Replace ideal switches with simplified transistor approximations.

## Milestone 3.1 — Technology Domain Model

Introduce a Rust-owned, serializable technology model with built-in educational
CMOS defaults:

* Supply voltage
* NMOS and PMOS threshold voltage
* Nominal on resistance
* Reference width and length
* Gate and diffusion capacitance coefficients

Acceptance:

* Invalid or non-physical values are rejected with useful diagnostics
* The built-in technology preserves all Milestone 2 truth tables
* Technology data remains independent from Tauri and UI code

## Milestone 3.2 — Technology Definition Files

Load and validate human-readable technology files based on the roadmap YAML
shape:

* Open a technology file for the current project
* Report malformed, incomplete, or unsupported definitions
* Fall back explicitly to the built-in educational technology
* Show the active technology name and core parameters
* Declare the process routing ceiling with `max_metal_layers`; the built-in
  educational technology provides five metal layers

Acceptance:

* Round-trip and fixture tests cover valid and invalid technology files
* Loading technology does not mutate schematic topology
* Physical routing and layer visualization never exceed the selected process limit

## Milestone 3.3 — Device Geometry

Persist transistor width and length and make them editable in the inspector:

* Backward-compatible defaults for existing `.chippy` projects
* Grid-independent numeric properties
* Derived effective resistance and capacitance
* NMOS and PMOS parameters remain technology-specific

Acceptance:

* Wider devices reduce effective on resistance
* Longer devices increase effective on resistance
* Geometry survives save/load and undo/redo
* Existing projects load with reference geometry

## Milestone 3.4 — Threshold & Drive Approximation

Use the technology and geometry model during switch evaluation:

* Map digital HIGH/LOW to supply and ground voltages
* Apply simplified NMOS and PMOS threshold checks
* Track effective conducting-path resistance
* Preserve FLOATING, CONTENDED, and UNKNOWN behavior

Acceptance:

* Existing inverter, NAND, NOR, and transmission-gate truth tables still pass
  at nominal voltage
* Insufficient gate overdrive prevents conduction
* Series devices have greater path resistance than parallel devices
* Results expose educational device/path details without claiming SPICE accuracy

## Milestone 3.5 — Capacitance & Propagation Delay

Estimate load and propagation delay using a documented first-order RC model:

* Gate and diffusion capacitance
* Output load from connected device terminals
* Approximate `0.69 × R × C` propagation delay
* Delayed transitions in waveform simulation
* Inspector readouts for estimated resistance, capacitance, and delay

Acceptance:

* Increasing width increases capacitance
* Increasing fanout increases estimated delay
* Stronger drive reduces delay for the same load
* Waveform output edges shift by the estimated delay
* Operating-point truth tables remain unchanged

## Milestone 3.6 — Compact Standard-Cell Physical Synthesis

Convert transistor schematics into tightly grouped, GDS-style standard-cell
layouts without treating schematic coordinates as physical placement:

* Build a Rust-owned physical-layout IR separate from the schematic model
* Recognize complementary pull-up and pull-down transistor networks
* Order PMOS and NMOS devices for diffusion sharing
* Place devices into compact rows with power rails, wells, taps, and pins
* Fold large or repeated networks into aspect-ratio-aware row banks instead of
  allowing transistor count to grow one unbounded horizontal strip
* Route logical nets across poly, contacts, vias, and multiple metal layers
* Enforce simplified technology spacing, width, and enclosure rules
* Derive both a layer-accurate 2D/GDS view and extruded 3D view from the same IR
* Keep a future GDSII writer behind the physical-layout boundary

Acceptance:

* Inverter, NAND, and NOR produce compact deterministic cell layouts
* Moving devices in the schematic does not directly move physical devices
* Generated layout connectivity is equivalent to schematic connectivity
* Shared diffusion reduces cell width where topology permits it
* Different valid schematic drawings of the same topology normalize to the same
  physical result
* Large repeated structures approach a compact rectangular footprint while
  preserving intentional linear datapaths where topology warrants them
* Generated shapes pass the simplified technology-rule checker
* 2D layer view and 3D extrusion represent identical geometry

This is intentionally a larger slice and should itself be staged as:

1. Connectivity-normalized physical IR
2. Transistor network recognition and ordering
3. Row placement and diffusion sharing
4. Pin assignment and multi-layer routing
5. Technology-rule checking and connectivity equivalence
6. Shared 2D/GDS and 3D rendering pipeline

## Milestone 3.8 — Scalable Design Navigation

Keep large schematics and generated physical cells practical to inspect:

* Expand schematic and physical zoom ranges based on design scale
* Preserve click/drag, trackpad, wheel, and keyboard panning
* Fit the complete schematic or physical bounds on demand
* Provide accelerated keyboard panning for large designs

Acceptance:

* A wide multi-bit schematic can be fit, zoomed out, and traversed without
  camera-range clipping
* The 3D top-fit action frames the complete generated physical bounds
* Fine zoom remains available for terminal and layer inspection

## Milestone 3.7 — Reusable Device Blocks

Turn a completed transistor-level circuit into a named hierarchical block that
can be saved and reused without expanding its internal schematic everywhere:

* Select or capture a circuit as a block definition
* Promote named inputs, outputs, VDD, and GND to an explicit pin interface
* Save block definitions inside the project and optionally as portable library
  files
* Place compact block instances with stable reference designators
* Compose existing blocks into higher-level blocks at arbitrary hierarchy depth
* Open an instance or definition to inspect and edit its transistor-level
  implementation
* Simulate and validate through hierarchy while retaining source-level
  diagnostics
* Prevent recursive definitions and incompatible pin/interface changes
* Preserve a path from logical block instances into Milestone 3.6 physical-cell
  synthesis

Acceptance:

* A transistor-level NAND can be saved, placed, wired, and simulated as a
  reusable NAND block
* Multiple instances share one definition without duplicating its internal
  schematic data
* A reusable block can contain other reusable blocks without losing simulation,
  DRC, waveform, truth-table, or physical-layout behavior
* Block definitions and instances survive save/load and undo/redo
* Editing a definition updates its instances while preserving compatible wiring
* Invalid recursion and broken pin mappings produce useful diagnostics
* Users can descend into a block and return to the parent schematic

This slice establishes logical hierarchy and reuse. Library versioning,
cross-project dependency resolution, and physical abstract/LEF generation can
be expanded in Milestone 4.

Current implementation:

* The current top-level transistor circuit can be captured as one shared
  definition with promoted input, output, VDD, and GND pins.
* Compact instances persist in `.chippy` projects, participate in undo/redo, and
  are mirrored as portable `.chippyblock` JSON inside a `chippyblocks/` folder
  beside the saved project. Opening a project discovers and deduplicates that
  adjacent library automatically.
* Simulation, truth tables, waveforms, DRC, and physical generation temporarily
  flatten arbitrary-depth instances with deterministic source identities while
  saved source data remains shared.
* A circuit containing existing block instances can be captured or used to
  update a compatible definition. Direct and indirect definition cycles, missing
  dependencies, and broken promoted-pin mappings are rejected.
* Multi-stage hierarchy settling distinguishes provisional floating internal
  gates from explicitly unknown stimuli, so cascaded reusable CMOS blocks
  converge without contaminating resolved power nets.
* Export writes the selected definition and its adjacent library dependencies so
  composite blocks can be reopened from the project-level `chippyblocks/` folder.
* Direct descend-and-edit navigation is represented by the shared-definition
  inspector and source recapture boundary; a dedicated nested canvas navigator
  remains a Milestone 4 library-workflow refinement.

Physical-preview routing currently reserves a signal channel between folded
PMOS and NMOS banks. Each logical signal receives a distinct track coordinate;
the selected process's upper metal carries horizontal trunks, while power,
PMOS-side, and NMOS-side drops use separated lower-layer roles with explicit
landing pads and adjacent-layer vias. This prevents via stacks from landing on
unrelated trunks and keeps long drops from crossing device banks or power rails.
Routing-lane pitch is derived from the landing dimension plus clearance rather
than transistor pitch, allowing via-only channel regions to compact without
reducing device-to-device spacing.

## Milestone 3 Non-Goals

* SPICE-compatible analog simulation
* Continuous nonlinear MOS equations
* Extracted interconnect parasitics
* Signoff timing, power, or physical verification
* Production-quality place-and-route or foundry-qualified GDS

### Initial Technology File Shape

```yaml
technology:
  format_version: 1
  name: OpenChippy EDU CMOS
  supply_voltage: 1.8
  max_metal_layers: 5

nmos:
  vt: 0.45
  ron: 12000
  reference_width: 1.0
  reference_length: 1.0
  gate_capacitance_ff_per_um: 2.0
  diffusion_capacitance_ff_per_um: 1.0

pmos:
  vt: -0.45
  ron: 22000
  reference_width: 1.0
  reference_length: 1.0
  gate_capacitance_ff_per_um: 2.2
  diffusion_capacitance_ff_per_um: 1.2
```

---

# Milestone 4 — Standard Cell Library

Users should be able to convert transistor networks into reusable cells.

Example:

```
INV_X1

NAND2_X1

NOR2_X1

MUX2_X1
```

Features:

* Cell hierarchy
* Expand / Collapse
* Pin mapping
* Library browser

Delivery inchstones:

1. **4.1 — Physical Rule-Deck Model (complete):** versioned Rust/YAML schema, educational
   defaults, validation, layer/via overrides, and backward-compatible projects.
2. **4.2 — Headless Physical DRC (complete foundation):** grid, legal-layer,
   width, spacing, area, cut, enclosure, well, and gate-extension checks against
   canonical shapes. Tap, rail, and pin-access checks follow their explicit
   structures in 4.3–4.6.
3. **4.3 — Physical Planning & Routing Resources (complete):** process-derived sites,
   tracks, preferred directions, layer capacities, obstacles, pin access,
   net classes, initial floorplan sizing, and bounded growth policy.
4. **4.4 — Global Placement & Legalization (complete):** deterministic placement
   candidates, topology/diffusion sharing, density and congestion feedback,
   legal sites, metrics, ranking, and selection rationale.
5. **4.5 — Negotiated Global Routing (complete):** coarse routing guides, global-net
   priority, capacity accounting, overflow analysis, bounded rip-up/reroute,
   and floorplan retry.
6. **4.6 — Detailed Routing & Compaction (complete foundation):** pin access and track assignment,
   initial track routing, DRC-driven search-and-repair, local rip-up/reroute,
   and safe post-route compaction.
7. **4.7 — 3D DRC Inspection (complete):** selectable violations, affected-shape
   highlighting, layer isolation, framing, explanations, and saved reports.
8. **4.7.1 — Physical DRC Closure (in progress):** coalesced conflict graphs,
   access-column repair, detailed-polygon connectivity equivalence, and
   DRC-clean rendered/GDS cutover for hierarchical multi-net designs.
9. **4.8 — Grouped Waveform Buses:** persisted ordered signal groups, binary/hex
   radix, crossed value transitions, and scalar expand/collapse.

Each inchstone must have independently testable fixtures and preserve all
accepted behavior from earlier milestones.

## Milestones 4.1–4.2 and 4.7 — Process-Aware Physical DRC

Extend the technology model into a versioned physical rule deck and validate the
same layout geometry consumed by the 2D/GDS and 3D views. This is a concrete
step toward generating signoff-ready designs, while foundry-qualified signoff
and external-tool correlation remain Milestone 9 responsibilities.

Initial rule families:

* Manufacturing grid and legal-layer checks
* Minimum and maximum width by diffusion, poly, contact, via, and metal layer
* Same-layer and selected cross-layer spacing
* Contact and via cut dimensions, arrays, pitch, and adjacent-metal enclosure
* Diffusion/poly overlap and gate-extension requirements
* Well width, spacing, enclosure, and device-to-well containment
* Metal area, notch, end-of-line, and minimum-area rules where the selected
  process declares them
* Power-rail, pin-access, and required tap checks

Architecture:

* Store units and rules in the selected process YAML instead of hard-coding
  educational values in the renderer
* Run physical DRC in Rust against canonical physical-layout shapes
* Give every violation a stable rule identifier, severity, affected shape IDs,
  measured value, required value, layer, and explanatory message
* Overlay violations directly in the 3D view with selectable markers and
  isolation of the offending layers/shapes
* Keep the DRC engine independent of Three.js so headless checks, saved reports,
  future GDS output, and CI use identical results

Acceptance:

* Deliberate width, spacing, via-size, via-enclosure, well, and off-grid
  violations are detected by fixture tests
* Clean inverter, NAND, NOR, and reusable-block cells pass the built-in
  educational process deck
* Selecting a DRC result frames and highlights the exact offending 3D geometry
* Rule results remain stable when the camera, layer visibility, or schematic
  drawing coordinates change
* Unsupported or incomplete process rules fail clearly instead of silently
  falling back to guessed manufacturing limits
* A machine-readable report can be compared later with Milestone 9
  foundry-qualified DRC results

Current 4.1 implementation:

* `Technology` embeds a versioned physical rule deck and older `.chippy`
  projects receive the educational defaults.
* The rule deck declares database units, manufacturing grid, base diffusion,
  poly, well, and metal width/spacing/area rules, contact/via
  size/spacing/enclosure, gate extension, and well enclosure.
* Named `metalN` and adjacent `viaNN` overrides support process-specific layers
  within the configured metal ceiling.
* Rust rejects unsupported versions, non-positive/off-grid values, invalid
  layer names, and non-adjacent or out-of-range via overrides.
* YAML round-trip and backward-compatibility tests cover the new schema; the
  inspector exposes the active rule-deck version, grid, contact size, and via
  size.

Current 4.2 implementation:

* A renderer-independent Rust engine validates canonical physical-layout
  rectangles and returns a machine-readable report through
  `validate_physical_layout`.
* Diagnostics contain a stable rule ID, severity, layer, deterministic shape
  indices, measured and required values, and an explanatory message.
* Implemented checks cover grid and legal layers; width, area, and same-layer
  spacing; contact/via size and adjacent-metal enclosure; device-side contact
  enclosure; P-diffusion containment in N-well; and poly gate extension.
* Generated rectangles are edge-snapped to the process grid. Gates and wells
  use active enclosure rules, and every via transition receives complete
  landing pads on both adjacent metals.
* Deliberate violations are fixture-tested, while generated inverter and NAND
  cells pass the educational deck. Explicit tap, rail, and pin-access
  structures will gain dedicated semantics when those canonical structures are
  introduced rather than being guessed from unrelated rectangles.

## Milestones 4.3–4.6 — Negotiated Cell Placement & Routing

OpenChippy should follow the staged structure of contemporary physical-design
flows while adapting it to transistor-level educational cells. It should not
copy a chip-scale analytic placer blindly, but it must preserve the same
separation of concerns:

1. Build process-owned placement sites, routing tracks, layer directions,
   capacities, obstacles, via transitions, and pin-access points.
2. Estimate a floorplan and place devices without committing to exact wires.
3. Legalize devices to valid sites while preserving spacing and well regions.
4. Globally route nets on a coarse capacity graph to expose congestion before
   detailed geometry exists.
5. Assign tracks and produce detailed routes.
6. Search and repair remaining DRC violations, ripping up only the conflicting
   routes when possible.
7. Compact only after a legal route proves the reserved space is unnecessary.

### 4.3 — Physical planning and tangible space budgets

The process YAML must describe routing pitch/offset, preferred direction,
usable signal layers, reserved power layers, via transitions, placement sites,
and per-layer capacity reductions. Planning derives space from demand:

* Device rows use legal diffusion/poly geometry plus device spacing, well
  enclosure, contacts, and tap/endcap reservations.
* Routing bins count available tracks after obstacles, power reservations, via
  keep-outs, and a configurable capacity margin.
* Estimated net demand uses pin count, bounding boxes, and rectilinear
  Steiner-tree length rather than one full-width track per net.
* Initial core area is the greater of device-area/density demand and estimated
  routing demand. Educational defaults target 60–70% device density and no
  more than 75% estimated routing utilization; both are process configurable.
* Try a small deterministic set of aspect ratios appropriate to the topology,
  such as `1:1`, `4:3`, and a topology-derived linear option.
* If global routing still overflows, grow the congested dimension by 5–10% and
  retry, up to three floorplan-growth passes by default. Report failure rather
  than silently emitting an illegal layout after the budget is exhausted.

These are explicit effort defaults, not manufacturing truths. The selected
process can override them, and every run records actual density, capacity,
demand, overflow, and growth decisions.

Current 4.3 implementation:

* Technology snapshots and YAML now carry validated placement-site dimensions,
  row height, density/utilization targets, floorplan-growth policy, routing and
  repair effort limits, and per-metal pitch, offset, preferred direction,
  capacity adjustment, and power reservation.
* Older technologies resolve a compatible educational resource model
  dynamically up to their configured metal-layer ceiling.
* Rust classifies nets as power, global, or signal and emits a stable routing
  order. VDD/GND are reserved first; external and high-fanout nets precede
  ordinary signals.
* The planner produces square, balanced, and topology-derived candidates,
  estimates device density and routing demand/capacity, applies bounded
  directional growth, and selects the lowest-area feasible score.
* The selected candidate owns a coarse routing grid with deterministic bins and
  per-layer horizontal/vertical track capacities. This is the graph boundary
  consumed by Milestone 4.5.
* Physical IR exposes every candidate, selected rationale inputs, resource
  limits, planned nets, and bins. The 3D sidebar reports selected dimensions,
  density, routing utilization, candidate/growth counts, and bin dimensions.
* The existing preview now takes its columns and bounds from the selected plan;
  Milestone 4.4 will replace its provisional device ordering with legal
  congestion-aware placement.

### 4.4 — Global placement and legalization

Replace the single physical-layout heuristic with two or three deterministic
candidate runs for each cell or repeated block region:

1. A topology-following candidate favoring diffusion sharing and short local
   connections
2. A compact folded candidate favoring minimum legal area
3. When the design size warrants it, a pin- and congestion-aware candidate
   favoring routability and balanced dimensions

Placement uses net weights and placement constraints before routing:

* Fix or reserve power rails, well regions, external pins, taps, and other
  physical anchors first.
* Give global/high-fanout nets and constrained pins more placement influence,
  but do not let one high-fanout net collapse unrelated devices into a hotspot.
* Favor complementary PMOS/NMOS alignment, diffusion sharing, short gate
  connections, and local series/parallel clusters.
* Estimate congestion after each placement pass. Inflate or spread devices in
  congested bins and retain the best non-divergent snapshot.
* Legalize to process sites/rows with routing padding, then recalculate
  congestion because legalization changes pin positions.

Current 4.4 implementation:

* Rust emits three deterministic device-placement candidates: topology-aware,
  diffusion-oriented, and congestion-spreading.
* Topology placement greedily clusters devices sharing non-power nets and
  assigns extra affinity to shared source/drain nets. Diffusion placement uses
  stable source/drain and gate signatures; congestion placement alternates
  high-connectivity devices across the available order.
* Each polarity is folded into serpentine rows and legalized to the 4.3
  placement sites, row height, planned dimensions, diffusion width, and
  process spacing. Duplicate kind/row/site occupancy makes a candidate
  ineligible.
* Candidate metrics include estimated half-perimeter wire length, peak demand
  against the 4.3 coarse-bin capacities, and adjacent diffusion-sharing pairs.
  Illegal candidates lose lexicographically; legal candidates minimize
  wire/congestion cost while rewarding diffusion sharing.
* Physical IR persists every placement candidate and selected index. Canonical
  diffusion, poly, contact, and routing anchors now consume the selected legal
  device coordinates instead of recreating an implicit name-sorted grid.
* The 3D sidebar reports the winning strategy, legality, HPWL estimate, peak
  coarse-bin utilization, and diffusion-sharing count.

### 4.5 — Negotiated global routing

Global routing produces layer-aware coarse guides, not final polygons:

* Reserve power/ground resources first. Route clocks or future clock-like
  global nets next, then constrained/high-fanout nets, then ordinary signals.
* Build rectilinear tree candidates for multi-terminal nets and charge every
  traversed bin edge and via transition against capacity.
* After an initial route, identify overflow edges and rip up nets contributing
  most to those conflicts.
* Reroute with negotiated costs: current congestion, accumulated historical
  congestion, wire length, vias, wrong-way travel, pin-access scarcity, and net
  priority.
* Preserve clean routes where possible. Do not clear and rebuild every net
  after each conflict.
* Default to at most 30 global-routing iterations. Stop early when overflow
  reaches zero or when the best score fails to improve for three iterations.
* If overflow remains, return to placement or grow only the congested
  floorplan dimension within the 4.3 pass budget.

Current 4.5 implementation:

* Rust maps selected 4.4 device locations and external pins into the selected
  4.3 coarse bins, then connects multi-terminal nets with deterministic
  Manhattan guide trees.
* Nets route in stable power, global/high-fanout, then signal order. Power nets
  prefer process-reserved layers; ordinary nets prefer signal resources.
* Every adjacent-bin guide edge chooses a physical metal and charges the
  directional capacity declared for both bins. Layer changes are counted as
  estimated vias.
* Each negotiation pass identifies over-capacity edges and the exact nets using
  them. Only contributing nets are ripped up; clean routes remain fixed.
* Rerouting costs include current utilization, accumulated historical overflow,
  wrong-way or unavailable capacity, and a deterministic layer tie-breaker.
  Alternate Manhattan orientation provides a second path family without
  introducing randomness.
* The router stops at zero overflow, the configured non-improvement limit, or
  the maximum iteration budget. It retains the lowest-overflow snapshot and
  reports explicit bounded failure when capacity cannot converge.
* Physical IR exposes per-net segments/layers, estimated length, via and rip-up
  counts, plus iteration overflow/reroute history. The 3D sidebar reports
  convergence, overflow, routed-net count, and negotiation passes. Exact
  process-grid polygons remain the Milestone 4.6 detailed-router boundary.

### 4.6 — Detailed routing, search-and-repair, and compaction

Detailed routing converts guides into exact process-grid polygons:

* Analyze pin access before committing routes and require at least one legal
  access point for every terminal.
* Assign preferred-direction tracks, then run an initial detailed route within
  each global guide.
* Run physical DRC and build a conflict graph from shape-indexed violations.
* Select a bounded conflict set, rip up its lowest-priority or highest-cost
  routes, and maze-route alternatives with history costs.
* Repair locally first; expand the search window or change layers only when a
  local repair cannot succeed.
* Default to ten detailed search-and-repair rounds. Preserve the best legal or
  lowest-violation snapshot so cancellation never discards all progress.
* Compact device banks and channels only after rerunning pin access, routing,
  connectivity equivalence, and physical DRC.

Current 4.6 foundation:

* A dedicated Rust detailed-routing stage consumes the selected global guides
  and assigns deterministic preferred-direction tracks at process-declared
  pitch. Multiple users of a guide edge receive distinct centered track slots.
* Guide edges become exact manufacturing-grid metal rectangles. Layer changes
  produce explicit adjacent-layer via polygons, and same-bin local connections
  remain valid pin-access cases without wasting a global track.
* Every detailed net reports its exact polygons, accessible and blocked pin
  counts, wire length, via count, priority, and repair count. The aggregate
  report exposes convergence, conflicts, blocked pins, bounded iterations,
  total wire length, and total vias in the physical IR and 3D sidebar.
* Cross-net same-layer polygon spacing builds the initial conflict set. Repair
  keeps unaffected routes stable, shifts only the deterministic lower-priority
  conflict owners, reruns conflict detection, and preserves the best snapshot
  within the process-defined ten-pass default.
* The NAND regression begins with detailed conflicts and converges after local
  repair; it also verifies polygon edges against the manufacturing grid,
  bounded iteration accounting, net identity, pin access, and wire metrics.
* Detailed polygons are retained beside the existing DRC-clean preview shapes
  until connectivity-equivalence coverage can make them the sole rendered/GDS
  geometry. This avoids presenting an incomplete pin-access migration as a
  clean layout; that cutover and post-route bank compaction remain the next
  refinement inside the physical pipeline.

The current Milestone 3 preview deliberately reserves one unique central track
coordinate per signal to avoid accidental cross-net via and metal collisions.
On high-net-count designs this can produce a visibly sparse routing canyon:
PMOS and NMOS banks are pushed to opposite cell edges while most of the space
between them contains only long interconnect and via transitions. Milestone 4
must replace that safety-first allocation with legal, utilization-aware routing:

* Reuse track coordinates on electrically isolated metal layers when via stacks
  and enclosures cannot collide
* Split one global channel into multiple local channels or routing regions near
  the devices and pins they serve
* Move transistor banks closer after detailed routing proves that fewer tracks
  are required
* Prefer short local routes and shared access points over full-cell vertical
  drops
* Measure channel density, whitespace, and via-only area explicitly
* Reject compaction that introduces shorts, blocked vias, inaccessible pins, or
  process-spacing violations

Candidate selection is lexicographic: correctness first, then routability, then
quality. A smaller illegal cell never beats a larger legal one:

1. Connectivity equivalence and zero shorts/opens
2. Zero illegal placement and physical DRC errors
3. Zero global/detailed routing overflow
4. Minimum area and unused channel area
5. Weighted wire length, vias, and pin-access risk
6. Aspect-ratio preference only as a final bounded tie-breaker

Record the following metrics rather than assuming a square is always optimal:

* Bounding-box area and unused channel area
* Routing-channel utilization and via-only whitespace
* Routing overflow, congestion, and blocked pin access
* Total estimated wire length
* Via and layer-transition count
* Diffusion sharing and device-row utilization
* Aspect-ratio penalty only outside configurable practical bounds

The selector should minimize legal area and routing cost first. Squareness is a
soft tie-breaker that prevents pathological strips when two candidates are
otherwise comparable; naturally linear cells and datapaths may remain linear
when that produces the better result.

Acceptance:

* The Rust backend emits two or three reproducible candidates and a score
  breakdown for each
* Every run exposes stage, pass, elapsed time, best score, density, congestion,
  overflow, DRC count, and floorplan growth so the UI can show determinate
  progress, cancellation, and a best-so-far preview
* The selected candidate is connectivity-equivalent, DRC-clean, and has zero
  routing overflow; failure to meet a bounded search budget is explicit
* NAND, NOR, mux, full-adder, and repeated full-adder fixtures demonstrate that
  different topologies can select different aspect ratios
* Turning off metal visibility no longer reveals a large avoidable empty canyon
  between device banks in high-net-count fixtures
* Compaction reduces bank separation and/or unused channel area without adding
  any cross-net overlap or DRC violation
* Candidate selection does not depend on schematic drawing coordinates
* The 3D inspector can report why the winning candidate was selected

## Milestone 4.7 — 3D DRC Inspection

Consume the stable headless DRC report after the negotiated router exists:

* List and filter violations by rule, severity, layer, and net
* Select a diagnostic to frame and highlight its exact shapes
* Isolate contributing layers and show measured versus required geometry
* Show the placement/routing pass that introduced a violation
* Save the report with the selected candidate and generation metrics

Current implementation:

* Entering 3D view generates the physical IR and headless Rust DRC report from
  the same project snapshot, preserving the report's stable shape indices.
* The scrollable physical sidebar reports clean/error/warning totals and lists
  diagnostics with rule ID, layer, message, severity, and measured-versus-
  required geometry. Filters cover rule, severity, layer, and affected net.
* Selecting a diagnostic isolates its contributing layer, renders every exact
  offending shape with a red emissive material, and frames that geometry with
  the orthographic camera. Normal layer visibility and Top view / Fit remain
  available to restore broader context.
* Save DRC report writes a versioned JSON artifact containing the project and
  technology identities, physical-IR version, selected planning and placement
  candidates, global and detailed routing metrics, and the complete stable
  diagnostic report.
* Clean generated inverter/NAND coverage remains green, headless violating
  fixtures retain deterministic shape IDs and measurements, and artifact
  persistence is verified byte-for-byte in Rust.

### 4.7.1 — Physical DRC closure

Large hierarchical circuits exposed a gap hidden by the clean inverter, NAND,
and same-net array fixtures: independently folded device rows can reuse the
same X access column for unrelated long vertical drops. The result is real
cross-net spacing conflicts on intermediate metals and via landings. The old
pairwise DRC presentation then multiplied one physical conflict across every
overlapping polygon fragment, producing four-digit result lists.

This closure pass precedes waveform buses because 4.8 does not alter physical
IR or layout geometry:

* Coalesce same-rule, same-layer, same-net-pair spacing fragments into one
  deterministic conflict diagnostic while retaining every affected shape
  index and the minimum measured spacing.
* Use the resulting conflict graph to allocate or repair access columns and
  vertical tracks across legal layers without introducing long Metal-1 jogs.
* Add multi-row, many-independent-net fixtures representative of flattened
  reusable-block designs; do not accept same-net-only stress tests as routing
  closure.
* Promote 4.6 detailed polygons to rendered/GDS geometry only after terminal
  connectivity equivalence, zero opens/shorts, and physical DRC pass together.

Current first slice:

* Rust DRC now collapses pairwise metal-spacing fragments by layer and
  electrical net pair, unions and sorts all contributing shape indices, keeps
  the worst measured clearance, and recomputes stable report totals.
* A 32-independent-inverter/64-MOS fixture reproduces the folded multi-net
  access-column failure and verifies that hundreds of raw fragment pairs become
  fewer than 100 actionable conflicts with multi-shape ownership. Geometry
  repair remains required; coalescing does not relabel a real conflict clean.
* A first attempt to distribute nets by ID across preferred M2/M4 vertical
  resources passed the generic fixture but regressed `4B_ADDER` from 496 to
  520 diagnostics. That heuristic was rejected and reverted. Future repair
  must consume the actual conflict graph and cannot select layers from net ID
  or synthetic-fixture score alone.
* The attached `4B_ADDER` report proved the pipeline was committing to its
  planning-stage square winner before routing: 32×34 produced 253 global
  overflow, 203 detailed conflicts, and 496 coalesced DRC errors. Routing all
  three bounded candidates showed 38×30.6 at 182/175 and topology-shaped
  72×23.8 at 82/45. Physical normalization now ranks candidates
  lexicographically by global overflow, detailed conflicts, then placement
  score, selecting the 72×23.8 result and reducing actual `4B_ADDER` DRC to
  189 (22 M2 and 167 M3 spacing conflicts).
* Saved DRC artifacts now include the complete canonical physical IR in
  addition to the summarized selected candidate, so offline shape indices can
  be resolved back to layers, nets, bounds, and exact polygon geometry.
* The complete follow-up artifact showed that directly substituting detailed
  guide polygons would create illegal terminal access and regress the exact
  design to 440 errors. The accepted repair instead scores legal connected
  drop/via candidates against the emitted conflict graph, runs original,
  reversed, and high-fanout-first net orders, and retains the lowest-conflict
  physical geometry. `4B_ADDER` now falls from 189 to 132 errors (22 M2, 97
  M3, and 13 M4) without regressing clean inverter/NAND fixtures. Detailed
  polygon cutover still requires a router-owned legal pin-access stage.
* Cut spacing now follows manufacturing semantics: same-net continuous metal
  may merge without a spacing error, but contacts and vias remain discrete
  cuts and must satisfy the process cut pitch even on the same net. Exact
  duplicate generated vias are collapsed before DRC, and violations use the
  distinct `CUT.MIN_SPACING` rule so they cannot be confused with metal
  spacing. The exact `4B_ADDER` result remains 132 after this correction.

## Milestone 4.8 — Grouped Waveform Buses

Let users combine related scalar inputs, outputs, or internal signals into a
named waveform bus similar to GTKWave and QuestaSim:

* Select two or more signals and define their explicit MSB-to-LSB order
* Name, reorder, expand, collapse, edit, and remove saved groups
* Choose binary or hexadecimal radix independently for each group
* Display the grouped value in the sticky signal-identification block at the
  active time cursor
* Draw the value inside each stable waveform interval when space permits
* Draw crossed `X` transition edges between intervals whenever the aggregate
  value changes, rather than representing a bus as a scalar HIGH/LOW line
* Resolve all member transitions at the same timestamp before computing the
  next displayed bus value
* Represent any interval containing `UNKNOWN`, `CONTENDED`, or unresolved
  `FLOATING` bits with `X` digits instead of a misleading numeric value
* Preserve access to the individual scalar lanes by expanding the group

Binary values should preserve leading zeroes to the configured bus width.
Hexadecimal values should use enough digits for that width and clearly retain
unknown nibbles, for example `0x3X`.

Acceptance:

* Users can group signals such as `SUM[7:0]`, select binary or hexadecimal
  radix, and see the same value in the lane label and waveform interval
* The time cursor updates the grouped label value using the exact same bit
  ordering as the rendered trace
* Every aggregate value change produces the conventional crossed `X` boundary
  shape, including simultaneous multi-bit changes
* Unknown, contended, and floating member states cannot be displayed as a valid
  numeric value
* Group definitions, ordering, radix, and collapsed state survive project
  save/load
* Expanding a group shows member traces without changing simulation data

Current implementation:

* Rust persists validated, undoable waveform groups in the project with unique
  membership, explicit signal order, radix, and collapsed state. Input/output
  renames follow group references and deleting members removes invalid groups.
* The waveform sidebar creates, renames, reorders, edits, expands/collapses, and
  removes groups. Scalar signals remain available below expanded buses.
* Aggregate lanes preserve leading zeroes, propagate unresolved bits as `X`,
  show the same value at the active cursor and sticky viewport edge, label
  stable intervals when space permits, and draw crossed transition boundaries
  after all same-time member changes are resolved.
* Fit/zoom buttons and Ctrl/Command-wheel scale the horizontal time axis from
  1× to 32× while retaining a centered viewport and denser time ticks. Stable
  aggregate intervals reveal their binary/hex values as zoom creates enough
  room, matching conventional waveform-inspection behavior.
* Aggregate rendering coalesces adjacent member-event slices when the formatted
  bus value did not change. Input buses therefore show labels across their true
  stable interval instead of losing text to numerous no-op sample boundaries.

---

# Milestone 5 — Truth Tables & Timing

Automatically generate:

* Truth tables
* Timing diagrams
* Switching activity
* Fanout analysis

Future additions:

* Estimated propagation delay
* RC delay
* Dynamic power estimation

## Milestone 5.1 — Timing-Aware Physical Candidate Selection

Extend the Milestone 4 multi-pass placer/router with simulation-derived timing,
fanout, and switching information:

* Estimate routed-net capacitance from physical length, layer, and via count
* Identify critical inputs, outputs, and internal paths
* Add timing-weighted placement and routing as one of the two or three candidate
  strategies
* Penalize excessive detours, high-fanout congestion, and avoidable layer
  transitions on critical paths
* Preserve compact-area candidates for designs whose timing already meets the
  selected target

Candidate scoring becomes a constrained tradeoff:

1. Reject DRC or connectivity failures
2. Meet the configured timing target when feasible
3. Minimize area, congestion, wire length, and via count
4. Prefer a practical aspect ratio only when the earlier costs are comparable

Acceptance:

* Candidate metrics expose area, dimensions, congestion, wire length, vias,
  estimated worst delay, and critical path
* A timing-critical fixture may legally choose a less-square or slightly larger
  layout when it produces a meaningful delay improvement
* A non-critical repeated design selects the smallest well-routed candidate
  rather than expanding merely to become square
* Selection remains deterministic for identical project, technology, and timing
  constraints

---

# Milestone 6 — RTL Integration

Introduce HDL support.

Initial goals:

* Verilog import
* Verilog export
* Gate-level viewer

Future:

* VHDL
* SystemVerilog subset

---

# Milestone 7 — Synthesis Integration

Rather than reimplement synthesis, integrate existing tools.

Initial adapters:

* Yosys
* ABC

Display:

* Synthesized netlist
* Cell usage
* Logic depth
* Fanout

---

# Milestone 8 — Physical Layout

Introduce layout editing.

Support:

* Diffusion
* Poly
* Metal
* Contacts
* Vias

Features:

* Layer visibility
* Measurement tools
* Hierarchy
* Grid snapping

---

# Milestone 9 — Physical Verification

Integrate:

* DRC
* LVS

Educational diagnostics should explain:

* what failed
* why
* possible fixes

---

# Milestone 10 — Placement & Routing

Integrate:

* OpenROAD
* OpenLane

Visualize:

* Standard cell placement
* Congestion
* Routing
* Critical paths

---

# Milestone 11 — 3D Chip Visualization

Render semiconductor structures.

Display:

* Wells
* Diffusion
* Poly
* Metal
* Dielectric
* Contacts
* Vias

Users should be able to rotate and inspect an IC in three dimensions.

## Milestone 11.1 — Live Signal-Path Visualization

Project switch-simulation state onto the routed physical geometry so users can
see which transistor and interconnect paths are electrically active:

* Preserve a stable logical-net identity from simulation through physical
  routing shapes, contacts, vias, and every metal segment
* Highlight active HIGH and LOW paths using the existing simulation palette
* Distinguish floating, contended, and unknown nets without implying current
  flow where none is known
* Illuminate conducting NMOS/PMOS channels and the connected path back to VDD
  or GND
* Support static operating-point inspection and waveform-time scrubbing
* Allow highlighting one net or one source-to-output path while dimming
  unrelated geometry
* Keep normal process-layer colors available as a toggle so electrical state
  never obscures layer identity

Acceptance:

* Inverter, NAND, NOR, and nested-block examples highlight the same logical
  state in the schematic, waveform, and 3D physical views
* Highlighting follows routed geometry across metal-layer changes and vias
* Contended and unknown paths remain visually distinct from valid HIGH/LOW
  conduction
* Moving the waveform cursor updates the physical state without regenerating
  layout geometry
* Every highlighted shape can be traced back to a physical net and simulation
  net identifier

This belongs after physical connectivity equivalence and routed-shape identity
are established. Implementing it earlier would risk displaying plausible but
electrically incorrect paths.

---

# Milestone 12 — Tiny Tapeout Integration

Support direct Tiny Tapeout workflows.

Project template:

* Pin constraints
* Tile limits
* Required ports
* Wrapper generation

Generate:

* Repository structure
* Testbench
* Configuration
* Documentation

Eventually:

One-click Tiny Tapeout project export.

---

# Milestone 13 — Plugin SDK

Expose APIs for:

* Simulators
* Importers
* Exporters
* PDKs
* Analysis tools
* Visualization modules

---

# Milestone 14 — Analog Simulation

Introduce SPICE-style analysis.

Support:

* DC Operating Point
* Transient Analysis
* AC Analysis

Likely through integration with an existing simulator before developing a native implementation.

---

# Milestone 15 — Professional Design Flow

Support complete ASIC development.

Examples:

```
RTL
 ↓
Synthesis
 ↓
Placement
 ↓
Routing
 ↓
DRC
 ↓
LVS
 ↓
Timing
 ↓
GDS
```

OpenChippy should orchestrate these tools while presenting them through a unified interface.

---

# Future Vision

Eventually OpenChippy should become a complete silicon engineering environment.

Possible modules include:

* CPU Architecture Designer
* ISA Designer
* RTL Simulator
* Standard Cell Editor
* Analog Designer
* Layout Editor
* Timing Analyzer
* Power Analyzer
* 3D Chip Viewer
* Package Viewer
* PCB Integration
* FPGA Flow
* Tiny Tapeout Export
* PDK Manager
* AI Engineering Assistant

---

# Non-Goals (for now)

OpenChippy is **not** initially attempting to:

* Replace Cadence Virtuoso
* Replace Siemens Calibre
* Replace Synopsys
* Develop a new synthesis engine
* Develop a new place-and-route engine
* Replace ngspice
* Replace OpenROAD

Instead, OpenChippy will focus on creating a unified, intuitive experience that integrates proven open-source tools while providing a significantly better educational and visualization experience.

---

# Guiding Philosophy

> "Chip design shouldn't feel like stitching together a dozen unrelated command-line tools."

OpenChippy exists to make silicon design approachable without sacrificing the ability to grow into professional workflows.

A beginner should be able to start by building a CMOS inverter from two transistors and, over time, follow that same design through synthesis, physical implementation, verification, and fabrication—all without leaving a single cohesive design environment.
