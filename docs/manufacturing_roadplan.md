# OpenChippy Physical Process Validation Architecture

## Overview

OpenChippy's physical implementation pipeline is centered around the **Physical IR**.

The Physical IR is the authoritative representation of a design.

Everything else is an interchange format generated from (or imported into) the Physical IR.

```
RTL
    ↓
Logical Netlist
    ↓
Transistor Netlist
    ↓
Physical Generator
    ↓
Physical IR
    ├── Native DRC
    ├── Native LVS
    ├── GDSII Export
    ├── GDSII Import
    └── LEF Export
```

The long-term objective is to prove that OpenChippy can generate manufacturable physical artifacts which can successfully complete an external fabrication flow such as Tiny Tapeout.

The routing architecture and acceptance order are defined in
[`routing_philosophy.md`](routing_philosophy.md). It distinguishes IEEE power
intent/modeling standards from foundry-owned physical reliability limits and
maps primary sources into current-aware PDN, IR-drop, electromigration,
pin-access, negotiated-routing, and manufacturing-repair slices.

---

## Packaged bottom-up standard-cell catalog

Physical IR version 11 embeds a technology-matched standard-cell catalog rather
than rediscovering every common circuit from flattened transistors. OpenChippy
ships recipes for the built-in educational process and the GF180MCU 3.3 V 5M
compatibility profile. The shared logical catalog contains 24 cells:

- AND, NAND, OR, NOR, XOR, and XNOR with two and three inputs
- inverter, buffer, and transmission gate
- D latch, SR flip-flop, JK flip-flop, and D flip-flop
- 2:1 and 4:1 multiplexers
- 1:2, 2:4, and 3:8 enabled decoders

Each entry has stable pins and behavior, transistor complexity, site-aligned
width, row height, routing layers, and process-specific MOS dimensions. The
recipes in `src-tauri/resources/standard_cells/` are parametric compatibility
cells, not characterized or foundry-qualified macros.

The next slice will locally route and validate canonical leaf geometry, clone
it as immutable instances, then connect only explicit cell pins. The proven
flattened route remains the production selection until that flow wins DRC and
connectivity comparison.

---

# Implementation Inchstones

The architecture below is implemented in independently testable slices. A slice
is complete only when its acceptance evidence is checked into the repository;
successful file generation by itself is not qualification.

## MPV-1 — Process-portable physical generation

- Load and strictly validate each supported OpenChippy process rule deck.
- Derive wire width, minimum route area, cut size, cut enclosure, track spacing,
  and landing geometry from the selected deck rather than educational defaults.
- Run clean inverter and NAND generation/DRC regressions against every
  non-empty example process deck.
- Prune final metal/via connected components that cannot reach a same-net
  device access or physical pin; report the removed count in Physical IR.
- When routing attempts are equally legal, prefer the smallest routed bounding
  area and then the fewest retained shapes.
- Add QD-0001 (`4B_ADDER`) to the matrix once its source project is checked in.
- Keep compatibility decks explicitly distinct from foundry signoff decks.

Acceptance:

- Every checked-in process deck loads without schema or grid errors.
- Generated inverter and NAND Physical IR has zero native DRC errors on every
  supported deck.
- QD-0001 has zero native DRC errors on every supported deck before that deck
  can advance beyond `UNIT_TESTED`.

Status: in progress. Rule-derived routing geometry and EDU/GF180 NAND matrix
coverage are implemented. The attached GF180 QD-0001 baseline exposed 3,146
fixed-geometry errors (2,858 width/area and 288 contact-enclosure errors).
The updated release generator locally regenerated the 4B_ADDER qualification
project with zero native DRC errors or warnings. Final terminal-core cleanup
removed 558 disconnected or dead-end metal/via fragments, leaving 3,507 shapes
while preserving atomic via landings. The three
existing route orders are now ranked by conflicts, routed bounding area, and
shape count. Its source project still needs to be checked into the
qualification suite before this is durable repository evidence. The current
`openchippy-sky130-5m.yaml` placeholder is empty and therefore is not yet a
process deck or part of the validation matrix.

## MPV-2 — Versioned process deck and validation records

- Extend process metadata with stable process/deck identifiers, source
  revision, units, layer-purpose mappings, and provenance.
- Store implementation and qualification claims in the separate process
  validation file described below.
- Hash the exact rule deck used by each generated artifact.

Acceptance:

- Editing a rule deck changes its recorded digest.
- Validation status cannot be embedded in or inferred from the rule deck.
- Loading an artifact with a different active deck is rejected or explicitly
  treated as a regeneration.

Status: in progress. Physical IR v5 now records a deterministic fingerprint of
the validated, normalized technology snapshot, and the physical sidebar and
headless audit output expose it. Process decks now carry stable process IDs,
deck revisions, source provenance, and strict process-owned GDS
layer/datatype-purpose mappings with legacy-compatible defaults. One Physical
IR layer may emit multiple process purposes, allowing GF180 N/P diffusion to
produce shared COMP plus polarity-specific implant geometry. The separate
typed validation-record schema remains.

## MPV-3 — GDSII export and structural validation

- Export hierarchy, boundaries, instances, labels, transforms, arrays, and
  database units from Physical IR.
- Validate record structure, references, coordinates, and layer/datatype pairs.
- Open generated output in an external viewer such as KLayout.

Acceptance:

- Native structural validation passes.
- KLayout opens QD-0001 without repair or malformed-record warnings.
- Reported top-cell bounds and per-layer geometry counts match Physical IR.

Status: in progress. OpenChippy now exports a flat binary GDSII stream from the
accepted Physical IR, emits rectangular boundaries and physical-pin labels,
uses the active deck's database-unit scale, and refuses to save when its native
record validator finds malformed structure. Regression coverage checks the
record stream, closure, boundary and label counts, per-layer counts, and
the exported shape envelope against an inverter Physical IR. The shape envelope
is intentionally distinct from planning/floorplan bounds so legal exported
geometry outside a provisional floorplan does not invalidate the GDS stream.
GF180 export now consumes the public process assignments for COMP, Nplus,
Pplus, Nwell, Poly2, contacts, Metal1-Metal5, and Via1-Via4; the synthetic
substrate preview is excluded and pin labels use the process label datatype.
Reusable physical blocks now export as named GDS structures referenced by the
top cell, while global/inter-block routing remains in the top structure. The
first hierarchy slice retains absolute block coordinates with zero-origin
references; canonical shared-cell geometry, translated/rotated transforms,
and arrays remain before MPV-3 is complete. KLayout 0.30.8 independently opens
and re-saves QD-0001 without repair, and its hierarchy, database units, bounds,
layer counts, and geometry compare canonically with the native stream.

Native validation can also be run directly on an exported artifact:

```sh
cargo run --manifest-path src-tauri/Cargo.toml \
  --example gds_validate -- /path/to/design.gds
```

## MPV-4 — GDSII import and canonical round trip

- Parse the supported GDSII records into Physical IR.
- Canonicalize geometry and compare hierarchy, instances, pins, labels, layer
  mapping, transforms, and connectivity.
