# Desktop alpha acceptance

Run these checks against the release executable on every advertised platform.
Rust tests and a frontend build alone do not exercise native dialogs, keyboard
shortcuts, WebView rendering, or the packaged application.

## Build and evidence

```sh
npm ci
npm run tauri -- build
cargo test --release --manifest-path src-tauri/Cargo.toml
```

Installer bundling is currently disabled. The executable is
`src-tauri/target/release/openchippy` (`openchippy.exe` on Windows).
Record the operating system/architecture, source commit and working-tree status,
executable SHA-256, and the result of each check below. Use a fresh scratch folder
and copies of the example files; preserve the repository fixtures.

## Interactive checks

| Check | Steps | Expected result |
| --- | --- | --- |
| Startup | Launch the release executable. Resize the window. | Editor and library render; controls remain usable; no startup failure. |
| Open and simulate | Open a copy of `docs/examples/qualification/inverter.chippy`. Run **Truth Table**; apply both input rows. | LOW input produces HIGH output; HIGH produces LOW; the schematic shows the corresponding states. |
| Save and reopen | Save As `inverter.ochippy` in the scratch folder. Rename or move a component, Save, then reopen the manifest. | Circuit and edits survive; the saved workspace is clean. Ordinary Save also works through Cmd+S on macOS or Ctrl+S elsewhere. |
| Save As isolation | Save As `inverter-copy.ochippy`; edit and save the copy, then reopen the original. | Copy has the changes; original project and source fixture remain unchanged. |
| Unsaved changes and history | Make an edit; Undo and Redo it. Attempt New/Open with unsaved work and cancel the discard prompt. | History restores the expected edit; cancel keeps the current project and unsaved work. |
| Reusable blocks | Open a fresh copy of `nand2.chippy`, use **Save as Block**, place the block in a new project and wire its pins. Save and reopen. | Instance, definition, pins, and wiring persist; truth table is NAND (only HIGH/HIGH yields LOW). |
| Native RTL import | Import `docs/examples/rtl/alpha-inverter.v` with the built-in frontend; inspect the preview, accept, and run Truth Table. Export Verilog, then import the export into a new project. | Import, logical view, inversion truth table, and export/reimport succeed without an external compiler. Cancelling an import leaves the current project intact. |
| Physical view and exports | Reopen the transistor inverter; generate the physical view. Fit, pan, zoom, toggle layers, run physical DRC, export GDS/LEF/report, then save and reopen. | View renders and navigates; native DRC has zero errors for the fixture; exports are nonempty; the saved matching layout reopens. |
| Large-project I/O responsiveness | Open and save a large project on the target platform (especially Windows). During disk work, move/resize the window. | Window remains responsive; save/load completes without losing edits. Automated worker round-trip coverage supplements this check. |
| Dialog cancellation | Cancel Open, Save As, RTL selection, and an export dialog. | Current project, file destination, and edits remain intact; no false success or crash. |

Yosys/slang integration is a separate acceptance item. A successful native
physical check also does not establish official foundry qualification.

## Current evidence

October 19, 2026 local audit:

| Evidence | Result |
| --- | --- |
| Platform | macOS 27.0.1 (26A434), Apple Silicon arm64 |
| Source | `31edfe28694f97278bba21ce01f6126e52ff8dba` plus uncommitted alpha work; this hash alone does not identify the built working tree |
| Tauri release build | PASS, including TypeScript/Vite production assets |
| Release Rust tests | PASS: 210 passed, six external-tool/fixture tests ignored; doc tests passed |
| Executable SHA-256 | `e93e2ba5bc6fa162e65285fe6079f341b5d34d7b8f48ca13f4e6c6b4e2c05137` |
| Release physical fixtures | PASS: four EDU/GF180 cases, deterministic regeneration, invalid geometry and process mismatch rejected |
| Process launch and editor rendering | PASS for the earlier executable: user confirmed the editor renders correctly; worker-refactor build needs a fresh interactive check |
| Remaining interactive rows above | NOT RUN: resizing, workflows, dialogs, and shortcuts still need confirmation |

The local Rust toolchain could not strip debug information because its
`rust-objcopy` could not load `libLLVM.dylib`. The build succeeded with warnings;
this does not establish a working installer or distribution signature.
Linux/Windows release acceptance is not established by this local macOS build.
See `CondensedContext.md` for the latest validation and outstanding checks.

Save/load worker refactor: complete workflows now run on blocking workers;
block refresh and cache hashing occur outside the workspace lock. Interactive
responsiveness must be rechecked on the updated executable, including Windows.
