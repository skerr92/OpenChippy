# OpenChippy alpha-v1 — 0.1.0-alpha.1

An experimental educational desktop release for drawing CMOS circuits, building
reusable blocks, simulating digital behavior, importing bounded RTL, and exploring
generated physical layouts. Source branch: `alpha-v1`.

## Included changes

- Schematic editing, nested reusable blocks, truth tables, and waveforms.
- Built-in RTL parsing plus optional Yosys/slang compiler-backed import.
- Experimental physical layout, native DRC/LVS, GDS and LEF exports.
- Background-worker Save/Load/Save As, including block refresh and cache checks.
- Save As project isolation and guards against stale save/load results.
- Dummy-fill import repair and refreshed process validation fingerprints.
- Reproducible EDU/GF180 small-cell physical checks with deterministic exports.

## Local macOS alpha package

The locally verified build target is Apple Silicon macOS (arm64). Extract the
ZIP, copy `OpenChippy.app` to Applications, and open it. This local alpha package
is unsigned and not notarized; macOS may require approval in Privacy & Security
before opening it. No signing credentials are included.

Example inverter and NAND2 circuits, a compiler-free RTL inverter, this document,
and the project license are included in the application resources. On macOS,
use Show Package Contents and look under `Contents/Resources/examples`, then copy
examples to a writable folder before editing them. Yosys/slang is installed
separately when compiler-backed import is needed.

## Validation and known limits

- The versioned alpha passed 210 release-mode Rust tests (six external-tool/fixture
  tests ignored), doc tests, frontend compilation, and a macOS application-bundle
  build. Bundled examples, license, release notes, version metadata, and archive
  checksums are checked by the packaging script.
- User confirmed the earlier release editor rendered correctly. Full interactive
  save/load, dialogs, shortcuts, blocks, RTL and export acceptance remains open,
  including responsiveness on Windows. The updated package is not yet visually
  accepted. See `docs/desktop-acceptance.md` in the source distribution.
- Four small-cell EDU/GF180 fixture cases pass native checks and deterministic
  GDS regeneration. Hierarchical-adder acceptance remains incomplete.
- Physical layout and process profiles are experimental. GF180 fine fill remains
  core-scoped; full-die density closure and current-design official foundry
  qualification are incomplete. Historical clean results do not qualify this
  generator or arbitrary designs. Do not use this alpha as a signoff flow.
- RTL-to-transistor synthesis, source file-list/include-directory UI, and broader
  simulation models remain future work. Optional compiler integration tests were
  not run in this local release audit.
- Linux CI is configured but its remote result was not verified here. Windows
  and Intel macOS release artifacts are not supplied by this local package.
- Large layouts may be slow. The frontend bundle-size advisory remains. The
  local Rust toolchain emits a nonfatal debug-stripping warning.

This release's source remains under the repository's GNU GPL v3 license; see
`LICENSE`. Preserve these release notes and validation context when moving the
documentation to another repository or publishing outside GitHub.
