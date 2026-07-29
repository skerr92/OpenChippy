use openchippy_lib::audit_physical_drc_sources;
use std::{env, fs, process::ExitCode};

fn run() -> Result<String, String> {
    let mut arguments = env::args().skip(1);
    let project_path = arguments.next().ok_or_else(|| {
        "usage: audit_physical_drc <project.chippy> <technology.yaml>".to_string()
    })?;
    let technology_path = arguments.next().ok_or_else(|| {
        "usage: audit_physical_drc <project.chippy> <technology.yaml>".to_string()
    })?;
    if arguments.next().is_some() {
        return Err("usage: audit_physical_drc <project.chippy> <technology.yaml>".into());
    }
    let project = fs::read_to_string(&project_path)
        .map_err(|error| format!("could not read {project_path}: {error}"))?;
    let technology = fs::read_to_string(&technology_path)
        .map_err(|error| format!("could not read {technology_path}: {error}"))?;
    serde_json::to_string_pretty(&audit_physical_drc_sources(&project, &technology)?)
        .map_err(|error| error.to_string())
}

fn main() -> ExitCode {
    match run() {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
