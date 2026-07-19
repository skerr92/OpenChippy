# OpenChippy Roadmap

> **Mission:**
> Build an open-source, educational-first silicon design studio that bridges the gap between transistor physics and manufacturable integrated circuits through an intuitive, unified design experience.

---

# Guiding Principles

OpenChippy is **not** intended to replace Cadence Virtuoso, Siemens Calibre, or Synopsys.

Instead, it aims to become the **Blender or KiCad of VLSI**, providing an integrated learning environment while remaining compatible with professional and open-source design flows.

Core principles:

* Education first
* Unified workflow
* Visual understanding
* Open architecture
* Extensible plugin system
* Compatible with existing EDA tooling
* Desktop-first with future WebAssembly support

---

# Long-Term Vision

OpenChippy should allow a designer to seamlessly navigate between every abstraction layer of chip design.

```text
Architecture
      ↓
RTL
      ↓
Logic Gates
      ↓
Standard Cells
      ↓
Individual Transistors
      ↓
Physical Layout
      ↓
3D Process Visualization
```

Every object should remain connected across these layers.

Clicking on a gate should allow the user to inspect:

* RTL implementation
* transistor implementation
* timing
* power
* routing
* layout
* waveform
* physical geometry

---

# Architecture

```
Frontend
├── Tauri Desktop
├── React
├── Three.js
└── Property Panels

Backend (Rust)
├── Project Model
├── Circuit Graph
├── Simulation Engine
├── Validation Engine
├── File Serialization
└── Plugin API

Future
├── WebAssembly
├── Mobile Viewer
└── Cloud Collaboration
```

---

# Milestone 0 — Project Foundation

## Goal

Create a stable desktop application architecture.

### Deliverables

* Tauri application
* Rust backend
* React frontend
* Three.js viewport
* Save / Load project
* Undo / Redo framework
* Plugin architecture skeleton

### Success Criteria

* Open application
* Place placeholder component
* Save project
* Reload project

---

# Milestone 1 — Transistor Schematic Editor

## Goal

Build a functional transistor editor capable of creating CMOS circuits.

### Components

* NMOS
* PMOS
* VDD
* GND
* Digital Input
* Output Probe
* Wire
* Junction
* Net Label

### Editor Features

* Grid snapping
* Orthogonal wire routing
* Rotate components
* Move components
* Delete
* Multi-select
* Undo / Redo
* Zoom / Pan

### Validation

Detect:

* Floating terminals
* Shorted power rails
* Duplicate names
* Disconnected nets
* Missing power

### Success Criteria

A user can build:

* CMOS inverter
* NAND gate

without editing any files manually.

---

# Milestone 2 — Switch-Level Simulation

## Goal

Simulate CMOS transistor networks using an educational switch model.

### Initial Model

NMOS

```
Gate HIGH
→ conducting
```

PMOS

```
Gate LOW
→ conducting
```

### Output States

* HIGH
* LOW
* FLOATING
* CONTENDED
* UNKNOWN

### Visualization

Highlight:

* conducting transistors
* active paths
* output nodes
* floating nets

### Success Criteria

Correct truth tables for:

* Inverter
* NAND
* NOR
* Transmission Gate

---

# Milestone 3 — Educational Device Models

Replace ideal switches with simplified transistor approximations.

Introduce:

* Threshold voltage
* On resistance
* Width
* Length
* Basic capacitance
* Basic propagation delay

Support technology definition files.

Example:

```yaml
technology:
  name: OpenChippy EDU CMOS

nmos:
  vt: 0.45
  ron: 12000

pmos:
  vt: -0.45
  ron: 22000
```

---

# Milestone 4 — Standard Cell Library

Users should be able to convert transistor networks into reusable cells.

Example:

```
INV_X1

NAND2_X1

NOR2_X1

MUX2_X1
```

Features:

* Cell hierarchy
* Expand / Collapse
* Pin mapping
* Library browser

