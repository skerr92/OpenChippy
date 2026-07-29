use std::{env, fs, process};

fn main() {
    let mut arguments = env::args().skip(1);
    let Some(left_path) = arguments.next() else {
        eprintln!("usage: cargo run --example gds_compare -- <left.gds> <right.gds>");
        process::exit(2);
    };
    let Some(right_path) = arguments.next() else {
        eprintln!("usage: cargo run --example gds_compare -- <left.gds> <right.gds>");
        process::exit(2);
    };
    let left = read(&left_path);
    let right = read(&right_path);
    let equivalent = left == right;
    println!(
        "{}",
        serde_json::json!({
            "formatVersion": 1,
            "left": left_path,
            "right": right_path,
            "equivalent": equivalent,
            "leftTopCell": left.top_cell,
            "rightTopCell": right.top_cell,
            "leftCellCount": left.cells.len(),
            "rightCellCount": right.cells.len(),
            "databaseUnitsPerMicron": left.database_units_per_micron,
        })
    );
    if !equivalent {
        process::exit(1);
    }
}

fn read(path: &str) -> openchippy_lib::gds_import::CanonicalGds {
    let bytes = fs::read(path).unwrap_or_else(|error| {
        eprintln!("could not read {path}: {error}");
        process::exit(2);
    });
    openchippy_lib::gds_import::import_canonical(&bytes).unwrap_or_else(|error| {
        eprintln!("could not import {path}: {error}");
        process::exit(1);
    })
}
