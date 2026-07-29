use openchippy_lib::{gds_import, physical_drc, technology::Technology};
use std::{collections::BTreeMap, env, fs, process};

fn main() {
    let mut arguments = env::args().skip(1);
    let Some(gds_path) = arguments.next() else {
        eprintln!("usage: cargo run --example gds_import_audit -- <design.gds> <technology.yaml>");
        process::exit(2);
    };
    let Some(technology_path) = arguments.next() else {
        eprintln!("usage: cargo run --example gds_import_audit -- <design.gds> <technology.yaml>");
        process::exit(2);
    };
    let gds = fs::read(&gds_path).unwrap_or_else(|error| fail(&gds_path, error));
    let technology_yaml =
        fs::read_to_string(&technology_path).unwrap_or_else(|error| fail(&technology_path, error));
    let technology = Technology::from_yaml(&technology_yaml).unwrap_or_else(|error| {
        eprintln!("could not load {technology_path}: {error}");
        process::exit(1);
    });
    let canonical = gds_import::import_canonical(&gds).unwrap_or_else(|error| {
        eprintln!("could not import {gds_path}: {error}");
        process::exit(1);
    });
    let physical = canonical
        .to_physical_geometry(&technology.gds_layers)
        .unwrap_or_else(|error| {
            eprintln!("could not map {gds_path} through {technology_path}: {error}");
            process::exit(1);
        });
    let drc = physical_drc::validate_imported_shapes(
        &physical.shapes,
        technology.max_metal_layers,
        &technology,
    );
    let mut by_rule = BTreeMap::<String, usize>::new();
    let mut by_layer = BTreeMap::<String, usize>::new();
    for diagnostic in &drc.diagnostics {
        *by_rule.entry(diagnostic.rule_id.clone()).or_default() += 1;
        *by_layer.entry(diagnostic.layer.clone()).or_default() += 1;
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "formatVersion": 1,
            "gds": gds_path,
            "technology": technology_path,
            "topCell": physical.source_top_cell,
            "databaseUnitsPerMicron": physical.database_units_per_micron,
            "shapeCount": physical.shapes.len(),
            "bounds": physical.bounds,
            "unmappedLayerPairs": physical.unmapped_layer_pairs,
            "nativeDrc": {
                "errorCount": drc.error_count,
                "warningCount": drc.warning_count,
                "byRule": by_rule,
                "byLayer": by_layer,
            },
        }))
        .unwrap()
    );
}

fn fail(path: &str, error: impl std::fmt::Display) -> ! {
    eprintln!("could not read {path}: {error}");
    process::exit(2);
}
