use openchippy_lib::{audit_physical_sources, PhysicalAuditSummary};
use std::{env, fs, process::ExitCode};

fn run() -> Result<PhysicalAuditSummary, String> {
    let mut arguments = env::args().skip(1);
    let project_path = arguments
        .next()
        .ok_or_else(|| "usage: audit_physical <project.chippy> <technology.yaml>".to_string())?;
    let technology_path = arguments
        .next()
        .ok_or_else(|| "usage: audit_physical <project.chippy> <technology.yaml>".to_string())?;
    if arguments.next().is_some() {
        return Err("usage: audit_physical <project.chippy> <technology.yaml>".into());
    }

    let project_source = fs::read_to_string(&project_path)
        .map_err(|error| format!("could not read {project_path}: {error}"))?;
    let technology_source = fs::read_to_string(&technology_path)
        .map_err(|error| format!("could not read {technology_path}: {error}"))?;
    audit_physical_sources(&project_source, &technology_source)
}

fn main() -> ExitCode {
    match run() {
        Ok(summary) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&summary)
                    .expect("physical audit summary is serializable")
            );
            if summary.error_count == 0 {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            }
        }
        Err(error) => {
            eprintln!("physical audit failed: {error}");
            ExitCode::FAILURE
        }
    }
}