- Re-import a KLayout-exported copy.

Acceptance:

- Native export/import is canonically equivalent.
- KLayout import/export/import is canonically equivalent within documented
  normalization rules.
- Native DRC produces equivalent results before and after round trip.

Status: in progress. A typed canonical importer now reads the supported MPV-3
record set (libraries, units, structures, boundaries, labels, and references),
normalizes polygon start vertex/winding and element ordering, rejects malformed
or unsupported geometry forms explicitly, and verifies exact equivalence
between the native QD-0001 export and a KLayout open/re-save copy. The importer
now also flattens referenced structures, reverses the technology layer map,
collapses process-purpose diffusion/implant pairs, infers same-layer and
via-to-metal connectivity, and can run native physical DRC on the imported
geometry. Translated/rotated transforms, arrays, and exact before/after native
DRC equivalence remain. Exact equivalence needs an explicit normalization rule
for component ownership and coincident logical shapes because ordinary GDSII
does not preserve either attribute.

An imported artifact can be mapped and audited with:

```sh
cargo run --manifest-path src-tauri/Cargo.toml \
  --example gds_import_audit -- /path/to/design.gds \
  docs/examples/gf180mcu-3v3-5m-openchippy.yaml
```

## MPV-5 — Native LVS and LEF export

- Compare the logical/transistor representation with devices and connectivity
  recognized from Physical IR.
- Export LEF macros, pins, obstructions, symmetry, site, and dimensions.

Status: started. Physical IR v10 gives each MOS gate an explicit process-sized
poly/contact/Metal-1 access stack outside the source/drain landing area. Native
physical validation now reports missing access landings and logical nets split
across multiple routing islands; manufacturing GDS export is blocked while
either condition exists. Connectivity-first route-attempt selection and
process-preferred upper-layer distribution first reduced GF180 4B_ADDER from
73 to 63 open nets. A post-route connected-island repair now searches
clearance-safe same-layer doglegs and commits each bridge atomically. Its
nearest-island search evaluates up to 96 deterministic anchor pairs and uses
true two- or three-segment Manhattan paths. A bounded alternate-layer pass
adds atomic landing/via stacks and searches obstacle-edge tracks, reducing that
result to 14 opens with zero spacing, cut, grid, or minimum-area errors.
Power distribution is no longer constrained to one VDD/GND rail pair: the
placer derives full-width Metal-2 trunks from every occupied PMOS/NMOS row,
retains edge access, and connects each supply terminal to its nearest
same-polarity trunk. This reduces the same GF180 result to 7 open signal nets
while retaining zero shorts and zero geometry, cut, grid, or minimum-area
errors. Multi-row regression coverage requires more than two distinct power
tracks. The first hierarchy-first standard-cell staging slice now groups identical
reusable instances by transistor topology, orders cells by their non-power
next-hop mesh, copies canonical device-relative placement into rectangular
macro slots, and atomically commits each typed macro boundary plus all device
footprints before considering the next slot. The candidate records its
standard-cell instance count and cannot leave partial occupancy after failure.
Production enumeration remains gated while canonical local-route and
boundary-pin templates are added: inserting the placement-only prototype
changed downstream timing-candidate identity and produced 10 opens, so the
proven placement remains active at 7. Physical IR v10 invalidates older caches.
Physical ownership now explicitly separates the largest floorplanning owner
from the lowest reusable instance. For example, a transistor flattened as
`FULL1·NAND3·M1` belongs to the `FULL1` region while its standard-cell owner is
`FULL1·NAND3`. The leaf candidate is stored outside the production candidate
array, avoiding timing-index changes, and publishes leaf—not parent—macro
regions. Nested regression coverage proves one parent region can contain two
independently reserved reusable leaf cells.
Residual-open repair now includes the RP-4 multilayer A* track graph. It
searches process-permitted `(x track, y track, metal layer)` states with legal
planar/via edges, an admissible Manhattan heuristic, deterministic costs, and
no wall-clock cutoff. A finer access lattice is retried when the process-pitch
lattice is exhausted, and long terminal-connected metal shapes expose their
full usable span instead of only their center. On the GF180 4B_ADDER evidence
case this reduces native DRC/LVS from 7 open signal nets to 1 while retaining
zero shorts and zero geometry, cut, grid, spacing, or minimum-area errors. The
remaining open is a sealed local pin-access connection inside
`XFULL_ADDER_1B4·XNAND_Gate8` (`M3.source` to `M4.drain`); it requires a
reserved local dogbone/access landing before surrounding global routes are
committed, rather than a larger late-stage search budget.
Physical IR v12 deliberately replaces the fixed per-transistor access model
instead of preserving that one-open result as the production architecture.
Repeated leaf blocks now use canonical standard-cell placement, and poly still
crosses the complete active region while its contact/Metal-1 extension can
move to either legal side. The generator tries the topology-preferred side and
atomically admits the complete diffusion/poly/contact/landing footprint before
routing. Conventional placement is no longer allowed to win merely because it
is locally closer to closure. The first honest GF180 4B baseline is 6 native
errors: 4 unrouted terminals plus 2 contact-enclosure failures, 0 shorts, and
144/144 recognized MOS. The next cell-geometry slice must synthesize continuous
active strips/shared series diffusion and reserve local dogbones and boundary
pins before committing global conductors.
The first lower-metal refinement slice now treats M1/M2 as flexible local
interconnect as well as power-access resources. It generates compact dogbone
alternatives for two-terminal, same-row internal nets inside one canonical
cell: direct M1 bends first, then M2 tracks with complete paired via stacks.
Global routing and exhaustive open-net repair finish before these candidates
are considered. Each local route is admitted as one atomic canvas transaction,
so it cannot strand a bend, landing, or via, and final orphan cleanup removes
unused alternatives. Cross-row, boundary, pin, and power nets remain outside
this initial scope until a process-aware local-channel allocator can reserve
their capacity.

On the real GF180 4B_ADDER this keeps the v12 baseline unchanged at 6 errors
(4 unrouted M1 terminals and 2 contact-enclosure errors), 0 shorts, and 144/144
recognized MOS devices while retaining 56 additional legal local-route shapes.
This is a safe enabling slice rather than a connectivity-closure claim.

Physical IR v13 changes the placement foundation rather than adding a display
overlay. The selected floorplan is filled with equal-height, alternating
N-well and P-well stripes; PMOS devices legalize into N-well stripes and NMOS
devices into P-well stripes. Placement begins at an enclosure-aware corner,
uses every legal site in a stripe, then spills into the next compatible stripe.
The stripe width and device edge capacity are derived from the process grid,
well enclosure, row height, and floorplan bounds. GF180 maps the explicit
P-well geometry to the foundry LVPWELL layer (204/0).

