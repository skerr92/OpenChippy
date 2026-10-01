# Reproducing physical qualification

Generate the layout once and retain the exact IR, GDS, and native DRC/LVS results:

```sh
cargo run --release --manifest-path src-tauri/Cargo.toml --example qualify_physical -- \
  /path/to/design.chippy docs/examples/gf180mcu-3v3-5m-openchippy.yaml /tmp/qualification
```

The same command accepts a saved `layout.json` to validate and export existing
geometry. Use a new output directory for each experiment. Native report results
must be inspected; generation success does not imply DRC or LVS closure.

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
