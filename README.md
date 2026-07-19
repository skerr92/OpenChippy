# OpenChippy
OpenChippy is an open source replacement for commercial VLSI design, visualization, and simulation tooling. One open source tool to handle what commercial tools do.

### About OpenChippy

OpenChippy is an open source project designed to help make it easier to develop
integrated circuits from a transitor level up. Most professional grade tools require
expensive license models and seat costs which are typically prohibitive towards individuals.
Open source tools currently only solve small parts of the flow and tend to require a bit of
work to understand and integrate each into the perspective steps of the integrated circuit
design flow.

OpenChippy seeks to address those issues by creating an open workflow to take you from
transistor placement through 3D visualization, to exporting GDS files for manufacturing.
These challenges represent time needed to be spent to implement and validate each stage.

Current capabilities will be listed in the next section, capabilities.

### Capabilities

Currently, there are no capabilities for OpenChippy as this project is just starting out.
Over time this main readme will be updated, and any deeper documentation will be held in the
[docs](docs/) directory.

### contribution

Anyone can join this project and contributors are welcome. This project mainly focuses the
backend in Rust to help with better data serialization and memory safety, while the front end
will likely be served in a web interface.

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

The browser preview can place placeholder components locally. Save/load and
undo/redo are backed by Rust and therefore require the desktop application.

Useful validation commands:

```sh
npm run build
cd src-tauri && cargo test
```