Ordinary designs now use flattened transistor geometry by default. Logical
hierarchy and repeated-block signatures remain available to planning, but do
not force copied physical macros. Deterministic cells may replace flattened
geometry only when an explicitly characterized implementation demonstrates a
concrete area, routing, timing, power, or generation benefit. Memories,
register arrays, and other regular structures are the primary intended
candidates. This prevents an arbitrary schematic block boundary from becoming
an artificial physical constraint while preserving deliberate macro support.

Physical IR v14 removes the remaining fixed full-height poly-gate template.
Each transistor now emits a process/device-sized gate core across active plus a
separate one-sided access extension sized from the selected contact, enclosure,
grid, and access direction. Same-net overlapping poly fragments may merge
atomically in the placement canvas. Transistor terminal connectivity is rooted
at manufactured contacts rather than counting both a gate's poly body and its
contact as duplicate terminal markers. Older cached physical artifacts are
invalidated by the IR revision.

Physical IR v15 removes the remaining left-edge bias from striped-well
placement. Devices are still ordered by routed topology, but the
highest-connectivity device now seeds the center of a compatible well stripe
and the remaining devices expand through the nearest legal sites on either
side. The well is therefore a legal placement region rather than a
left-to-right queue. Final cleanup also distinguishes transactional
local-route provenance from true component pins, so a rejected or superseded
branch cannot survive merely because it carries an internal route marker.
Physical caches older than v15 must be regenerated before placement or
connectivity results are compared.

Physical generation now runs independent geometry/refinement attempts on a
bounded work-stealing thread pool. Each attempt owns a separate placement and
routing canvas, so parallel execution cannot race the occupancy state or admit
conflicting geometry. Results are collected and scored in deterministic input
order. Routing nets within an individual attempt remains serialized until the
shared canvas supports deterministic partitioning, reservation, and merge.

Residual connectivity repair uses a staged runtime architecture. Independent
candidates first run bounded, cached dogleg and layer-family screening in
parallel. Only the selected candidate receives multilayer graph repair, and
that graph is confined to the rectilinear corridor between unique island
access centers instead of spanning every track across the chip. The default
uses the coarse corridor; finer lattices are available only through
`OPENCHIPPY_EXPERIMENTAL_ASTAR=1`. On the GF180 4B_ADDER reference this changes
generation from more than an hour to approximately 2 minutes 55 seconds,
recognizes all 144 MOS devices, reports zero shorts, and leaves 12 explicit
open nets for topology/access work. A measured fine pass took approximately
four minutes, added about 700 shapes, and improved only two opens, so it is not
the production default.

Placement closure now treats a flattened leaf logic cell as a contiguous
neighborhood without turning it into an immutable copied macro. A strict
M1/M2 power-reservation experiment increased the reference design from 5 open
nets to 15–24, showing that layer restriction was exposing rather than causing
the remaining defect. The actual placement failure was the center-out row
legalizer: consecutive transistors in one NAND could alternate onto opposite
sides of a row, and a leaf group could straddle a row boundary. Leaf groups
now wrap intact and place their members on consecutive sites, while topology
still orders the groups and global routing remains free to use process-allowed
layers. The GF180 4B_ADDER reference consequently produces 5,223 shapes, passes
native physical DRC with zero errors, recognizes all 144 MOS devices, closes
all 82 nets with zero shorts, and passes native LVS.
These immutable diffusion landings and contiguous leaf-placement semantics
define Physical IR v16; v15 cached `.chippy_gds` artifacts are deliberately
rejected and regenerated.
Physical IR v17 extends that cache boundary to the manufacturing contract.
Older GF180 snapshots are migrated narrowly by process identity to canonical
LVPWELL `204/0`, N+/P+ implant enclosure, exact contact-cut size, and 9K
top-metal rules. Custom technologies are never assigned guessed process rules
and still fail validation until their own deck declares them.

Physical IR v18 begins the process-derived device-geometry pass. It may mirror
source/drain orientation for electrically symmetric MOS placement, and joins
adjacent same-polarity active regions only when their facing terminals carry
the same net. GF180 now declares a maximum tap distance and receives explicit
well/substrate tap arrays connected to distributed power tracks. Implant
enclosures were raised to the public-deck requirement. Together these changes
remove every official GF180 tap and implant marker from the 4B_ADDER reference:
the external variant-C result falls from 375 to 119, of which 110 are metal
spacing findings and nine are density/stream-correlation findings.

Final legalization also expands the active floorplan to contain legal routing
geometry, but never beyond the technology's usable tapeout window; exceeding
that window is a routing failure. Residual-open fallback routes now compare all
legal launch candidates by wire length and penalize excursions outside the
occupied design envelope instead of accepting the first legal boundary detour.
Same-net fragments inside a routed-metal spacing threshold are coalesced only
when the process-sized bridge is grid-aligned and creates no cross-net conflict.
M1 device/contact landings remain the responsibility of pin-access legalization.
Physical IR v19 invalidates the first v18 cache: its final outline now snaps to
complete well-stripe pitches, rebuilds substrate and alternating N/P well
coverage across that entire outline, and measures detour excursions against
device fabric rather than an envelope already polluted by earlier routes.
Physical IR v20 replaces component-wide leaf trimming with a length-weighted
terminal route tree. It discards redundant cycles, via ladders, and route
attempt geometry while retaining process-valid enclosure landings for every
selected via. Non-M1 metal overhangs are trimmed to their outermost electrical
attachments, and compatible consecutive devices may share diffusion across a
larger otherwise-unused local gap. On the GF180 4B_ADDER reference this reduces
the final IR from 5,501 to 4,068 shapes, removes 1,804 routing shapes, and still
passes native DRC/LVS with 82/82 nets closed, zero shorts, and 144/144 MOS.
The official GF180 variant-C deck falls from the submitted v19 stream's 231
violations to 43 on the regenerated v20 stream: 34 spacing findings and nine
density/DBU findings.

The first official-deck rerun of the v16 placement-closed GF180 4B_ADDER reads
all three LVPWELL polygons and reduces the archived baseline from 3,839 to
2,760 violations. The remaining families are process-geometry work rather
than OpenChippy native connectivity failures: exact 0.22 µm contacts, 9K top
metal width, implant enclosure/extension, well/substrate taps, active density,
and smaller metal width/spacing/area findings. KLayout represents the encoded
0.001 µm database unit as `0.0009999999999999998`; the official deck's exact
floating comparison reports one DBU marker, which requires stream-unit
correlation rather than a tolerance waiver.

The next device-geometry slice replaces the fixed `1.55 µm` active rectangle
with process-derived active spans and legal shared diffusion. This is not a
license to resize the transistor channel arbitrarily: gate length and effective
channel width remain explicit electrical parameters. Only source/drain active
outside the channel may stretch, merge, or compact when adjacent devices have
the same polarity, occupy the same compatible well, and expose the same
facing net. A shared region must:

- preserve a gate-defined channel for every transistor;
- preserve source/drain contact enclosure and implant/well rules;
- carry an explicit set of owning devices and terminal nets in Physical IR;
- be reserved atomically in placement before global routing;
- remain reconstructable by native LVS and GDS round trip;
- fall back to isolated active when sharing is not legal or does not improve
  area/routing.

