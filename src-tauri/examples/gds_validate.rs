use std::{env, fs, process};

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: cargo run --example gds_validate -- <design.gds>");
        process::exit(2);
    };
    let bytes = fs::read(&path).unwrap_or_else(|error| {
        eprintln!("could not read {path}: {error}");
        process::exit(2);
    });
    let report = openchippy_lib::gdsii::validate(&bytes).unwrap_or_else(|error| {
        eprintln!("GDSII validation failed: {error}");
        process::exit(1);
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    if !report.structurally_valid {
        process::exit(1);
    }
}
