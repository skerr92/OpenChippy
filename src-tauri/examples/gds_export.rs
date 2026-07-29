use openchippy_lib::export_gds_sources;
use std::{env, fs, process};

fn main() {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        eprintln!("usage: gds_export <project.chippy> <technology.yaml> <output.gds>");
        process::exit(2);
    }
    let project = fs::read_to_string(&arguments[0]).unwrap_or_else(|error| {
        eprintln!("could not read {}: {error}", arguments[0]);
        process::exit(2);
    });
    let technology = fs::read_to_string(&arguments[1]).unwrap_or_else(|error| {
        eprintln!("could not read {}: {error}", arguments[1]);
        process::exit(2);
    });
    let (bytes, report) = export_gds_sources(&project, &technology).unwrap_or_else(|error| {
        eprintln!("could not export GDSII: {error}");
        process::exit(1);
    });
    fs::write(&arguments[2], bytes).unwrap_or_else(|error| {
        eprintln!("could not write {}: {error}", arguments[2]);
        process::exit(2);
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