OpenLane/OpenROAD places predesigned, characterized standard cells rather than
reshaping transistor diffusion during top-level routing. OpenChippy's analogous
optimization therefore belongs in leaf-cell/device geometry generation before
placement; a generated geometry variant must pass DRC/LVS and eventually be
characterized before it can become a reusable fixed cell view.

The target is topology-shaped row geometry rather than individually stretched
rectangles. Within a compatible row, Physical IR should form maximal legal
N-diffusion or P-diffusion islands from consecutive source/drain connectivity.
It may widen or jog poly and add a row-local gate-net strap when that reduces
access cost, but every transistor crossing must preserve process-owned channel
length, gate extension, well enclosure, and contact rules. Contacts are placed
only after the active/poly topology is known.

This work is split into measurable slices:

1. Record explicit provenance for device channels, diffusion islands, poly
   gates/straps, contacts, pin landings, route segments, and via landings.
2. Build row topology from oriented source/gate/drain sequences and synthesize
   maximal same-net active islands without crossing an occupied site, another
   net, or a well/process boundary.
3. Synthesize gate poly from row topology, allowing legal widening, jogs, and
   shared gate-net access instead of fixed per-FET rectangles.
4. Score and compact placement using synthesized active/poly geometry and
   hard-net access cost, without forcing left-to-right alignment.
5. Extract a minimum terminal-connecting route tree and trim all internally
   generated M1-Mx segments to their outermost real attachments. External pins
   and immutable device landings remain protected.

Acceptance on the exact GF180 4B adder requires native DRC/LVS/connectivity to
remain clean, official DRC not to regress by rule class, no terminal-free
metal/via component to survive, deterministic regeneration, and explicit
reporting of route area plus maximum envelope excursion. Physical IR v21 starts
the fifth slice by making overhang cleanup provenance-aware on M1 as well as
upper metal, directly targeting the remaining access-route shootouts visible
in the v20 layout.

Physical IR v22 completes the first two topology slices and establishes the
third:

- every generated shape carries backward-compatible explicit purpose
  provenance (`active`, `gate`, `gate_access`, `contact`, `device_landing`,
  `pin`, `route`, `via`, `via_landing`, `fabric`, or `tap`);
- deterministic row topology records oriented devices, ordered terminal nets,
  maximal active-island membership, bounds, and eligible shared gate straps;
- compatible consecutive devices now replace their individual active
  rectangles with one owned maximal active island, rather than drawing a
  cosmetic overlay;
- native device recognition, contact enclosure, and gate-extension checks use
  the row-island ownership map; and
- placement scoring rewards both legal diffusion sharing and adjacent
  same-gate access opportunities.

On the exact GF180 4B adder, v22 reduces 144 separate active rectangles to 38
shared active islands containing all 144 MOS devices and creates seven legal
rectilinear shared gate-access straps covering 14 devices. A dedicated
gate-topology placement candidate groups common gates while preserving
source/drain affinity; the final topology may bridge contacts on opposite
sides of a row through a checked trunk-and-drop poly tree. Drops may cross
active only along the corresponding device's existing gate axis, preventing
accidental transistor creation. The artifact contains 3,966 shapes (down from
4,065 in v21), removes 1,807 orphan/dead routing
shapes, has zero native DRC errors or warnings, closes all 82 nets with zero
shorts, and matches native LVS at 144/144 recognized MOS devices. Official
GF180 DRC must be rerun on a v22 stream before official rule-class
non-regression is claimed.

The contact stage now consumes finalized island topology after routing and
cleanup. When a facing source/drain boundary has enough legal M1 clearance,
the two per-device cuts and landings are replaced atomically by one
topology-owned contact and one landing spanning the original route endpoints.
If that landing would violate a foreign-net M1 keepout, the isolated contacts
remain. The exact 4B artifact accepts 98 shared terminal contacts covering 196
device terminals, reducing the final IR to 3,770 shapes without changing its
route solution, native DRC, connectivity, or LVS result.

Two independent v22 exports are byte-identical
(`7aff2abdbbbbb57095b7b2a3f2495dd30544a1af81c8c9986e2d41721afdd4ba`),
structurally valid, and contain 3,855 boundaries, 15 labels, five structures,
and four references. The official GF180 variant-C 5LM/9K deep deck reports 39
violations, improving the v20 reference result of 43 rather than regressing it.
The remaining classes are DCF.1b (1), M1.1 (4), M1.2a (1), M1.4 (1), M2.2a
(9), M2.4/M3.4/M4.4/M5.4 (1 each), MT.2a (16), MT.3 (1), PL.8 (1), and the
known DBU comparison (1).

Physical IR v24 completes the first compact-routing and route-quality slice.
Signal nets are packed onto reusable interval tracks instead of receiving one
chip-wide track each. Same-cell gate nets may use checked cross-row poly
straps, provided the strap cannot cross foreign active or create an unintended
transistor. Final route-quality diagnostics persist routed area, routing
bounds, device/terminal-envelope excursion, terminal-free conductor
components, duplicate vias, unlanded vias, and the largest residual
excursions.

Foundry-style boolean geometry exposed a distinction that the earlier native
rectangle checks missed: offset same-net route and landing rectangles can be
electrically connected while leaving a sub-rule inward-facing notch. v24
therefore adds coverage-aware composite `route_fill` geometry. A fill is
emitted only for an uncovered same-net notch or thin landing transition, must
touch two supporting same-net conductor shapes, and is rejected if it
approaches a foreign net. It is not treated as an independently sized wire
because its legal width and area are those of the resulting composite
conductor.

On the exact GF180 4B adder, v24 contains 3,724 Physical IR shapes, including
38 shared active islands covering all 144 MOS devices, 61 poly gate straps
covering 122 devices, 99 shared terminal accesses covering 198 terminals, and
23 composite route fills. It closes all 82 nets, has zero shorts, zero native
DRC errors or warnings, matched 144/144 native LVS, zero terminal-free routed
components, zero duplicate vias, and zero unlanded vias. Compared with v22,
routed rectangle area falls from 1,602.394 to 1,370.238 µm², routing bounding
area falls from 9,004.461 to 5,408.491 µm², and maximum terminal-envelope
excursion falls from 27.33 to 0.905 µm.

Two v24 GDS regenerations are byte-identical
(`313983151abe5a883e3c10dae19cf34ebe8d26464ac7d881cedc27f1cc8b811f`),
structurally valid, and contain 3,809 boundaries, 15 labels, five structures,
and four references. The official GF180 variant-C 5LM/9K deep deck reports
nine findings, down from v22's 39. All routed metal spacing and minimum-width
findings are closed. The remaining set is process-wide minimum-area/density
and comparison infrastructure: DCF.1b, M1.4, M2.4, M3.4, M4.4, M5.4, MT.3,
PL.8, and DBU (one each). These now form the fill/density qualification slice,
not an unresolved routed-connectivity defect.

