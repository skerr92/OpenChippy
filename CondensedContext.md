# CondensedContext

## Context Freshness

- Context ID: FRESH-001
- Last Verified Commit: `2b9a6cad6f0dcf30432dad86e7c2809865a682a1`
- Current HEAD: `2b9a6cad6f0dcf30432dad86e7c2809865a682a1`
- Generated: 2026-08-11
- Status: partial; Physical IR v38-v45 is committed and verified, while the two CondensedContext files await their tracking commit.
- Files requiring verification: `CondensedContext.md` and `CondensedContext.CCF1`.
- Full semantic memory: `CondensedContext.CCF1`

## Current Focus

- Context ID: ACTIVE-001
- Confidence: High for the current working tree and recorded qualification results; source remains authoritative.
- Committed Physical IR v45 tightens terminal identity and cell-bounded FEOL sharing, removes unnecessary topology-internal access stacks, restores process-owned GF180 dummy fill for legacy projects, exposes per-material fill in audits and the 3D viewer, and repairs a foundry-width M1 route-to-landing neck.
- Exact legacy GF180 4B adder qualification is recorded as native DRC 0, 144/144 devices, 82/82 nets, no opens or shorts, deterministic valid GDS, and official variant-C 5LM/9K DRC with no geometry/process findings; only the known KLayout/Ruby DBU comparison marker remains.
- The next physical-quality slice is sliding-window density closure with minimum/preferred/maximum targets and timing-aware keepouts. Route/path compaction and transition-stack cleanup remain worthwhile but secondary to preserving the now-clean manufacturing baseline.

## Handoff

- Context ID: HANDOFF-001
- Last known state: Physical IR v38-v45 and refreshed README/manufacturing documentation are committed at `2b9a6ca`; only the context tracking update remains uncommitted.
- Next useful step: commit this context tracking update, then implement sliding-window density analysis and preferred-target fill from the clean checkpoint.
- Validation: `npm run build` passes on 2026-08-11 with only the existing bundle-size warning. The stable toolchain's direct cargo binary passes all 184 Rust unit tests; Cargo's subsequent empty doc-test phase hung and was stopped. Historical v45 evidence records clean native DRC/connectivity/LVS, deterministic GDS, and official GF180 geometry/process DRC closure.

## Recent Changes

| Date | Tags | Change | Commit | Remote |
| --- | --- | --- | --- | --- |
| 2026-08-04 | physical-ir-v45, gf180, m1-neck | Repaired short zero-gap route-to-landing junctions without introducing concave-corner violations; exact GF180 4B official geometry/process DRC is clean. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v44, density-fill, legacy-gf180 | Required fill on every configured material, exposed per-layer counts and viewer controls, and consistently migrated narrowly identified legacy GF180 snapshots. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v43, generation-contract, cache | Applied GF180 manufacturing migration at generation/digest boundaries and rejected configured processes that emit no dummy fill. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v42, topology-drc | Made terminal-obligation DRC understand topology-internal shared active and hydrated missing canonical GF180 manufacturing contracts. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v41, internal-access, viewer | Removed routing/access stacks for nets fully closed inside proven shared-active islands and exposed synthetic top-metal fill in 3D. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v40, cell-boundary | Restricted shared diffusion/poly transformations to devices in the same explicit leaf standard-cell instance. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v38-v39, route-pruning, terminal-identity | Fragmented routes at real junctions for finer pruning and retired same-net pseudo-terminal coalescing. | `2b9a6ca` | Not confirmed |

## Open Threads

- Implement process-configurable sliding-window density analysis and fill targeting; report underfilled, overfilled, and worst-density windows per layer.
- Preserve timing, clock, power, antenna, and device keepouts while targeting preferred density rather than maximum occupancy.
- Revisit redundant route branches, transition stacks, and path-length compaction only with DRC/connectivity/LVS/determinism gates intact.
- The broader roadmap still defers full RTL synthesis/tool integration, linked multi-module RTL, and production signoff claims; use `docs/roadmap.md` and `docs/manufacturing_roadplan.md` for scope.
