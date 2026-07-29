use openchippy_lib::export_lef_sources;
use std::{env, fs, process::ExitCode};

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let project_path = arguments.next().ok_or_else(|| {
        "usage: lef_export <project.chippy> <technology.yaml> <output.lef>".to_string()
    })?;
    let technology_path = arguments.next().ok_or_else(|| {
        "usage: lef_export <project.chippy> <technology.yaml> <output.lef>".to_string()
    })?;
    let output_path = arguments.next().ok_or_else(|| {
        "usage: lef_export <project.chippy> <technology.yaml> <output.lef>".to_string()
    })?;
    if arguments.next().is_some() {
        return Err("usage: lef_export <project.chippy> <technology.yaml> <output.lef>".into());
    }
    let project = fs::read_to_string(&project_path)
        .map_err(|error| format!("could not read {project_path}: {error}"))?;
    let technology = fs::read_to_string(&technology_path)
        .map_err(|error| format!("could not read {technology_path}: {error}"))?;
    fs::write(&output_path, export_lef_sources(&project, &technology)?)
        .map_err(|error| format!("could not write {output_path}: {error}"))?;
    println!("{output_path}");
    Ok(())
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