Physical IR v25 adds process-owned, non-electrical density fill after routing,
topology materialization, and cleanup. Technology decks declare target and
maximum density, primary and fallback tile sizes, circuit/fill spacing,
optional support material, and distinct GDS layer purposes. Centered tile
dimensions must place both edges on the manufacturing grid. Dummy shapes carry
no net or component identity, are excluded from route quality and extraction,
round-trip through GDS as `dummy_fill`, and are checked independently by native
DRC without changing electrical connectivity.

On the exact GF180 4B adder, v25 contains 5,617 Physical IR shapes, including
1,893 dummy-fill shapes. Native DRC remains 0/0, all 82 nets close, native LVS
matches all 144 MOS devices with zero shorts, and route quality still reports
zero terminal-free routed components, duplicate vias, or unlanded vias. Two
fresh GDS exports are byte-identical
(`be204ebec25b97c69f091c08f831ff0ba0c004adfef5e2814019446dfc959660`),
structurally valid, and contain 5,702 boundaries, 15 labels, five structures,
and four references. KLayout measures Metal1–5 and top-metal density at
30.0113%, 30.7179%, 31.8982%, 32.0510%, 35.1271%, and 31.0417%.
The official GF180 variant-C 5LM/9K deep deck falls from nine findings to
three: DCF.1b active density, PL.8 poly density, and the known DBU comparison.

The remaining FEOL density work is not a license to place dummy active or poly
inside arbitrary wells. GF180 requires dummy COMP/poly clearance from
Nwell/LVPwell boundaries, while the current alternating well fabric consumes
the complete routed outline. The next topology slice must plan well-owned
device banks and legal field-fill regions together: preserve every MOS/tap
enclosure, reserve sufficient field area before routing, emit supported dummy
active/poly there, and include that area in placement, tapeout containment,
density, native DRC, and official-deck candidate scoring.

Physical IR v26 implements that split. Device and tap topology now owns a
bounded alternating-well bank; routing may expand the full chip outline without
stretching those wells through the reserved field bands. Before final fill,
the planner deterministically grows the outline within the usable tapeout
height until process-legal FEOL fill can satisfy both active and poly density.
GF180 emits paired 6 × 6 µm dummy COMP/poly tiles in those fields so each
dummy-active island has the required poly cover while retaining well and
circuit clearance.

On the exact GF180 4B adder, v26 contains 6,761 Physical IR shapes, including
3,038 dummy-fill shapes. Native DRC remains 0/0, all 82 nets close with zero
shorts, native LVS matches all 144 MOS devices, and route quality still reports
zero terminal-free routed components, duplicate vias, or unlanded vias. Two
GDS regenerations are byte-identical
(`fb3a882ab0466e4c2c26355a8529782cbe3fabea50ac83939a097f78641fb886`),
structurally valid, and contain 6,846 boundaries, 15 labels, five structures,
and four references. KLayout measures active/poly density at 26.5108% and
25.8357%; Metal1–5/top-metal measure 32.5078%, 32.7970%, 32.5243%,
31.4645%, 32.8606%, and 31.1797%. The official GF180 variant-C 5LM/9K deep
deck reports only its known DBU comparison finding: routed geometry, FEOL
density, DPF.1 dummy coverage, and all metal density rules pass.

Physical IR v27 begins topology-aware physical ordering rather than treating
the schematic or flattened device list as a placement order. A geometry
candidate walks the strongest device-net adjacency into actual neighboring
sites. For reusable leaf blocks, one connectivity-weighted block order is
projected into both PMOS and NMOS rows so related pull-up and pull-down
networks remain vertically aligned. Both the established leaf order and the
topology challenger are fully routed; the winner is selected from real
overflow, detailed conflicts, timing, area, wire length, and via count rather
than the placement heuristic alone.

On the exact GF180 4B adder, the selected topology candidate reduces global
routing overflow from 58 to 28 and routed wire length from 2,243.390 µm to
1,948.150 µm versus the conventional leaf-order candidate. It increases
cross-row gate sharing from 61 straps/122 devices to 88 straps/176 devices and
shared terminal access from 99/198 terminals to 101/202. Full-width Manhattan
corner fill closes foundry minimum-width notches without creating concave
replacement notches. The resulting 6,766-shape IR remains native DRC 0/0,
closes all 82 nets with zero shorts, matches all 144 MOS devices in native LVS,
and has no terminal-free route components, duplicate vias, or unlanded vias.
Two GDS regenerations are byte-identical
(`69f5d177f4f18f1f1a17a50189c87feac21fbe8342e3114b3217d5c667e5d092`);
the official GF180 variant-C 5LM/9K deep deck again reports only its known DBU
comparison finding.

Physical IR v28 removes the remaining fixed transistor-template dimensions.
Source/drain access offset, active width, and active height now come from one
process-derived geometry contract. Gate length and Poly2 width determine the
gate-side clearance; contact size/enclosure and active spacing determine legal
source/drain access; and the schematic transistor width becomes the physical
channel width subject to diffusion width/area/contact minima. Floorplan area,
placement pitch, footprint admission, row-island synthesis, shared-contact
replacement, and route anchors all consume those same dimensions.

Mixed-width regression devices (0.7 µm NMOS and 2.4 µm PMOS) preserve their
requested channel widths, pass native DRC/LVS, and regenerate deterministically.
The exact GF180 4B adder remains clean at 6,779 shapes: native DRC 0/0, all
82 nets closed with zero shorts, native LVS 144/144, no terminal-free route
components, duplicate vias, or unlanded vias. It increases legal shared
terminal accesses to 106 pairs/212 terminals while retaining 88 gate straps
covering 176 devices. Two GDS exports are byte-identical
(`51c6cedb6b47888d9b22bd6d2d00fb604a6e44014f93d1c3ca9778b16b4caa99`);
the official GF180 variant-C 5LM/9K deep deck still reports only DBU.

Physical IR v29 orients and compacts complete physical rows rather than
independently recentering hierarchy leaves. Its variable pitch is derived from
the real poly/contact access footprint, and the compact footprint admitted by
placement is the same atomic geometry seeded into detailed routing. Placement
reports persist whether the candidate was topology-compacted, so cached and
rendered results remain auditable.

Correctness remains the final selection gate. Final refinement carries the
strongest compact and conventional placements through full multilayer closure
before comparing connectivity, required M2 power distribution, conflicts,
route envelope, and shape count. This bounded two-finalist policy prevents a
cheap preview from rejecting a compact candidate merely because it exposes a
route that the exhaustive repair stage can legally close.

Physical IR v30 stops independently recentering complete PMOS and NMOS rows.
It first captures each contiguous logical-leaf occurrence, compacts only the
transistors inside that occurrence, and assigns all polarity occurrences of
the leaf one shared horizontal center derived from the legal pre-compaction
placement. Ungrouped row runs retain their own center. Final row columns are
then recomputed from the moved geometry and every non-active footprint is
admitted atomically before the candidate can proceed. This preserves flattened
transistor geometry while keeping cross-row gate access and next-hop locality.

