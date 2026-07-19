# OpenChippy
OpenChippy is an education-first, open source desktop environment for transistor-level
integrated-circuit design, switch simulation, validation, and physical-layout visualization.

### About OpenChippy

OpenChippy bridges the gap between integrated commercial environments such as Cadence
Virtuoso and powerful but disjointed open source tools. It provides one coherent workspace
for learning and prototyping CMOS circuits from the transistor level upward, while keeping
the authoritative project, technology, connectivity, simulation, and physical-layout models
in Rust.

The project is deliberately growing in validated milestones. OpenChippy is not yet a
foundry-qualified replacement for signoff tools, a SPICE-compatible analog simulator, or a
production place-and-route system. Its current focus is a sound, inspectable foundation:
schematic intent, educational switch behavior, reusable hierarchy, and a compact
process-aware physical representation that can later feed shared 2D/GDS and 3D views.

### Capabilities

The current Milestone 0–3 implementation includes:

* A Tauri desktop project workflow with new, open, save, save-as, rename, dirty-state
  tracking, and undo/redo.
* A pan-and-zoom 2D CMOS schematic editor with NMOS, PMOS, VDD, GND, digital inputs,
  output probes, junctions, net labels, resistors, rotatable and multi-selectable
  components, and editable orthogonal or intentionally dangling wires.
* Rust-owned connectivity DRC for missing power, floating terminals, broken connections,
  duplicate names, dangling routes, shorted rails, and invalid reusable-block interfaces.
* A five-state educational CMOS switch solver (`HIGH`, `LOW`, `FLOATING`, `CONTENDED`,
  and `UNKNOWN`) with supply and threshold awareness, geometry-derived resistance and
  capacitance, path resistance, and `0.69RC` delay estimates.
* Interactive operating-point visualization, generated truth tables, and a bounded
  GTKWave-inspired timed waveform view.
* Versioned technology YAML with validated NMOS/PMOS characteristics and an explicit
  process routing-layer ceiling. The built-in educational process provides five metals.
* A Rust-normalized physical-layout IR independent of schematic drawing coordinates,
  aspect-ratio-aware folded CMOS row banks, diffusion/poly/contact geometry,
  process-bounded direction-separated routing through a reserved signal channel,
  stacked vias, and a broadly zoomable Three.js layer view.
* Reusable device blocks captured from transistor-level or block-composed circuits with
  promoted input, output, VDD, and GND pins; compact shared instances; arbitrary-depth,
  cycle-checked nesting; project persistence and undo/redo; automatic portable
  `.chippyblock` storage and discovery in a project-adjacent `chippyblocks/` folder; and
  deterministic hierarchy flattening for DRC, simulation, truth tables, waveforms, and
  physical generation.

The Rust suite currently validates 52 tests covering project history, model
compatibility, technology files, CMOS behavior, timing, hierarchy, DRC, and physical
routing. The TypeScript/Vite production build and packaged Tauri release build are also
part of the milestone validation workflow. Detailed scope and future work live in the
[roadmap](docs/roadmap.md).

### contribution

Contributors are welcome. The backend is intentionally Rust-first for durable models,
serialization, analysis, and memory safety. The desktop interface uses React, TypeScript,
HTML/CSS, SVG, and Three.js so the same authoritative backend data can support approachable
interactive views.

### questions?

Please open an issue if you have any questions.

### Development

The current foundation is a Tauri 2 desktop application with a Rust backend and a
React, TypeScript, and Three.js frontend.

Prerequisites:

* Node.js 20 or newer
* The stable Rust toolchain
* The platform prerequisites listed in the
  [Tauri setup guide](https://v2.tauri.app/start/prerequisites/)

Install dependencies and start the desktop application:

```sh
npm install
npm run tauri dev
```

For frontend-only development in a browser:

```sh
npm run dev
```

The full editor, save/load workflow, simulation, DRC, technology loading, reusable blocks,
and physical generation require the desktop application. The browser build is useful for
frontend development but does not replace the Rust backend.

Useful validation commands:

```sh
npm run build
cd src-tauri && cargo test
npm run tauri build
```