---

# Milestone 5 — Truth Tables & Timing

Automatically generate:

* Truth tables
* Timing diagrams
* Switching activity
* Fanout analysis

Future additions:

* Estimated propagation delay
* RC delay
* Dynamic power estimation

---

# Milestone 6 — RTL Integration

Introduce HDL support.

Initial goals:

* Verilog import
* Verilog export
* Gate-level viewer

Future:

* VHDL
* SystemVerilog subset

---

# Milestone 7 — Synthesis Integration

Rather than reimplement synthesis, integrate existing tools.

Initial adapters:

* Yosys
* ABC

Display:

* Synthesized netlist
* Cell usage
* Logic depth
* Fanout

---

# Milestone 8 — Physical Layout

Introduce layout editing.

Support:

* Diffusion
* Poly
* Metal
* Contacts
* Vias

Features:

* Layer visibility
* Measurement tools
* Hierarchy
* Grid snapping

---

# Milestone 9 — Physical Verification

Integrate:

* DRC
* LVS

Educational diagnostics should explain:

* what failed
* why
* possible fixes

---

# Milestone 10 — Placement & Routing

Integrate:

* OpenROAD
* OpenLane

Visualize:

* Standard cell placement
* Congestion
* Routing
* Critical paths

---

# Milestone 11 — 3D Chip Visualization

Render semiconductor structures.

Display:

* Wells
* Diffusion
* Poly
* Metal
* Dielectric
* Contacts
* Vias

Users should be able to rotate and inspect an IC in three dimensions.

---

# Milestone 12 — Tiny Tapeout Integration

Support direct Tiny Tapeout workflows.

Project template:

* Pin constraints
* Tile limits
* Required ports
* Wrapper generation

Generate:

* Repository structure
* Testbench
* Configuration
* Documentation

Eventually:

One-click Tiny Tapeout project export.

---

# Milestone 13 — Plugin SDK

Expose APIs for:

* Simulators
* Importers
* Exporters
* PDKs
* Analysis tools
* Visualization modules

---

# Milestone 14 — Analog Simulation

Introduce SPICE-style analysis.

Support:

* DC Operating Point
* Transient Analysis
* AC Analysis

Likely through integration with an existing simulator before developing a native implementation.

---

# Milestone 15 — Professional Design Flow

Support complete ASIC development.

Examples:

```
RTL
 ↓
Synthesis
 ↓
Placement
 ↓
Routing
 ↓
DRC
 ↓
LVS
 ↓
Timing
 ↓
GDS
```

OpenChippy should orchestrate these tools while presenting them through a unified interface.

---

# Future Vision

Eventually OpenChippy should become a complete silicon engineering environment.

Possible modules include:

* CPU Architecture Designer
* ISA Designer
* RTL Simulator
* Standard Cell Editor
* Analog Designer
* Layout Editor
* Timing Analyzer
* Power Analyzer
* 3D Chip Viewer
* Package Viewer
* PCB Integration
* FPGA Flow
* Tiny Tapeout Export
* PDK Manager
* AI Engineering Assistant

---

# Non-Goals (for now)

OpenChippy is **not** initially attempting to:

* Replace Cadence Virtuoso
* Replace Siemens Calibre
* Replace Synopsys
* Develop a new synthesis engine
* Develop a new place-and-route engine
* Replace ngspice
* Replace OpenROAD

Instead, OpenChippy will focus on creating a unified, intuitive experience that integrates proven open-source tools while providing a significantly better educational and visualization experience.

---

# Guiding Philosophy

> "Chip design shouldn't feel like stitching together a dozen unrelated command-line tools."

OpenChippy exists to make silicon design approachable without sacrificing the ability to grow into professional workflows.

A beginner should be able to start by building a CMOS inverter from two transistors and, over time, follow that same design through synthesis, physical implementation, verification, and fabrication—all without leaving a single cohesive design environment.
