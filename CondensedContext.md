# CondensedContext

## Context Freshness

- Context ID: FRESH-001
- Last Verified Commit: `34d386a9396635b6569f0e372bbd23e17027d521`
- Current HEAD: `34d386a9396635b6569f0e372bbd23e17027d521`
- Generated: 2026-08-11
- Status: partial; Physical IR v46 is committed and verified, while the two CondensedContext files await their tracking commit.
- Files requiring verification: `CondensedContext.md` and `CondensedContext.CCF1`.
- Full semantic memory: `CondensedContext.CCF1`

## Current Focus

- Context ID: ACTIVE-001
- Confidence: High for the current working tree and recorded qualification results; source remains authoritative.
- Physical IR v46 adds backward-compatible global/window density limits, boolean-union coverage measurement, half-window sliding evaluation with edge anchoring, preferred-target fill, hard maximum admission, and persisted per-layer coverage reports for the viewer and headless audit.
- Existing circuit/device spacing remains a hard fill keepout. Dedicated critical-net, clock/power, antenna, and coupling-aware keepout classes are intentionally deferred until the process contract can express non-invented distances.
- The committed v45 exact legacy GF180 4B baseline remains native DRC/connectivity/LVS and official geometry/process DRC clean; v46 still needs exact-design regeneration and official-deck correlation before inheriting that external qualification claim.

## Handoff

- Context ID: HANDOFF-001
- Last known state: v38-v45 is committed at `2b9a6ca`, compact context at `0afe816`, and validated v46 density closure is committed at `34d386a`.
- Next useful step: commit this context tracking update, then regenerate the exact GF180 4B design and correlate its persisted density report and GDS against the official deck before adding process-owned protected-net keepout classes.
- Validation: `cargo test --lib` passes 185/185 on 2026-08-11; `npm run build` passes with only the existing bundle-size warning; `cargo fmt --check`/`git diff --check` are clean. Focused tests prove deterministic inert fill, zero configured under/overfilled windows, maximum admission, and density-contract validation.

## Recent Changes

| Date | Tags | Change | Commit | Remote |
| --- | --- | --- | --- | --- |
| 2026-08-11 | physical-ir-v46, density, sliding-window | Added process-configurable global/window minima, preferred targets and maxima; boolean-union measurement; edge-anchored half-window closure; persisted audit/viewer reporting; and validation coverage. | `34d386a` | Not confirmed |
| 2026-08-04 | physical-ir-v45, gf180, m1-neck | Repaired short zero-gap route-to-landing junctions without introducing concave-corner violations; exact GF180 4B official geometry/process DRC is clean. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v44, density-fill, legacy-gf180 | Required fill on every configured material, exposed per-layer counts and viewer controls, and consistently migrated narrowly identified legacy GF180 snapshots. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v43, generation-contract, cache | Applied GF180 manufacturing migration at generation/digest boundaries and rejected configured processes that emit no dummy fill. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v42, topology-drc | Made terminal-obligation DRC understand topology-internal shared active and hydrated missing canonical GF180 manufacturing contracts. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v41, internal-access, viewer | Removed routing/access stacks for nets fully closed inside proven shared-active islands and exposed synthetic top-metal fill in 3D. | `2b9a6ca` | Not confirmed |
| 2026-08-04 | physical-ir-v40, cell-boundary | Restricted shared diffusion/poly transformations to devices in the same explicit leaf standard-cell instance. | `2b9a6ca` | Not confirmed |

## Open Threads

- Add process-owned spacing classes for critical nets, clocks/power, antenna risk, and coupling-sensitive geometry; existing generic circuit/device keepouts remain enforced.
- Requalify exact GF180 4B v46 output with native DRC/connectivity/LVS, deterministic GDS, persisted density metrics, and the official deck.
- Revisit redundant route branches, transition stacks, and path-length compaction only with DRC/connectivity/LVS/determinism gates intact.
- The broader roadmap still defers full RTL synthesis/tool integration, linked multi-module RTL, and production signoff claims; use `docs/roadmap.md` and `docs/manufacturing_roadplan.md` for scope.