The exact GF180 4B compact candidate now closes the two formerly open signal
nets and wins final selection. It emits 6,521 shapes, 38 shared active islands
covering all 144 MOS devices, 105 shared terminal accesses covering 210
terminals, and 88 gate straps covering 176 devices. Native DRC is 0/0, all 82
nets close with zero shorts, native LVS matches 144/144 MOS, and route-quality
audits find no terminal-free components, duplicate vias, or unlanded vias.
Two complete audits are byte-identical. Two GDS exports are byte-identical
(`a07d7d3afc707dfc1d5a77f83a95cf1a5d61c247a63031ccf108e9159900c5e4`);
the stream contains 6,606 boundaries, 15 labels, five structures, and four
references. The official GF180 variant-C 5LM/9K deep deck reports only its
known DBU comparison finding and no geometry/process violations.

The next routing experiment keeps leaf-local signal-access sharing as an
optional refinement rather than forcing it into every net. Terminals may share
an upper-metal escape only when they belong to the same leaf standard-cell
occurrence and the complete bus, via stacks, and grid-guarded enclosures can be
admitted atomically. The established and shared-access policies are screened
independently, so a shared escape cannot replace a smaller legal route merely
because it uses fewer drops. On the exact GF180 4B design the established v30
route still wins and remains byte-for-byte identical; this preserves the clean
baseline while allowing later fixtures with repeated local fanout to prove
whether the refinement has real value.

Physical IR v31 makes geometric closure stricter than graph reachability.
Every gate/source/drain contact must physically touch its topology-owned
poly/active and a same-net Metal 1 landing; physical boundary pins participate
in the required net islands; and every axial generated-metal endpoint must end
on compatible metal, a legal via, a device contact, or a pin. Final cleanup
retains one canonical device landing per contact and removes redundant access
bars and attached dead branches. The exact canonical GF180 4B adder closes all
144 devices, 82 nets, and route endpoints with native DRC/LVS clean and only
the known official-deck DBU comparison marker.

Physical IR v32 replaces the fixed-height shared-active rectangle with a
deterministic rectilinear union derived from each row's oriented transistor
topology. Equal-width runs remain one rectangle. Mixed-width runs are split
into non-overlapping, edge-abutting rectangles so every gate retains its
process-derived channel height while compatible facing source/drain terminals
share one manufactured diffusion island. The decomposition is persisted in
row topology and consumed by contact enclosure, terminal attachment, native
DRC, and LVS rather than being a display-only shape.

A focused 0.7 µm/2.4 µm NMOS series regression proves both channel heights
survive sharing. The complete 172-test Rust suite passes. The canonical GF180
4B adder contains only equal-width devices, so its qualified output remains
byte-identical to v31: 6,393 IR shapes, 144/144 recognized devices, 82/82
closed nets, zero shorts, zero native DRC diagnostics, zero missing terminals,
zero dangling route endpoints, and deterministic GDS SHA-256
`c0145c8f5e701f70a47fd8b9268354a0a9525e81883671dda5553b39e4654f44`.
The official GF180 variant-C 5LM/9K deep deck again reports only its known DBU
comparison marker and no foundry geometry/process violation.

Physical IR v33 closes a placement-to-synthesis occupancy mismatch. Compact
placement now atomically reserves one topology-owned diffusion envelope for
every prospective shared-active run before that candidate can be selected or
detailed routing can consume the surrounding sites. Final synthesis still
replaces the conservative reservation with v32's exact rectilinear union.
Edge-abutting same-net poly is treated as one continuous conductor during
occupancy admission, while a nonzero sub-spacing gap remains illegal.

This is also an acceptance-policy correction: a visually plausible or
geometry-DRC-clean layout is not "good" while any transistor terminal or
logical net is open. The exact canonical GF180 4B adder keeps topology
compaction enabled and reports 144/144 recognized devices, 82/82 closed nets,
zero missing terminals, zero shorts, zero dangling route endpoints, zero
native DRC diagnostics, and native LVS matched. The complete Rust suite now
passes 173/173 tests and the production frontend and native release builds
pass.

Physical IR v34 removes another logical-membership shortcut from shared poly.
Cross-row gate grouping previously drew an L-shaped connection between only
the first and last contacts, then recorded every intermediate device as a
member. It now evaluates deterministic horizontal- and vertical-trunk trees,
adds a branch to every member contact, rejects any tree that does not
geometrically cover every claimed gate, and chooses the smallest legal tree.
Compact placement replays this exact synthesizer and reserves each selected
same-net tree as one composite conductor; foreign-net poly remains a hard
collision.

Dedicated three-device cross-row and collective-occupancy regressions cover
the defect. The complete Rust suite passes 175/175 tests. The canonical GF180
4B adder remains topology-compacted with 144/144 devices, 82/82 closed nets,
zero missing terminals, opens, shorts, dangling endpoints, or native DRC
diagnostics, and matched native LVS. Two GDS exports remain byte-identical at
415,654 bytes and SHA-256
`c0145c8f5e701f70a47fd8b9268354a0a9525e81883671dda5553b39e4654f44`.
The official GF180 variant-C 5LM/9K deep deck reports only its known DBU
comparison marker.

Physical IR v35 corrects the acceptance contract exposed by visual inspection:
a gate-strap bounding box and list of device IDs are not proof of physical
connectivity. Every shared strap now persists its exact rectilinear conductor.
Native DRC requires that conductor to exist in the emitted shape set, form one
continuous union, and physically reach the component-owned gate access of
every device it claims. Native LVS treats any violation as an open.

Applying this stricter rule to the prior v34 GF180 4B result exposes 88
unmaterialized gate straps. The earlier zero-open result is therefore withdrawn
as sufficient qualification evidence: its ordinary metal/contact graph closes,
but its persisted topology claims geometry that is absent from the final
rendered shape set. The defect was final-preview admission: it committed each
poly rectangle independently, so a trunk or branch could reject against its own
same-net access even though placement had admitted the complete tree.

Final preview admission now groups topology-owned poly by net and commits the
complete conductor transactionally. The GF180 4B result emits 232 gate-access
rectangles—144 component-owned accesses plus all 88 shared straps—and the
strict v35 check returns to zero native DRC diagnostics, 144/144 recognized
devices, 82/82 closed nets, zero missing terminals/opens/shorts, and matched
native LVS. Two GDS exports are byte-identical at 421,286 bytes with 6,566
boundaries and SHA-256
`6b5b546f4880dfb9fff4e9f0ad2fc4f8940f7a33eff80f7e0fc856ab57cbdcd6`.
The official GF180 variant-C 5LM/9K deep deck reports only its known DBU
comparison marker. This restores qualification with physical evidence rather
than suppressing the defect.

Physical IR v36 removes the remaining placement approximation for shared
active. Compact placement previously reserved one bounding rectangle around a
run, while final synthesis emitted a smaller rectilinear union. Placement and
final preview now replay the same topology synthesizer, group edge-abutting
active rectangles into physical islands, and commit each island atomically.
This preserves mixed transistor widths without making a legal polygon collide
with its own spacing halo and frees empty corners of the former bounding box
for later legal placement.

