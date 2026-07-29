# OpenChippy Routing Philosophy

## Scope

OpenChippy should follow modern digital place-and-route stage boundaries while
remaining honest about what can be proven from an educational or compatibility
technology deck.

There is no single IEEE standard that dictates an integrated circuit's metal
widths, number of power rails, via arrays, or routing algorithm.

- [IEEE 1801-2024](https://standards.ieee.org/ieee/1801/7466/) defines a
  portable representation of power intent: supplies, voltage domains, power
  states, isolation, level shifting, retention, and related implementation
  intent.
- [IEEE 2416](https://standards.ieee.org/ieee/2416/12645/) defines the content
  of parameterized power models for system-level analysis.
- The selected process design manual supplies physical reliability limits.
  For GF180MCU, this includes current density and per-contact/via current limits
  intended to meet a stated electromigration lifetime target.
- LEF/DEF is the accepted interchange contract for place-and-route tools,
  maintained by Cadence and distributed by
  [Si2](https://si2.org/lef-def-downloads/).

Therefore OpenChippy should say that it supports IEEE-compatible power intent
when it implements IEEE 1801 semantics. It should only claim a physical power
network meets a process when that exact network passes the process deck and
reliability analysis. It should not describe ordinary geometric routing as
"IEEE compliant."

## Primary-source baseline

### Process reliability owns physical power constraints

The
[GF180MCU electromigration rules](https://gf180mcu-pdk.readthedocs.io/en/latest/physical_verification/design_manual/drm_14_2.html)
specify:

- allowable current density per drawn metal width;
- allowable current per contact and via;
- different unidirectional and bidirectional limits;
- temperature-dependent limits; and
- an explicit reliability target.

The router consequently needs current demand and temperature/corner inputs,
not a hard-coded stripe count. Required width and via multiplicity must be
derived from the worst current carried by each network segment.

The GF180
[design-for-manufacturing guidance](https://gf180mcu-pdk.readthedocs.io/en/latest/physical_verification/design_manual/drm_05_2.html)
also prefers extra width/spacing when room exists, multiple contacts/vias, and
effective substrate taps. Minimum-rule geometry is legal, but should not be the
quality target.

Very wide power conductors introduce their own requirements. GF180's
[metal slotting rules](https://gf180mcu-pdk.readthedocs.io/en/latest/physical_verification/design_manual/drm_14_6_3.html)
set a maximum unslotted width and define slot geometry. Power widening must
therefore be bounded and capable of producing process-owned slot markers.

### Production routing is staged

OpenROAD is a useful open implementation reference:

1. Legalize placed cells on process sites and leave routing padding where
   required.
2. Create the power distribution network from policy: voltage domains, rings,
   straps, follow-pin rails, layer choices, width, pitch, spacing, and via
   connections.
3. Globally route signals using congestion capacity and iterative negotiation.
4. Perform pin access, track assignment, and detailed maze routing.
5. Repair antenna and PDN-via violations.
6. Extract parasitics and analyze timing, IR drop, and electromigration.
7. Finish with fill and external physical verification.

References:

- [OpenROAD flow stages](https://openroad.readthedocs.io/en/latest/main/README2.html)
- [OpenROAD PDN generator](https://openroad.readthedocs.io/en/latest/main/src/pdn/README.html)
- [OpenROAD global routing](https://openroad.readthedocs.io/en/latest/main/src/grt/README.html)
- [OpenROAD detailed routing and pin access](https://openroad.readthedocs.io/en/latest/main/src/drt/README.html)
- [OpenROAD pin placement](https://openroad.readthedocs.io/en/latest/main/src/ppl/README.html)

The OpenROAD detailed router documentation points to the primary algorithm
papers for TritonRoute and its pin-access oracle. OpenChippy should use the same
separation of concerns even when its implementation is simpler.

## Required OpenChippy data model

### Power intent

Add a first-class, versioned power-intent model:

- named supply nets and ground nets;
- voltage domains and nominal voltages;
- primary and secondary supplies;
- always-on and switchable domains;
- legal power states;
- isolation, retention, and level-shifter requirements;
- pad, bump, or package supply entry points; and
- operating activity, current estimate, temperature, and reliability corner.

This model should be designed so a future IEEE 1801 importer/exporter can map
onto it without changing Physical IR.

### Technology reliability

Extend technology YAML with process-owned values:

- metal current-density limits by layer, directionality, and temperature;
- contact/via current limits and allowed via arrays;
- sheet resistance and via resistance;
- routing width/spacing tables and non-default rules;
- maximum unslotted width and slotting rules;
- antenna rules and available repair cells;
- tap, latch-up, well, and guard-ring requirements;
- density and fill windows; and
- supported routing and power layers.

Missing reliability data must produce `UNCHARACTERIZED`, not a passing result.

### Routed network

Every accepted route segment should retain:

- net and voltage-domain ownership;
- endpoints and parent route tree;
- layer, width, length, and direction;
- estimated current;
- resistance and voltage drop;
- via-stack identity and cut count;
- capacity demand;
- route-stage provenance; and
- DRC, antenna, EM, and IR-drop status.

Power geometry and signal geometry should share the same occupancy database but
remain distinct route classes.

## Routing objectives

Candidate selection is lexicographic:

1. logical and extracted connectivity equivalence;
2. no shorts or opens;
3. process DRC legality;
4. power-domain correctness;
5. electromigration and via-current legality;
6. IR-drop target;
7. antenna legality;
8. zero routing overflow;
9. timing constraints;
10. manufacturability margin;
11. area, wire length, via count, and aspect ratio.

A smaller design must never beat a connected, reliable design merely because
its geometry is compact.

## Power distribution algorithm

1. Estimate current per standard-cell instance and voltage domain.
2. Cluster current sinks spatially after standard-cell placement.
3. Choose a bounded set of candidate topologies: straps, mesh, rings where
   applicable, and local follow-pin rails.
4. Size each segment from accumulated downstream current, the process
   current-density limit, temperature, and a configurable margin.
5. Size via arrays from current per cut; add redundant cuts when space permits.
6. Connect each standard-cell power pin to the nearest legal local rail.
7. Solve the resistive network for static IR drop.
8. Widen, add straps, add vias, or move entry points at failing nodes.
9. Reserve the accepted PDN as fixed geometry before signal detailed routing.
10. Repair PDN vias after detailed routing and re-run DRC/EM/IR analysis.

Rail count is an outcome of current demand, span, resistance, congestion,
process rules, and available layers. It is never globally fixed at two.

## Signal routing algorithm

1. Route explicit standard-cell boundary pins, not flattened internal
   transistor terminals.
2. Generate multiple legal pin-access points per pin where geometry permits.
3. Build rectilinear Steiner candidates during global routing.
4. Charge process-layer capacity and reserve power resources.
5. Route critical clocks and timing-sensitive nets with explicit policy,
   followed by ordinary signals.
6. Negotiate congestion through bounded rip-up and reroute with historical
   costs.
7. Assign tracks and vias through atomic geometry transactions.
8. Use non-default width/spacing rules for clocks, high-current nets, or
   sensitive routes.
9. Repair antenna violations with layer hopping, gate splitting, or declared
   antenna cells.
10. Extract the final routed network and re-check timing, connectivity, DRC,
    antenna, and power integrity.

## Measurable implementation slices

### RP-1 — Schemas and provenance

- Add power-intent and reliability schemas.
- Encode the public GF180 EM/current limits with source URLs and revision.
- Mark EDU values as educational assumptions.
- Reject unsupported or internally inconsistent reliability tables.

### RP-2 — Current-aware PDN synthesis

- Replace rail-count heuristics with current accumulation.
- Generate at least strap and mesh candidates.
- Derive width and via-array count from process limits.
- Report segment current, utilization, and margin.

### RP-3 — IR-drop and electromigration analysis

- Solve the DC resistive network.
- Report worst drop and overloaded segment/cut.
- Iteratively repair failing PDN candidates.
- Preserve `UNCHARACTERIZED` when source data is incomplete.

### RP-4 — Standard-cell pin access and signal global routing

- Clone validated cell geometry.
- Expose multiple legal boundary access points.
- Route inter-cell nets with Steiner candidates and negotiated congestion.
- Keep cell-local geometry immutable.
- Close residual open islands with a multilayer A*/Dijkstra track graph. States
  are legal `(track-x, track-y, layer)` access points; planar edges carry
  length/congestion cost and vertical edges carry via, layer, and reliability
  cost. Use Manhattan distance as the admissible heuristic. Search every
  process-permitted layer and terminate only when the best complete route is
  proven or the reachable graph is exhausted.
- Do not substitute fixed dogleg counts or wall-clock timeouts for graph
  exhaustion. Safe dominance pruning is allowed when a candidate has the same
  endpoints/layer/via cost and no shorter remaining path can beat it.

### RP-5 — Detailed routing and manufacturing repair

- Track assignment and maze repair.
- Antenna checks and repair.
- Redundant-via optimization.
- Power-via repair after signal routing.
- Density, fill, slotting, and external DRC/LVS correlation.

## Claim policy

OpenChippy may report:

- `LOGICALLY_CONNECTED`
- `NATIVE_DRC_CLEAN`
- `PROCESS_DECK_DRC_CLEAN`
- `NATIVE_LVS_MATCHED`
- `PROCESS_LVS_MATCHED`
- `EM_CHECKED`
- `IR_DROP_CHECKED`
- `POWER_INTENT_VALIDATED`
- `UNCHARACTERIZED`

It must attach the technology fingerprint, rule/deck revision, operating
corner, input assumptions, and evidence artifact to every result. "Signoff
ready" remains unavailable until the external foundry-qualified flow passes.
