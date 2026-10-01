# OpenChippy

OpenChippy is an open source desktop workspace for learning how digital integrated
circuits move from transistor schematics to simulation and physical layout.

It brings schematic editing, reusable circuit blocks, waveform inspection, basic design
checks, RTL import, and a layered 3D layout view into one application. The long-term goal
is to make open chip-design tools feel connected and approachable without hiding how the
design works.

OpenChippy is still under active development. It is useful for education and experiments,
but it is not yet a replacement for a foundry-qualified signoff flow, a full analog
simulator, or production place-and-route software.

## What you can do today

- Draw CMOS schematics with NMOS and PMOS transistors, power and ground, digital inputs,
  output probes, net labels, junctions, resistors, and routed wires.
- Pan, zoom, rotate parts, select multiple objects, continue dangling wires, and save or
  reopen your work.
- Build reusable blocks, including nested blocks, and use them in larger designs.
- Run basic circuit checks for missing power, floating connections, shorts, broken block
  pins, and other common schematic mistakes.
- Simulate transistor-level digital behavior and inspect high, low, floating, contended,
  or unknown signals.
- Generate truth tables and timed waveforms. Waveforms support cursors, zoom, scalar
  signals, and user-defined binary or hexadecimal buses.
- Load a technology YAML file or use the included five-metal educational process. See
  [the example technology file](docs/examples/openchippy-edu-5m.yaml).
- Physical IR snapshots include a packaged 24-cell library for the educational
  process and GF180MCU compatibility profile. It covers common logic gates,
  inverter/buffer, transmission gate, latches and flip-flops, multiplexers, and
  decoders. GF180 output still requires official foundry DRC/LVS.
- Generate a compact physical layout, inspect process layers in 3D, run physical design
  checks, isolate violations, and export a report.
- Import synthesizable Verilog/SystemVerilog through Yosys or slang, inspect its logical structure,
  simulate supported designs, and export the preserved RTL again.

## Project files

New projects save as an `.ochippy` manifest. It lists the files that belong to the project:

- `.chippy` contains the editable circuit and logical design.
- `.chippy_gds` contains the generated physical layout used by the 3D viewer.
- `chippyblocks/` contains reusable `.chippyblock` files stored beside the project.

The cached physical file is tied to the circuit and effective technology contract it came
from. If they no longer match, OpenChippy refuses to display the stale layout. A matching
cache lets the 3D view reopen without repeating placement and routing. Existing standalone
`.chippy` files remain supported, including narrowly identified legacy GF180 snapshots whose
missing packaged manufacturing mappings can be restored safely.

`.chippy_gds` is OpenChippy's versioned physical-layout cache. The 3D workspace
can separately export an initial binary `.gds` stream and validates its record
structure and geometry counts before saving. GDS layer/datatype assignments
come from the active process deck; GF180 diffusion expands into COMP plus its
polarity-specific implant and excludes the synthetic substrate preview. The
exporter remains an early interoperability path rather than a signoff replacement.
Physical blocks are emitted as referenced GDS structures, although shared canonical cells
and non-zero transforms are still under development. The exact GF180 four-bit-adder
qualification design has deterministic GDS output, clean native DRC/connectivity/LVS, and
no geometry or process findings under the official variant-C 5LM/9K KLayout deck; the deck's
direct floating-point DBU comparison still produces a known tool-version-specific marker.
Broader process coverage, extraction correlation, reusable hierarchy qualification, and
fabricated-silicon evidence remain manufacturing-roadmap work.
Physical power routing is distributed across occupied placement rows instead
of being forced through a single VDD/GND pair; devices use the nearest matching
rail, leaving upper routing capacity available for signal closure.
For designs built from reusable blocks, the physical planner is gaining a
standard-cell path. Its staging implementation gives identical blocks
canonical device-relative placement, typed rectangular keepouts, and
next-hop-aware ordering. It is not yet enabled in production generation while
pre-routed local geometry and explicit boundary-pin templates are completed.
Nested blocks are resolved from the bottom up: a full adder built from reusable
NANDs identifies each NAND as the physical cell, while the full-adder boundary
remains available for higher-level floorplanning.

