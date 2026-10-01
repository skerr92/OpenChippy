//! Generate once and retain reproducible IR, native DRC/LVS, and GDS evidence.
use openchippy_lib::{
    gdsii, generate_physical_ir_sources, physical_drc, physical_lvs, technology::Technology,
    PhysicalLayoutIr,
};
use std::{env, fs, path::Path, process::ExitCode};
fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 && !(args.len() == 4 && args[3] == "--refill") {
        return Err("usage: qualify_physical <project.chippy|layout.json> <technology.yaml> <output-directory> [--refill]".into());
    }
    let source = fs::read_to_string(&args[0]).map_err(|e| e.to_string())?;
    let yaml = fs::read_to_string(&args[1]).map_err(|e| e.to_string())?;
    let mut tech = Technology::from_yaml(&yaml)?;
    tech.migrate_legacy_gds_layers();
    let mut ir: PhysicalLayoutIr = if args[0].ends_with(".json") {
        serde_json::from_str(&source).map_err(|e| format!("invalid IR: {e}"))?
    } else {
        generate_physical_ir_sources(&source, &yaml)?
    };
    if args.len() == 4 {
        if !tech.physical_rules.density_fill.use_arrays || !tech.physical_rules.density_fill.cover_full_usable_area {
            return Err("--refill requires full-die density arrays in the process YAML".into());
        }
        // Rebuild only inert fill, preserving the routed fixture for correlation.
        // Block references contain old shape indices, so export a flat top cell.
        ir.shapes.retain(|shape| format!("{:?}",shape.purpose) != "DummyFill");
        ir.physical_blocks.clear();
        (ir.density_arrays, ir.density) = openchippy_lib::density_arrays::generate(&ir.shapes, &ir.bounds, &tech)?;
    }
    let out = Path::new(&args[2]);
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    fs::write(
        out.join("layout.json"),
        serde_json::to_vec(&ir).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let drc = physical_drc::validate(&ir, &tech);
    let lvs = physical_lvs::compare(&ir, &tech);
    let (gds, report) = gdsii::export(
        &ir,
        tech.physical_rules.database_units_per_micron,
        &tech.gds_layers,
    )?;
    fs::write(out.join("layout.gds"), gds).map_err(|e| e.to_string())?;
    let evidence = serde_json::json!({"drc": drc, "lvs": lvs, "gds": report});
    fs::write(
        out.join("native-report.json"),
        serde_json::to_vec_pretty(&evidence).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "{} shapes; {} DRC errors; evidence: {}",
        ir.shapes.len(),
        drc.error_count,
        out.display()
    );
    Ok(())
}
fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
