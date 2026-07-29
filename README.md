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
- Import a useful, deliberately limited subset of Verilog, inspect its logical structure,
  simulate supported designs, and export the preserved RTL again.

## Project files

New projects save as an `.ochippy` manifest. It lists the files that belong to the project:

- `.chippy` contains the editable circuit and logical design.
- `.chippy_gds` contains the generated physical layout used by the 3D viewer.
- `chippyblocks/` contains reusable `.chippyblock` files stored beside the project.

The cached physical file is tied to the circuit it came from. If they no longer match,
OpenChippy refuses to display the stale layout. A matching cache lets the 3D view reopen
without repeating placement and routing. Existing standalone `.chippy` files remain
supported.

`.chippy_gds` is OpenChippy's versioned physical-layout cache. The 3D workspace
can separately export an initial binary `.gds` stream and validates its record
structure and geometry counts before saving. GDS layer/datatype assignments
come from the active process deck; GF180 diffusion expands into COMP plus its
polarity-specific implant and excludes the synthetic substrate preview. The
exporter is still early: complete reusable hierarchy, external KLayout
validation, and foundry signoff remain manufacturing-roadmap work. Physical
blocks are now emitted as referenced GDS structures, although shared canonical
cells and non-zero transforms are still under development.
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

The 3D workspace can also export an early LEF macro view containing the
Physical IR dimensions, input/output/power pin rectangles, symmetry, and metal
obstructions. Process-derived placement-site dimensions are included, and the
GF180 four-bit-adder example parses in KLayout with matching bounds and all
physical pin labels. Broader tool qualification remains in progress.

## RTL support and limitations

OpenChippy's RTL importer is intentionally smaller than a full Verilog or SystemVerilog
compiler. It is designed to preserve supported source clearly and reject unsupported code
instead of silently changing its meaning.

Currently supported:

- One module per imported source.
- ANSI and classic module ports, scalar signals, and fixed packed vectors.
- Built-in gates such as `and`, `or`, `xor`, `nand`, `nor`, `xnor`, `not`, and `buf`.
- Continuous assignments with a bounded set of arithmetic and bitwise expressions.
- Integer parameters, parameter-based vector widths, and parameter overrides on referenced
  cells.
- Constant bit selection, concatenation, and constant or parameter-counted replication.
- Simple `always_comb` and `always @*` logic with bounded blocking assignments,
  `if`/`else`, and `case` statements.
- Simple edge-triggered `always` and `always_ff` registers with one nonblocking assignment.
- Waveform simulation for the supported combinational and sequential forms.

Not yet supported:

- Multi-module source bundles or descending into linked child-module definitions.
- General SystemVerilog syntax, interfaces, packages, classes, or advanced types.
- `generate`, `defparam`, arbitrary constant functions, or full parameter elaboration.
- General part selects, signed arithmetic rules, shifts, unpacked arrays, and memories.
- Clocked blocks with reset/enable branches, multiple statements, or mixed assignment
  styles.
- `initial` blocks, delays, tasks, `force`, or unrestricted testbench code.
- Converting imported RTL directly into transistor schematics or final physical layout.

These items are tracked in Milestone 7 alongside Yosys and ABC synthesis integration. See
the [roadmap](docs/roadmap.md) for the planned order.

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

The current baseline is 121 passing Rust tests plus successful frontend and Tauri release
builds.

## Contributing

Contributions and bug reports are welcome. The Rust backend owns saved project data,
simulation, validation, and physical-layout generation. React, TypeScript, SVG, and
Three.js provide the desktop interface and visualization.

Please open an issue when proposing a large feature so it can be matched to the roadmap
and file-format compatibility requirements.
