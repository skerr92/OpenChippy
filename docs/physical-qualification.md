# Reproducing physical qualification

Generate the layout once and retain the exact IR, GDS, and native DRC/LVS results:

```sh
cargo run --release --manifest-path src-tauri/Cargo.toml --example qualify_physical -- \
  /path/to/design.chippy docs/examples/gf180mcu-3v3-5m-openchippy.yaml /tmp/qualification
```

Add `--strict` to exit unsuccessfully when native DRC, LVS, nonempty device
inventory, GDS structural validation, or technology-fingerprint agreement fails.
Reports are written before a strict-check failure so the evidence is inspectable.
Without `--strict`, the command retains its diagnostic export behavior.

The same command accepts a saved `layout.json` to validate and export existing
geometry. Use a new output directory for each experiment. Native report results
must be inspected; generation success does not imply DRC or LVS closure.

Run the repository-owned alpha fixtures without a private saved project:

```sh
cargo build --manifest-path src-tauri/Cargo.toml --example qualify_physical
python3 scripts/check_alpha_physical.py --output /tmp/alpha-native-physical
```

Choose a new output directory each run. This checks inverter and NAND2 circuits
against the EDU and GF180 decks, independently regenerates each GDS twice,
compares bytes, and proves invalid geometry fails strict qualification. Evidence
includes IR, native reports, GDS, and a summary with source/deck/GDS SHA-256 hashes.
CI runs the same checks and uploads evidence even when a case fails. These small
fixtures do not replace hierarchical-adder or official-deck qualification.

Run the official GF180 variant-C deck with an installed KLayout and local checkout
of the upstream `globalfoundries-pdk-libs-gf180mcu_fd_pv` repository:

```sh
python3 scripts/qualify_gf180.py /tmp/qualification/layout.gds \
  --deck /path/to/globalfoundries-pdk-libs-gf180mcu_fd_pv/klayout/drc \
  --klayout /path/to/klayout --output /tmp/official-qualification
```

The runner retains the report, log, GDS hash, upstream commit, and marker counts.
It adapts Ruby endless-method syntax in a temporary deck copy for older KLayout
Ruby versions. It does not alter rule geometry or suppress categories, including
density and DBU markers. Review all residual categories before making a
qualification claim. Independent foundry LVS and manufacturing qualification
remain separate from native device/connectivity comparison.

Historical repository evidence describes its recorded artifacts. A new process
fingerprint, layout version, or input circuit requires fresh qualification.
