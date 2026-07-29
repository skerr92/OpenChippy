use openchippy_lib::generate_physical_ir_sources;
use std::{env, fs, process::ExitCode};

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let project_path = arguments.next().ok_or_else(|| {
        "usage: physical_ir_export <project.chippy> <technology.yaml> <output.json>".to_string()
    })?;
    let technology_path = arguments.next().ok_or_else(|| {
        "usage: physical_ir_export <project.chippy> <technology.yaml> <output.json>".to_string()
    })?;
    let output_path = arguments.next().ok_or_else(|| {
        "usage: physical_ir_export <project.chippy> <technology.yaml> <output.json>".to_string()
    })?;
    if arguments.next().is_some() {
        return Err(
            "usage: physical_ir_export <project.chippy> <technology.yaml> <output.json>".into(),
        );
    }
    let project = fs::read_to_string(&project_path)
        .map_err(|error| format!("could not read {project_path}: {error}"))?;
    let technology = fs::read_to_string(&technology_path)
        .map_err(|error| format!("could not read {technology_path}: {error}"))?;
    let layout = generate_physical_ir_sources(&project, &technology)?;
    let json = serde_json::to_string_pretty(&layout)
        .map_err(|error| format!("could not serialize Physical IR: {error}"))?;
    fs::write(&output_path, json).map_err(|error| format!("could not write {output_path}: {error}"))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