Process YAML may select a packaged physical library with
`standard_cell_library`. The identifiers currently packaged are `openchippy-edu`
and `gf180mcu-3v3-5m`. After hierarchy is flattened, OpenChippy recognizes exact
CMOS `INV`, `NAND2`, and `NOR2` connectivity inside the lowest reusable block and
records the selected library cell in Physical IR. Unrecognized topology continues
through transistor-level physical generation; recognition never relies on block or
device names.

The 3D workspace can also export an early LEF macro view containing the
Physical IR dimensions, input/output/power pin rectangles, symmetry, and metal
obstructions. Process-derived placement-site dimensions are included, and the
GF180 four-bit-adder example parses in KLayout with matching bounds and all
physical pin labels. Broader tool qualification remains in progress.

## RTL support and limitations

OpenChippy can elaborate synthesizable Verilog with Yosys and broader SystemVerilog
with the Yosys slang plugin. In **Import Verilog**, select the compiler, optionally
enter the top module, and set the Yosys executable path. The path is remembered on
this computer. `OPENCHIPPY_YOSYS` or a `yosys` executable on `PATH` also works.
Auto uses Yosys when available and otherwise uses the built-in subset parser.
Compilers are installed separately; an OSS CAD Suite installation with slang is
suitable for the SystemVerilog option.

Compiler-backed import supports linked modules in one source, parameters,
preprocessor macros, generate loops, functions, signed arithmetic, variable
selection, and synthesizable memories. Registers include enables, synchronous
and asynchronous resets, simultaneous nonblocking updates, and synthesized
initial values. Slang additionally handles synthesizable packages, packed
structs, and `always_comb`. Imported logic can be inspected and simulated;
export preserves the original source exactly.

The built-in parser remains available for small designs without an external
compiler. It supports one module, fixed vectors, primitive gates, assignments,
and simple combinational or single-assignment clocked processes.

Limits remain explicit: this is a hardware elaboration flow, not an unrestricted
Verilog testbench simulator. Delays, classes, force/release, arbitrary file I/O,
tri-state/inout ports, latches, and cells without simulator models are unsupported.
Source bundles currently use one text file; there is no project file-list or
include-directory UI. Compiler support does not yet convert RTL into transistor
schematics or manufacture-ready layout. Imports are bounded to 16 MiB source,
100,000 cells, and a 120-second compiler run.

Compiler integration regressions can be run with:

```sh
OPENCHIPPY_YOSYS=/path/to/yosys cargo test --manifest-path src-tauri/Cargo.toml rtl_compiler -- --include-ignored
```

## Build from source

### Prerequisites

- Node.js 20 or newer
- The stable Rust toolchain from [rustup](https://rustup.rs/)
- The platform tools required by
  [Tauri 2](https://v2.tauri.app/start/prerequisites/)

On macOS, install Xcode Command Line Tools if needed:

```sh
xcode-select --install
```

### Install dependencies

```sh
git clone https://github.com/skerr92/OpenChippy.git
cd OpenChippy
npm install
```

### Run the desktop application

```sh
npm run tauri dev
```

The browser-only frontend can be started with `npm run dev`, but saving, loading,
simulation, design checks, technology files, and physical generation require the desktop
application and Rust backend.

### Build the latest release executable

```sh
npm run tauri -- build
```

This repository currently leaves automatic installer bundling disabled. The release
executable is written to `src-tauri/target/release/` on the current platform.

### Build an Apple Silicon app and DMG

Run this on an Apple Silicon Mac:

```sh
rustup target add aarch64-apple-darwin
npm run tauri -- build \
  --target aarch64-apple-darwin \
  --bundles app,dmg \
  --config '{"bundle":{"active":true}}' \
  --no-sign
```

The unsigned local packages will be under:

```text
src-tauri/target/aarch64-apple-darwin/release/bundle/macos/OpenChippy.app
src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/
```

`--no-sign` is suitable for local testing. A DMG intended for other users should be signed
with an Apple Developer ID certificate and notarized before distribution.

### Validation

```sh
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri -- build
```

The current baseline is 193 passing Rust tests plus successful frontend and Tauri release
builds. Some physical-layout regressions are intentionally heavyweight and may take more
than a minute on a development machine.

## Contributing

Contributions and bug reports are welcome. The Rust backend owns saved project data,
simulation, validation, and physical-layout generation. React, TypeScript, SVG, and
Three.js provide the desktop interface and visualization.

Please open an issue when proposing a large feature so it can be matched to the roadmap
and file-format compatibility requirements.