Native DRC/LVS now independently require every persisted active rectangle to
exist in emitted geometry, require the union to be connected, and require every
claimed transistor gate and terminal access to reach it.
`CONNECTIVITY.OPEN_ACTIVE_ISLAND` prevents island metadata from hiding a
missing rectilinear piece. Dedicated 0.7 µm/2.4 µm placement and malformed-IR
regressions prove both atomic admission and rejection.

The equal-width GF180 4B reference remains byte-identical to v35: native DRC
and LVS are clean, all 144 devices and 82 nets close, and deterministic GDS
remains 421,286 bytes/6,566 boundaries with SHA-256
`6b5b546f4880dfb9fff4e9f0ad2fc4f8940f7a33eff80f7e0fc856ab57cbdcd6`.
Byte identity preserves the official variant-C result of only the known DBU
comparison marker. The complete Rust suite passes 177/177 tests.

The native LVS API recognizes all 144 expected MOS devices and all of their
terminals, reports zero shorts, and now matches the fully closed reference.
The first LEF export slice emits a Physical-IR-sized block macro
with directions/uses for physical pins, metal pin rectangles, symmetry, and
non-pin metal obstructions from the 3D sidebar. It now emits a process-derived
placement site using the planning site width and row height. KLayout 0.30.8
independently parses the GF180 QD-0001 LEF as a 76.09 × 65.59 µm macro with
all 15 physical pin labels. Closing the remaining routes and comparing
extracted device connectivity against the logical transistor graph remain
before MPV-5 acceptance.

Acceptance:

- QD-0001 passes native LVS with deterministic mismatch diagnostics.
- LEF parses in an independent tool and agrees with Physical IR bounds and pins.

### MPV-5.1 — Tapeout perimeter I/O planning

Schematic-level digital inputs and outputs need an explicit physical interface
to the tapeout boundary. This is separate from internal transistor pin access
and from a later package/bond-pad implementation.

- Each promoted digital input or output may be assigned to `left`, `right`,
  `top`, or `bottom` of the usable tapeout perimeter.
- An assignment records direction, edge, ordering, and either an automatic
  legal position or a user-requested offset. VDD, GND, clock, reset, analog,
  bidirectional, and reserved roles remain distinguishable even when this first
  UI exposes only digital inputs and outputs.
- Digital input and output symbols expose a tapeout-pin assignment in the
  schematic inspector. The tapeout-I/O scope may also assign ports in bulk,
  filter them by direction or assignment state, report
  duplicate/missing/illegal assignments, and preview edge order without
  changing logical connectivity.
- Physical planning legalizes assignments onto the selected process pin layer,
  grid, width, spacing, edge keepout, and permitted access direction. It may
  move an automatically placed pin along its chosen edge, but must report a
  fixed-position conflict rather than silently moving it to another side.
- Global routing treats legalized perimeter pins as immutable terminals and
  budgets escape capacity before ordinary signal routing.
- The schematic and 3D/GDS views show the tapeout outline, named perimeter
  pins, logical direction, and highlighted hookup path from a selected
  schematic input/output symbol to its physical boundary terminal. Selecting
  either endpoint cross-selects the other so the user can inspect the logical
  net and its tapeout-facing route as one interface.
- `.chippy`, `.ochippy`, Physical IR, GDS labels, LEF pins, regeneration
  fingerprints, and round-trip validation preserve the assignments
  deterministically.

Acceptance:

- A qualification fixture assigns inputs and outputs across all four sides and
  round-trips without changing side, order, role, or logical net.
- Legalization produces no pin-to-pin, edge-keepout, grid, width, or access
  violations and rejects over-capacity sides with actionable diagnostics.
- Every assigned logical I/O has exactly one perimeter terminal on the same
  net; unassigned required I/O and orphan perimeter pins are reported.
- Moving internal placement or rerouting the design does not change fixed I/O
  assignments, while automatic offsets remain deterministic.
- Native DRC/LVS/connectivity, GDS export/import, and LEF export agree on the
  pin names, nets, directions, layers, and physical locations.

## MPV-6 — Qualification bundle and external process correlation

- Produce the complete qualification artifact set defined below.
- Run the official selected-process DRC/LVS flow and archive tool versions,
  logs, rule-deck hashes, and results.
- Add signing only after the evidence manifest is deterministic.

Acceptance:

- Native checks and official process checks both pass for QD-0001.
- Results are reproducible from the archived manifest.
- Claims advance only to the highest status supported by checked-in evidence.

---

# Process Rule Architecture

Process support is divided into two independent files.

## Process Rule Deck

Example:

[`examples/process_gf180mcu_3v3_5m_dr.yaml`](examples/process_gf180mcu_3v3_5m_dr.yaml)

Contains:

- process metadata
- layers
- routing layers
- derived layers
- device definitions
- physical rules
- rule identifiers
- technology constraints

This file defines **what the process requires**.

It does **not** contain validation status.

---

## Process Validation

Example:

[`examples/process_gf180mcu_3v3_5m_validation.yaml`](examples/process_gf180mcu_3v3_5m_validation.yaml)

Contains:

- implementation status
- qualification evidence
- manufacturing evidence
- silicon verification
- cryptographic signature

This file defines **what OpenChippy has proven**.

The checked-in example is intentionally conservative: it records the current
native unit-test and local-audit evidence, leaves unavailable artifact digests
empty, and marks GDSII, LVS, external signoff, tapeout, manufacturing, and
silicon verification as not run.

---

# Physical IR

The Physical IR is the canonical design database.

Everything generated by OpenChippy originates from it.

```
                Physical IR
              /     |      \
             /      |       \
         Native   GDSII     LEF
          DRC     Export    Export
           |
        Native LVS
```

No editing should occur directly on GDSII or LEF.

---

# GDSII Support

Unlike LEF, GDSII will support both export and import.

## GDSII Export

OpenChippy generates standards-compliant GDSII from the Physical IR.

Supported responsibilities:

- hierarchy
- polygons
- instances
- labels
- layer mapping
- transforms
- arrays
- database units

Process contracts may attach a non-negative enclosure to an individual GDS
purpose. This lets one Physical IR diffusion rectangle emit the electrically
active COMP polygon plus a larger polarity implant without distorting the
device model. Import resolves an ambiguous COMP polygon from the unique
companion implant that encloses it, so the same contract remains round-trip
safe.

The current GF180 compatibility contract additionally fixes contact cuts at
the foundry size and gives the 9K top-metal option its own width, spacing, and
area rules. These are export requirements, not native-DRC waivers.
Known older GF180 project snapshots are migrated to that complete contract on
load, and Physical IR v17 invalidates cached geometry generated with earlier
contact or routing rules. Custom process identities are never rewritten.

---

## GDSII Import

OpenChippy includes a native GDSII parser.

The parser enables:

- interoperability validation
- round-trip verification
- regression testing
- future third-party design import

