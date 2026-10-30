# Alpha-v1 release checklist

Scope: an educational desktop alpha with schematic editing, reusable blocks,
digital simulation, bounded RTL import, and experimental physical layout.
Manufacturing signoff and complete RTL-to-transistor synthesis remain later work.

| Priority | Item | Release evidence |
| --- | --- | --- |
| P0 | Reproducible physical regression checks | Repository-owned source fixtures; strict native DRC/LVS and GDS checks; deterministic regeneration; negative-case rejection; CI evidence retained. Small-cell coverage is the first slice; a hierarchical adder fixture remains open. |
| P0 | Desktop release acceptance | macOS arm64 release build and 210 release-mode Rust tests pass. User confirmed release editor rendering; remaining interactive checks stay open. Follow [desktop acceptance](desktop-acceptance.md) for open/save/Save As, blocks, simulation, RTL, layout, exports, and cancellation on each advertised platform. |
| P1 | Sequential simulation and compiler coverage | Explicit DFF waveform acceptance; Yosys/slang integration tests run with recorded tool versions; unsupported constructs yield useful errors. |
| P1 | Physical limits and diagnostics | Current-layout native results are distinguished from historical official evidence; core-only density and experimental exports are clearly identified; failed generation remains inspectable. |
| P1 | Packaging and release documentation | Version 0.1.0-alpha.1 aligned across manifests; local unsigned macOS arm64 app bundle includes examples/license/release notes. Source archive and checksum packager added. Publication destination remains unconfirmed. |
| P2 | Performance and interface polish | Profile larger designs; address major stalls and verify navigation. Bundle-size tuning follows measured impact. |

Native physical evidence can be reproduced with the commands in
[physical-qualification.md](physical-qualification.md). This does not establish
official foundry DRC/LVS or full-die density closure. Those remain requirements
for stronger manufacturing claims, rather than prerequisites for an explicitly
experimental educational alpha.