Imported GDSII is converted back into Physical IR.

---

# GDSII Validation Levels

Validation occurs in multiple stages.

## 1. Structural Validation

Verify:

- valid record structure
- valid hierarchy
- valid coordinates
- valid layer/datatype pairs
- valid references
- valid transformations
- no malformed records

This verifies the file is structurally valid GDSII.

---

## 2. Round-Trip Validation

```
Physical IR
      ↓
GDSII Export
      ↓
GDSII Import
      ↓
Physical IR
```

Compare:

- hierarchy
- geometry
- instances
- pins
- labels
- transforms
- layer mapping
- connectivity

Comparison is based on canonical geometry rather than byte-for-byte file equality.

---

## 3. External Interoperability Validation

```
OpenChippy
      ↓
GDSII
      ↓
External Tool
      ↓
Re-export GDSII
      ↓
OpenChippy
```

Validate:

- external tool accepts file
- external tool exports valid GDSII
- OpenChippy imports external output
- geometry remains equivalent
- connectivity remains equivalent

This demonstrates standards compliance.

---

# LEF Support

LEF is generated from the Physical IR.

Initial implementation is **export only**.

Generated information includes:

- macro definitions
- pins
- obstructions
- routing layers
- symmetry
- site information
- cell dimensions

LEF exists as an interoperability artifact for external place-and-route flows.

It is **not** considered the authoritative database.

---

# Native Validation

OpenChippy performs validation directly on the Physical IR.

Validation includes:

- DRC
- LVS
- connectivity
- hierarchy
- device recognition
- manufacturing grid
- layer constraints

After GDSII export, the imported GDSII should produce identical validation results.

---

# Qualification Designs

Qualification designs exist to validate the physical implementation flow.

Example:

```
QD-0001
4B_ADDER
```

Future qualification designs:

```
QD-0002
Register File

QD-0003
UART

QD-0004
GPIO Matrix

QD-0005
Tiny CPU
```

Each design exercises different portions of the physical implementation engine.

---

# Qualification Flow

The qualification pipeline is:

```
RTL
    ↓
Physical IR
    ↓
Native DRC
    ↓
Native LVS
    ↓
GDSII Export
    ↓
Native GDSII Validation
    ↓
External Tool Validation
    ↓
External GDSII Export
    ↓
OpenChippy GDSII Import
    ↓
Round-Trip Comparison
    ↓
LEF Export
    ↓
Tiny Tapeout Submission
    ↓
Manufacturing
    ↓
Silicon Verification
```

---

# Qualification Coverage

Each qualification design records coverage.

Example:

```
Rules implemented

Rules reference matched

Rules exercised

Rules manufactured

Rules silicon verified

Layers exercised

Device types exercised
```

Coverage grows as additional qualification designs are added.

---

# Validation Status

Recommended progression:

```
DOCUMENTED

IMPLEMENTED

UNIT_TESTED

REFERENCE_MATCHED

TAPEOUT_EXERCISED

MANUFACTURED

SILICON_VERIFIED
```

Validation should always be tracked per rule and per capability.

---

# Cryptographic Validation

Validation records are cryptographically signed.

The signature certifies:

- process rule deck hash
- validation record
- qualification design
- Physical IR revision
- GDSII hash
- LEF hash
- OpenChippy version

Suggested signature:

```
Ed25519
```

---

# Qualification Artifacts

Each qualification should archive:

```
design.chippy

physical_ir.json

design.gds

design.lef

native_drc.json

native_lvs.json

gds_validation.json

round_trip_validation.json

external_validation.json

coverage.json

validation_manifest.yaml

validation_manifest.sig
```

---

# Definition of Passing QD-0001 (4B_ADDER)

A qualification is complete when all of the following succeed:

1. Physical IR generated successfully.
2. Native DRC passes.
3. Native LVS passes.
4. GDSII exports successfully.
5. Native GDSII structural validation passes.
6. GDSII imports successfully.
7. Imported Physical IR is equivalent to original.
8. External tool imports GDSII successfully.
9. External tool re-exports GDSII.
10. OpenChippy imports external GDSII.
11. Canonical geometry remains equivalent.
12. Connectivity remains equivalent.
13. LEF exports successfully.
14. Tiny Tapeout accepts generated artifacts.
15. Manufacturing succeeds.
16. Returned silicon passes functional testing.

---

# Guiding Philosophy

OpenChippy should never claim that an entire PDK is "validated."

Physical-synthesis experiments must also not be accepted or rejected from one
aggregate error count. A changed placement or route can expose a defect that an
earlier cleanup pass accidentally hid. Candidate comparison therefore records
the invariant classes independently:

- recognized versus expected devices and terminals
- shorts
- open logical nets and their island counts
- unrouted device terminals
- geometry, cut, enclosure, and spacing violations by rule
- orphan geometry removed
- routed area and shape count
- runtime

Terminal reachability is necessary but is not route closure by itself. Every
axial endpoint of generated metal must also terminate on compatible same-net
metal, a legal via transition, a device contact, or an external pin. A branch
that is attached to an otherwise valid net but ends in empty space is a
`CONNECTIVITY.DANGLING_ROUTE_ENDPOINT` failure, not an acceptable visual
artifact. Endpoint diagnostics must identify the net, layer, shape, and
coordinate so cleanup can shorten or remove the branch transactionally without
masking a genuine open.

Physical IR v37 strengthens this rule into a per-terminal obligation proof.
Every logical MOS gate, source, and drain must bind to its exact persisted
access geometry and then reach either another required terminal, a persisted
boundary pin, or an explicit power-distribution rail. A contact, landing, or
via stack by itself is not evidence of closure. Shared diffusion and shared
poly remain legal, but only when the persisted topology geometry proves the
claimed connection between all member devices.

This stricter audit exposed a real generator defect in the simple inverter
qualification: its local VDD/GND terminal geometry was not always connected to
the distributed power fabric even though the older net-level report passed.
Power rails are now explicit Physical IR shapes, participate in repair and
orphan pruning, and serve as the external obligation for supply nets. New
diagnostics distinguish missing terminal mappings, isolated one-terminal nets,
and open terminal obligations. The canonical GF180 4B_ADDER passes the stronger
native DRC/LVS audit, but visual route cleanup remains active: terminal-free
shootouts, redundant transition stacks, and unnecessary long branches must
continue to be compacted rather than hidden by aggregate pass counts.

Correct device extraction and zero shorts are hard gates. Open connectivity and
manufacturing-rule diagnostics remain explicit defects, but a pass that exposes
them is retained as diagnostic evidence rather than automatically reverted.
Area and runtime are optimization criteria only after electrical and geometric
integrity can be compared clearly.

Instead, OpenChippy should make evidence-based claims:

> "These specific process rules, physical implementation capabilities, GDSII generation, GDSII interoperability, LEF generation, manufacturing workflows, and silicon qualification steps have been independently verified."

The ultimate goal is to build confidence incrementally through measurable qualification designs, culminating in successful fabrication and verified silicon.
