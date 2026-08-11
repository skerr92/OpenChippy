pub mod gds_import;
pub mod gdsii;
mod history;
pub mod lef;
mod model;
#[allow(dead_code)]
mod physical_canvas;
mod physical_detailed_routing;
pub mod physical_drc;
mod physical_global_routing;
mod physical_layout;
pub mod physical_lvs;
mod physical_placement;
mod physical_planning;
#[allow(dead_code)]
mod plugins;
mod rtl;
mod simulation;
pub mod standard_cells;
pub mod technology;
mod validation;

use gdsii::GdsExportReport;
use history::ProjectHistory;
use model::{
    BlockDefinition, DeviceCharacteristics, Project, TerminalRef, WaveformGroup,
    CURRENT_FORMAT_VERSION,
};
use physical_drc::PhysicalDrcReport;
pub use physical_layout::PhysicalLayoutIr;
use physical_layout::{PhysicalRouteQualityReport, CURRENT_PHYSICAL_IR_VERSION};
use physical_lvs::NativeLvsReport;
use serde::{Deserialize, Serialize};
use simulation::{LogicState, SimulationResult, TruthTableResult, WaveformConfig, WaveformResult};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};
use tauri::Emitter;
use technology::Technology;
use thiserror::Error;
use validation::ValidationReport;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalInspection {
    layout: PhysicalLayoutIr,
    drc: PhysicalDrcReport,
    build_report: PhysicalBuildReport,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalBuildReport {
    elapsed_ms: u128,
    stages: Vec<&'static str>,
    global_routing_overflow: usize,
    detailed_routing_conflicts: usize,
    rejected_geometry_count: usize,
    orphan_routing_shapes_removed: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalBuildProgress {
    stage: &'static str,
    percent: u8,
    elapsed_ms: u128,
}

const BLOCK_LIBRARY_DIRECTORY: &str = "chippyblocks";
const OCHIPPY_FORMAT_VERSION: u32 = 1;
const PHYSICAL_ARTIFACT_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalAuditSummary {
    pub project: String,
    pub technology: String,
    pub technology_fingerprint: String,
    pub topology_compacted: bool,
    pub shape_count: usize,
    pub shape_purposes: BTreeMap<String, usize>,
    pub dummy_fill_by_layer: BTreeMap<String, usize>,
    pub density_layers: Vec<physical_layout::PhysicalDensityLayerReport>,
    pub row_count: usize,
    pub active_island_count: usize,
    pub shared_active_island_count: usize,
    pub devices_in_shared_active: usize,
    pub gate_strap_count: usize,
    pub devices_on_gate_straps: usize,
    pub shared_terminal_access_count: usize,
    pub devices_on_shared_terminal_accesses: usize,
    pub route_quality: PhysicalRouteQualityReport,
    pub error_count: usize,
    pub warning_count: usize,
    pub orphan_routing_shapes_removed: usize,
    pub by_rule: BTreeMap<String, usize>,
    pub by_layer: BTreeMap<String, usize>,
    pub open_nets: Vec<PhysicalAuditOpenNet>,
    pub native_lvs: NativeLvsReport,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalAuditOpenNet {
    pub net: usize,
    pub name: String,
    pub islands: usize,
    pub terminals: Vec<String>,
}

pub fn audit_physical_sources(
    project_source: &str,
    technology_source: &str,
) -> Result<PhysicalAuditSummary, String> {
    let mut project: Project = serde_json::from_str(project_source)
        .map_err(|error| format!("project JSON is invalid: {error}"))?;
    let mut technology = Technology::from_yaml(technology_source)?;
    technology.migrate_legacy_gds_layers();
    project.set_technology(technology.clone());
    let layout = physical_layout::normalize_project(&project)?;
    let report = physical_drc::validate(&layout, &technology);
    if std::env::var_os("OPENCHIPPY_ROUTER_TRACE").is_some() {
        for diagnostic in &report.diagnostics {
            eprintln!("{}: {}", diagnostic.rule_id, diagnostic.message);
            for shape_index in &diagnostic.shape_indices {
                let Some(shape) = layout.shapes.get(*shape_index) else {
                    continue;
                };
                let component = shape
                    .component_id
                    .and_then(|id| {
                        layout
                            .devices
                            .iter()
                            .find(|device| device.component_id == id)
                    })
                    .map(|device| device.name.as_str())
                    .unwrap_or("-");
                eprintln!(
                    "  shape={shape_index} layer={:?} net={:?} component={component} at=({:.4},{:.4}) size=({:.4},{:.4})",
                    shape.layer, shape.net, shape.x, shape.y, shape.width, shape.height
                );
            }
        }
    }
    let native_lvs = physical_lvs::compare(&layout, &technology);
    let mut by_rule = BTreeMap::new();
    let mut by_layer = BTreeMap::new();
    let mut shape_purposes = BTreeMap::new();
    let mut dummy_fill_by_layer = BTreeMap::new();
    for shape in &layout.shapes {
        *shape_purposes
            .entry(shape.purpose.as_str().to_string())
            .or_insert(0) += 1;
        if shape.purpose == physical_layout::PhysicalShapePurpose::DummyFill {
            let layer = match shape.layer {
                physical_layout::PhysicalLayer::Substrate => "substrate".to_string(),
                physical_layout::PhysicalLayer::Pwell => "pwell".to_string(),
                physical_layout::PhysicalLayer::Nwell => "nwell".to_string(),
                physical_layout::PhysicalLayer::Ndiff => "ndiff".to_string(),
                physical_layout::PhysicalLayer::Pdiff => "pdiff".to_string(),
                physical_layout::PhysicalLayer::Poly => "poly".to_string(),
                physical_layout::PhysicalLayer::Metal(index) => format!("metal{index}"),
                physical_layout::PhysicalLayer::Contact => "contact".to_string(),
                physical_layout::PhysicalLayer::Via(index) => {
                    format!("via{}{next}", index, next = index + 1)
                }
            };
            *dummy_fill_by_layer.entry(layer).or_insert(0) += 1;
        }
    }
    for diagnostic in &report.diagnostics {
        *by_rule.entry(diagnostic.rule_id.to_string()).or_insert(0) += 1;
        *by_layer.entry(diagnostic.layer.clone()).or_insert(0) += 1;
    }
    let mut open_nets = report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule_id == "CONNECTIVITY.OPEN_NET")
        .filter_map(|diagnostic| {
            let net_id = diagnostic
                .shape_indices
                .iter()
                .filter_map(|index| layout.shapes.get(*index)?.net)
                .next()?;
            let net = layout.nets.iter().find(|net| net.id == net_id)?;
            Some(PhysicalAuditOpenNet {
                net: net.id,
                name: net.name.clone(),
                islands: diagnostic.measured.round().max(0.0) as usize,
                terminals: net
                    .terminals
                    .iter()
                    .map(|terminal| format!("{}:{}", terminal.component_name, terminal.terminal))
                    .collect(),
            })
        })
        .collect::<Vec<_>>();
    open_nets.sort_by_key(|net| net.net);
    Ok(PhysicalAuditSummary {
        project: project.name,
        technology: technology.name,
        technology_fingerprint: layout.technology_fingerprint.clone(),
        topology_compacted: layout.placement.candidates[layout.placement.selected_candidate]
            .topology_compacted,
        shape_count: layout.shapes.len(),
        shape_purposes,
        dummy_fill_by_layer,
        density_layers: layout.density.layers.clone(),
        row_count: layout.row_topology.len(),
        active_island_count: layout
            .row_topology
            .iter()
            .map(|row| row.islands.len())
            .sum(),
        shared_active_island_count: layout
            .row_topology
            .iter()
            .flat_map(|row| &row.islands)
            .filter(|island| island.device_ids.len() > 1)
            .count(),
        devices_in_shared_active: layout
            .row_topology
            .iter()
            .flat_map(|row| &row.islands)
            .filter(|island| island.device_ids.len() > 1)
            .map(|island| island.device_ids.len())
            .sum(),
        gate_strap_count: layout
            .row_topology
            .iter()
            .map(|row| row.gate_straps.len())
            .sum(),
        devices_on_gate_straps: layout
            .row_topology
            .iter()
            .flat_map(|row| &row.gate_straps)
            .map(|strap| strap.device_ids.len())
            .sum(),
        shared_terminal_access_count: layout
            .row_topology
            .iter()
            .flat_map(|row| &row.islands)
            .flat_map(|island| &island.accesses)
            .filter(|access| access.shared_contact)
            .count(),
        devices_on_shared_terminal_accesses: layout
            .row_topology
            .iter()
            .flat_map(|row| &row.islands)
            .flat_map(|island| &island.accesses)
            .filter(|access| access.shared_contact)
            .map(|access| access.device_ids.len())
            .sum(),
        route_quality: layout.route_quality.clone(),
        error_count: report.error_count,
        warning_count: report.warning_count,
        orphan_routing_shapes_removed: layout.orphan_routing_shapes_removed,
        by_rule,
        by_layer,
        open_nets,
        native_lvs,
    })
}

pub fn generate_physical_ir_sources(
    project_source: &str,
    technology_source: &str,
) -> Result<PhysicalLayoutIr, String> {
    let mut project: Project = serde_json::from_str(project_source)
        .map_err(|error| format!("project JSON is invalid: {error}"))?;
    let mut technology = Technology::from_yaml(technology_source)?;
    technology.migrate_legacy_gds_layers();
    project.set_technology(technology);
    physical_layout::normalize_project(&project)
}

pub fn audit_physical_drc_sources(
    project_source: &str,
    technology_source: &str,
) -> Result<PhysicalDrcReport, String> {
    let mut project: Project = serde_json::from_str(project_source)
        .map_err(|error| format!("project JSON is invalid: {error}"))?;
    let mut technology = Technology::from_yaml(technology_source)?;
    technology.migrate_legacy_gds_layers();
    project.set_technology(technology.clone());
    Ok(physical_drc::validate(
        &physical_layout::normalize_project(&project)?,
        &technology,
    ))
}

pub fn export_lef_sources(project_source: &str, technology_source: &str) -> Result<String, String> {
    let mut project: Project = serde_json::from_str(project_source)
        .map_err(|error| format!("project JSON is invalid: {error}"))?;
    let mut technology = Technology::from_yaml(technology_source)?;
    technology.migrate_legacy_gds_layers();
    project.set_technology(technology);
    lef::export(&physical_layout::normalize_project(&project)?)
}

pub fn export_gds_sources(
    project_source: &str,
    technology_source: &str,
) -> Result<(Vec<u8>, GdsExportReport), String> {
    let mut project: Project = serde_json::from_str(project_source)
        .map_err(|error| format!("project JSON is invalid: {error}"))?;
    let mut technology = Technology::from_yaml(technology_source)?;
    technology.migrate_legacy_gds_layers();
    project.set_technology(technology.clone());
    let layout = physical_layout::normalize_project(&project)?;
    gdsii::export(
        &layout,
        technology.physical_rules.database_units_per_micron,
        &technology.gds_layers,
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OchippyManifest {
    format_version: u32,
    project_name: String,
    included_files: Vec<OchippyIncludedFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OchippyIncludedFile {
    role: String,
    path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PhysicalArtifact {
    format_version: u32,
    source_project_digest: String,
    physical_ir: PhysicalLayoutIr,
}

fn project_digest(project: &Project) -> Result<String, ProjectError> {
    let mut effective_project = project.clone();
    effective_project.technology.migrate_legacy_gds_layers();
    let bytes = serde_json::to_vec(&effective_project)?;
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Ok(format!("fnv1a64:{hash:016x}"))
}

fn safe_included_path(root: &Path, relative: &str) -> Result<PathBuf, ProjectError> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ProjectError::InvalidAction(
            ".ochippy included files must stay inside the project directory".into(),
        ));
    }
    Ok(root.join(path))
}

fn build_physical_artifact(project: &Project) -> Result<PhysicalArtifact, ProjectError> {
    Ok(PhysicalArtifact {
        format_version: PHYSICAL_ARTIFACT_FORMAT_VERSION,
        source_project_digest: project_digest(project)?,
        physical_ir: physical_layout::normalize_project(project)
            .map_err(ProjectError::InvalidAction)?,
    })
}

fn validate_physical_artifact(
    artifact: PhysicalArtifact,
    project: &Project,
) -> Result<PhysicalArtifact, ProjectError> {
    if artifact.format_version != PHYSICAL_ARTIFACT_FORMAT_VERSION {
        return Err(ProjectError::InvalidAction(format!(
            "unsupported .chippy_gds format version {}",
            artifact.format_version
        )));
    }
    if artifact.physical_ir.format_version != CURRENT_PHYSICAL_IR_VERSION {
        return Err(ProjectError::InvalidAction(format!(
            "cached physical IR version {} is not supported by this build",
            artifact.physical_ir.format_version
        )));
    }
    if artifact.source_project_digest != project_digest(project)? {
        return Err(ProjectError::InvalidAction(
            "cached .chippy_gds does not match the referenced circuit".into(),
        ));
    }
    Ok(artifact)
}

fn matching_cached_layout(
    cache: &Option<PhysicalArtifact>,
    project: &Project,
) -> Option<PhysicalLayoutIr> {
    let digest = project_digest(project).ok()?;
    cache
        .as_ref()
        .filter(|artifact| {
            artifact.format_version == PHYSICAL_ARTIFACT_FORMAT_VERSION
                && artifact.physical_ir.format_version == CURRENT_PHYSICAL_IR_VERSION
                && artifact.source_project_digest == digest
        })
        .map(|artifact| artifact.physical_ir.clone())
}

fn block_library_directory(project_path: &Path) -> PathBuf {
    project_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(BLOCK_LIBRARY_DIRECTORY)
}

fn block_file_name(name: &str) -> String {
    let stem = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{stem}.chippyblock")
}

fn write_block(project_path: &Path, definition: &BlockDefinition) -> Result<PathBuf, ProjectError> {
    let directory = block_library_directory(project_path);
    fs::create_dir_all(&directory)?;
    let destination = directory.join(block_file_name(&definition.name));
    fs::write(&destination, serde_json::to_string_pretty(definition)?)?;
    Ok(destination)
}

fn save_block_library(
    project_path: &Path,
    definitions: &[BlockDefinition],
) -> Result<(), ProjectError> {
    for definition in definitions {
        write_block(project_path, definition)?;
    }
    Ok(())
}

fn load_block_library(project_path: &Path, project: &mut Project) -> Result<(), ProjectError> {
    let directory = block_library_directory(project_path);
    if !directory.is_dir() {
        return Ok(());
    }
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("chippyblock") {
            continue;
        }
        let definition: BlockDefinition = serde_json::from_str(&fs::read_to_string(&path)?)
            .map_err(|error| {
                ProjectError::InvalidAction(format!(
                    "block library file {} is invalid: {error}",
                    path.display()
                ))
            })?;
        if project
            .block_definitions
            .iter()
            .any(|existing| existing.id == definition.id || existing.name == definition.name)
        {
            continue;
        }
        project.block_definitions.push(definition);
    }
    project
        .validate_block_graph()
        .map_err(ProjectError::InvalidAction)?;
    Ok(())
}

struct Workspace {
    history: ProjectHistory,
    path: Option<String>,
    saved: Project,
    physical_cache: Option<PhysicalArtifact>,
}

impl Default for Workspace {
    fn default() -> Self {
        let project = Project::default();
        Self {
            history: ProjectHistory::default(),
            path: None,
            saved: project,
            physical_cache: None,
        }
    }
}

impl Workspace {
    fn state(&self) -> WorkspaceState {
        WorkspaceState {
            project: self.history.current(),
            path: self.path.clone(),
            dirty: self.history.current() != self.saved,
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
        }
    }

    fn reset(&mut self, project: Project, path: Option<String>) {
        self.saved = project.clone();
        self.history.reset(project);
        self.path = path;
        self.physical_cache = None;
    }

    fn reset_with_cache(
        &mut self,
        project: Project,
        path: Option<String>,
        physical_cache: Option<PhysicalArtifact>,
    ) {
        self.saved = project.clone();
        self.history.reset(project);
        self.path = path;
        self.physical_cache = physical_cache;
    }
}

struct AppState {
    workspace: Mutex<Workspace>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceState {
    project: Project,
    path: Option<String>,
    dirty: bool,
    can_undo: bool,
    can_redo: bool,
}

#[derive(Debug, Error)]
enum ProjectError {
    #[error("project state is unavailable")]
    StateUnavailable,
    #[error("could not read project: {0}")]
    Read(#[from] std::io::Error),
    #[error("project data is invalid: {0}")]
    Invalid(#[from] serde_json::Error),
    #[error("project format version {found} is newer than supported version {supported}")]
    UnsupportedVersion { found: u32, supported: u32 },
    #[error("choose a file name before saving this project")]
    MissingPath,
    #[error("{0}")]
    InvalidAction(String),
}

impl serde::Serialize for ProjectError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[tauri::command]
fn new_project(state: tauri::State<AppState>) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace.reset(Project::default(), None);
    Ok(workspace.state())
}

#[tauri::command]
fn add_placeholder(state: tauri::State<AppState>) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .update(|project| project.add_placeholder());
    Ok(workspace.state())
}

#[tauri::command]
fn add_resistor(
    x: f64,
    y: f64,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .update(|project| project.add_resistor(x, y));
    Ok(workspace.state())
}

#[tauri::command]
fn add_component(
    kind: String,
    x: f64,
    y: f64,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.add_component(&kind, x, y).map(|_| ()))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn move_component(
    id: uuid::Uuid,
    x: f64,
    y: f64,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.move_component(id, x, y))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn move_wire(
    id: uuid::Uuid,
    route_x: f64,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.move_wire(id, route_x))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn delete_wire(
    id: uuid::Uuid,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.delete_wire(id))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn connect_terminals(
    from_component_id: uuid::Uuid,
    from_terminal: String,
    to_component_id: uuid::Uuid,
    to_terminal: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| {
            project.connect(
                TerminalRef {
                    component_id: from_component_id,
                    terminal: from_terminal,
                },
                TerminalRef {
                    component_id: to_component_id,
                    terminal: to_terminal,
                },
            )
        })
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn connect_to_point(
    from_component_id: uuid::Uuid,
    from_terminal: String,
    x: f64,
    y: f64,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| {
            project.connect_to_point(
                TerminalRef {
                    component_id: from_component_id,
                    terminal: from_terminal,
                },
                x,
                y,
            )
        })
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn extend_wire(
    id: uuid::Uuid,
    x: f64,
    y: f64,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.extend_wire(id, x, y))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn finish_wire(
    id: uuid::Uuid,
    to_component_id: uuid::Uuid,
    to_terminal: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| {
            project.finish_wire(
                id,
                TerminalRef {
                    component_id: to_component_id,
                    terminal: to_terminal,
                },
            )
        })
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn rotate_components(
    ids: Vec<uuid::Uuid>,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.rotate_components(&ids))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn delete_components(
    ids: Vec<uuid::Uuid>,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.delete_components(&ids))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn rename_component(
    id: uuid::Uuid,
    name: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.rename_component(id, name))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn set_device_geometry(
    id: uuid::Uuid,
    width_um: f64,
    length_um: f64,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.set_device_geometry(id, width_um, length_um))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn device_characteristics(
    id: uuid::Uuid,
    state: tauri::State<AppState>,
) -> Result<DeviceCharacteristics, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .current()
        .device_characteristics(id)
        .map_err(ProjectError::InvalidAction)
}

#[tauri::command]
fn rename_project(
    name: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.rename(name))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn set_timing_target(
    target_ns: Option<f64>,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.set_timing_target(target_ns))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn set_high_fanout_warning_threshold(
    threshold: usize,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.set_high_fanout_warning_threshold(threshold))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn set_waveform_groups(
    groups: Vec<WaveformGroup>,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.set_waveform_groups(groups))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn validate_project(state: tauri::State<AppState>) -> Result<ValidationReport, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    Ok(validation::validate_hierarchical(
        &workspace.history.current(),
    ))
}

#[tauri::command]
fn parse_verilog(source: String) -> Result<rtl::RtlModule, ProjectError> {
    rtl::parse_structural_verilog(&source).map_err(ProjectError::InvalidAction)
}

#[tauri::command]
fn read_verilog_source(path: String) -> Result<String, ProjectError> {
    Ok(fs::read_to_string(Path::new(&path))?)
}

#[tauri::command]
fn import_verilog(
    source: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let module = rtl::parse_structural_verilog(&source).map_err(ProjectError::InvalidAction)?;
    let design = rtl::map_module(module);
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .update(|project| project.set_rtl_design(design));
    Ok(workspace.state())
}

#[tauri::command]
fn export_verilog(state: tauri::State<AppState>) -> Result<String, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    let design = project.rtl_design.as_ref().ok_or_else(|| {
        ProjectError::InvalidAction(
            "this project does not contain an imported logical RTL design".into(),
        )
    })?;
    Ok(rtl::export_structural_verilog(&design.module))
}

#[tauri::command]
fn save_text_file(path: String, data: String) -> Result<String, ProjectError> {
    fs::write(Path::new(&path), data)?;
    Ok(path)
}

#[tauri::command]
fn generate_physical_ir(state: tauri::State<AppState>) -> Result<PhysicalLayoutIr, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    if let Some(layout) = matching_cached_layout(&workspace.physical_cache, &project) {
        return Ok(layout);
    }
    physical_layout::normalize_project(&project).map_err(ProjectError::InvalidAction)
}

#[tauri::command]
fn validate_physical_layout(
    state: tauri::State<AppState>,
) -> Result<PhysicalDrcReport, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    let layout = matching_cached_layout(&workspace.physical_cache, &project)
        .map(Ok)
        .unwrap_or_else(|| {
            physical_layout::normalize_project(&project).map_err(ProjectError::InvalidAction)
        })?;
    Ok(physical_drc::validate(&layout, &project.technology))
}

#[tauri::command]
fn validate_physical_lvs(state: tauri::State<AppState>) -> Result<NativeLvsReport, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    let layout = matching_cached_layout(&workspace.physical_cache, &project)
        .map(Ok)
        .unwrap_or_else(|| {
            physical_layout::normalize_project(&project).map_err(ProjectError::InvalidAction)
        })?;
    Ok(physical_lvs::compare(&layout, &project.technology))
}

#[tauri::command]
async fn inspect_physical_layout(
    state: tauri::State<'_, AppState>,
    window: tauri::Window,
) -> Result<PhysicalInspection, ProjectError> {
    let (project, cached_layout) = {
        let workspace = state
            .workspace
            .lock()
            .map_err(|_| ProjectError::StateUnavailable)?;
        let mut project = workspace.history.current();
        project.technology.migrate_legacy_gds_layers();
        let cached = matching_cached_layout(&workspace.physical_cache, &project);
        (project, cached)
    };
    let source_digest = project_digest(&project)?;
    let project_for_build = project.clone();
    let inspection = tauri::async_runtime::spawn_blocking(
        move || -> Result<PhysicalInspection, ProjectError> {
            let started = Instant::now();
            let from_cache = cached_layout.is_some();
            let emit_progress = |stage: &'static str, percent: u8| {
                let _ = window.emit(
                    "physical-build-progress",
                    PhysicalBuildProgress {
                        stage,
                        percent,
                        elapsed_ms: started.elapsed().as_millis(),
                    },
                );
            };
            emit_progress(
                if from_cache { "physicalIr" } else { "topology" },
                if from_cache { 94 } else { 0 },
            );
            let layout = cached_layout.map(Ok).unwrap_or_else(|| {
                physical_layout::normalize_project_with_progress(
                    &project_for_build,
                    |stage, percent| emit_progress(stage, percent),
                )
                .map_err(ProjectError::InvalidAction)
            })?;
            emit_progress("drc", 96);
            let drc = physical_drc::validate(&layout, &project_for_build.technology);
            emit_progress("complete", 100);
            let build_report = PhysicalBuildReport {
                elapsed_ms: started.elapsed().as_millis(),
                stages: if from_cache {
                    vec!["cachedPhysicalIr", "drc", "complete"]
                } else {
                    vec![
                        "initializing",
                        "placement",
                        "deviceGeneration",
                        "localRouting",
                        "globalRouting",
                        "physicalIr",
                        "drc",
                        "complete",
                    ]
                },
                global_routing_overflow: layout.global_routing.total_overflow,
                detailed_routing_conflicts: layout.detailed_routing.conflict_count,
                rejected_geometry_count: layout.detailed_routing.rejected_geometry_count,
                orphan_routing_shapes_removed: layout.orphan_routing_shapes_removed,
            };
            Ok(PhysicalInspection {
                layout,
                drc,
                build_report,
            })
        },
    )
    .await
    .map_err(|error| {
        ProjectError::InvalidAction(format!("physical build task failed: {error}"))
    })??;
    {
        let mut workspace = state
            .workspace
            .lock()
            .map_err(|_| ProjectError::StateUnavailable)?;
        if project_digest(&workspace.history.current())? == source_digest {
            workspace.physical_cache = Some(PhysicalArtifact {
                format_version: PHYSICAL_ARTIFACT_FORMAT_VERSION,
                source_project_digest: source_digest,
                physical_ir: inspection.layout.clone(),
            });
        }
    }
    Ok(inspection)
}

#[tauri::command]
fn save_physical_layout(
    path: String,
    state: tauri::State<AppState>,
) -> Result<String, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    let artifact = match matching_cached_layout(&workspace.physical_cache, &project) {
        Some(physical_ir) => PhysicalArtifact {
            format_version: PHYSICAL_ARTIFACT_FORMAT_VERSION,
            source_project_digest: project_digest(&project)?,
            physical_ir,
        },
        None => build_physical_artifact(&project)?,
    };
    fs::write(&path, serde_json::to_string_pretty(&artifact)?)?;
    workspace.physical_cache = Some(artifact);
    Ok(path)
}

#[tauri::command]
fn export_gdsii(
    path: String,
    state: tauri::State<AppState>,
) -> Result<GdsExportReport, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    let layout = matching_cached_layout(&workspace.physical_cache, &project)
        .map(Ok)
        .unwrap_or_else(|| {
            physical_layout::normalize_project(&project).map_err(ProjectError::InvalidAction)
        })?;
    let connectivity_errors = physical_drc::validate(&layout, &project.technology)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.rule_id.starts_with("CONNECTIVITY."))
        .count();
    if connectivity_errors > 0 {
        return Err(ProjectError::InvalidAction(format!(
            "GDSII export blocked: physical layout has {connectivity_errors} unconnected transistor-net diagnostics. Regenerate or repair the physical layout before manufacturing export."
        )));
    }
    let (bytes, report) = gdsii::export(
        &layout,
        project.technology.physical_rules.database_units_per_micron,
        &project.technology.gds_layers,
    )
    .map_err(ProjectError::InvalidAction)?;
    if !report.structurally_valid {
        return Err(ProjectError::InvalidAction(format!(
            "GDSII structural validation failed: {}",
            report.diagnostics.join("; ")
        )));
    }
    fs::write(path, bytes)?;
    Ok(report)
}

#[tauri::command]
fn export_lef(path: String, state: tauri::State<AppState>) -> Result<String, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    let layout = matching_cached_layout(&workspace.physical_cache, &project)
        .map(Ok)
        .unwrap_or_else(|| {
            physical_layout::normalize_project(&project).map_err(ProjectError::InvalidAction)
        })?;
    let source = lef::export(&layout).map_err(ProjectError::InvalidAction)?;
    fs::write(&path, source)?;
    Ok(path)
}

#[tauri::command]
fn save_physical_drc_report(path: String, data: String) -> Result<String, ProjectError> {
    fs::write(Path::new(&path), data)?;
    Ok(path)
}

#[tauri::command]
fn capture_block(
    name: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.capture_block(name).map(|_| ()))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn place_block(
    definition_id: uuid::Uuid,
    x: f64,
    y: f64,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.place_block(definition_id, x, y).map(|_| ()))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn update_block_from_current(
    definition_id: uuid::Uuid,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .try_update(|project| project.update_block_from_current(definition_id))
        .map_err(ProjectError::InvalidAction)?;
    Ok(workspace.state())
}

#[tauri::command]
fn export_block(
    definition_id: uuid::Uuid,
    state: tauri::State<AppState>,
) -> Result<String, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project_path = workspace.path.as_deref().ok_or(ProjectError::MissingPath)?;
    let project = workspace.history.current();
    let definition: &BlockDefinition = project
        .block_definition(definition_id)
        .ok_or_else(|| ProjectError::InvalidAction("block definition not found".into()))?;
    save_block_library(Path::new(project_path), &project.block_definitions)?;
    let destination =
        block_library_directory(Path::new(project_path)).join(block_file_name(&definition.name));
    Ok(destination.to_string_lossy().into_owned())
}

#[tauri::command]
fn load_technology(
    path: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let source = fs::read_to_string(Path::new(&path)).map_err(|error| {
        ProjectError::InvalidAction(format!("could not read technology: {error}"))
    })?;
    let technology = Technology::from_yaml(&source).map_err(ProjectError::InvalidAction)?;
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .update(|project| project.set_technology(technology));
    Ok(workspace.state())
}

#[tauri::command]
fn reset_technology(state: tauri::State<AppState>) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace
        .history
        .update(|project| project.reset_technology());
    Ok(workspace.state())
}

#[tauri::command]
fn simulate_project(
    inputs: HashMap<String, LogicState>,
    state: tauri::State<AppState>,
) -> Result<SimulationResult, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let flattened = workspace
        .history
        .current()
        .flattened()
        .map_err(ProjectError::InvalidAction)?;
    Ok(simulation::simulate(&flattened, &inputs))
}

#[tauri::command]
fn generate_truth_table(state: tauri::State<AppState>) -> Result<TruthTableResult, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    if let Some(design) = &project.rtl_design {
        return simulation::rtl_truth_table(&design.module).map_err(ProjectError::InvalidAction);
    }
    let flattened = workspace
        .history
        .current()
        .flattened()
        .map_err(ProjectError::InvalidAction)?;
    simulation::truth_table(&flattened).map_err(ProjectError::InvalidAction)
}

#[tauri::command]
fn simulate_waveform(
    config: WaveformConfig,
    state: tauri::State<AppState>,
) -> Result<WaveformResult, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let project = workspace.history.current();
    if let Some(design) = &project.rtl_design {
        return simulation::rtl_waveform(&design.module, config)
            .map_err(ProjectError::InvalidAction);
    }
    let flattened = workspace
        .history
        .current()
        .flattened()
        .map_err(ProjectError::InvalidAction)?;
    simulation::waveform(&flattened, config).map_err(ProjectError::InvalidAction)
}

#[tauri::command]
fn undo(state: tauri::State<AppState>) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace.history.undo();
    Ok(workspace.state())
}

#[tauri::command]
fn redo(state: tauri::State<AppState>) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace.history.redo();
    Ok(workspace.state())
}

fn load_circuit_file(path: &Path) -> Result<Project, ProjectError> {
    let mut project: Project = serde_json::from_str(&fs::read_to_string(path)?)?;
    if project.format_version > CURRENT_FORMAT_VERSION {
        return Err(ProjectError::UnsupportedVersion {
            found: project.format_version,
            supported: CURRENT_FORMAT_VERSION,
        });
    }
    project.technology.migrate_legacy_gds_layers();
    load_block_library(path, &mut project)?;
    Ok(project)
}

fn load_project_files(path: &Path) -> Result<(Project, Option<PhysicalArtifact>), ProjectError> {
    if path.extension().and_then(|extension| extension.to_str()) != Some("ochippy") {
        return Ok((load_circuit_file(path)?, None));
    }
    let manifest: OchippyManifest = serde_json::from_str(&fs::read_to_string(path)?)?;
    if manifest.format_version != OCHIPPY_FORMAT_VERSION {
        return Err(ProjectError::InvalidAction(format!(
            "unsupported .ochippy format version {}",
            manifest.format_version
        )));
    }
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    let circuits = manifest
        .included_files
        .iter()
        .filter(|file| file.role == "circuit")
        .collect::<Vec<_>>();
    if circuits.len() != 1 {
        return Err(ProjectError::InvalidAction(
            ".ochippy must include exactly one circuit file".into(),
        ));
    }
    let circuit_path = safe_included_path(root, &circuits[0].path)?;
    let project = load_circuit_file(&circuit_path)?;
    let physical_files = manifest
        .included_files
        .iter()
        .filter(|file| file.role == "physical")
        .collect::<Vec<_>>();
    if physical_files.len() > 1 {
        return Err(ProjectError::InvalidAction(
            ".ochippy may include at most one physical file".into(),
        ));
    }
    let physical_cache = if let Some(file) = physical_files.first() {
        let artifact_path = safe_included_path(root, &file.path)?;
        fs::read_to_string(artifact_path)
            .ok()
            .and_then(|source| serde_json::from_str(&source).ok())
            .and_then(|artifact| validate_physical_artifact(artifact, &project).ok())
    } else {
        None
    };
    Ok((project, physical_cache))
}

#[tauri::command]
fn save_project(
    path: Option<String>,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let destination = path
        .or_else(|| workspace.path.clone())
        .ok_or(ProjectError::MissingPath)?;
    let project = workspace.history.current();
    let destination_path = Path::new(&destination);
    if destination_path
        .extension()
        .and_then(|extension| extension.to_str())
        == Some("ochippy")
    {
        let root = destination_path.parent().unwrap_or_else(|| Path::new("."));
        let stem = destination_path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
            .unwrap_or("project");
        let circuit_name = format!("{stem}.chippy");
        let physical_name = format!("{stem}.chippy_gds");
        let circuit_path = root.join(&circuit_name);
        let physical_path = root.join(&physical_name);
        let artifact = match matching_cached_layout(&workspace.physical_cache, &project) {
            Some(physical_ir) => PhysicalArtifact {
                format_version: PHYSICAL_ARTIFACT_FORMAT_VERSION,
                source_project_digest: project_digest(&project)?,
                physical_ir,
            },
            None => build_physical_artifact(&project)?,
        };
        fs::write(&circuit_path, serde_json::to_string_pretty(&project)?)?;
        fs::write(&physical_path, serde_json::to_string_pretty(&artifact)?)?;
        let manifest = OchippyManifest {
            format_version: OCHIPPY_FORMAT_VERSION,
            project_name: project.name.clone(),
            included_files: vec![
                OchippyIncludedFile {
                    role: "circuit".into(),
                    path: circuit_name,
                },
                OchippyIncludedFile {
                    role: "physical".into(),
                    path: physical_name,
                },
            ],
        };
        fs::write(destination_path, serde_json::to_string_pretty(&manifest)?)?;
        save_block_library(&circuit_path, &project.block_definitions)?;
        workspace.physical_cache = Some(artifact);
    } else {
        fs::write(destination_path, serde_json::to_string_pretty(&project)?)?;
        save_block_library(destination_path, &project.block_definitions)?;
    }
    workspace.saved = project;
    workspace.path = Some(destination);
    Ok(workspace.state())
}

#[tauri::command]
fn load_project(
    path: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let (project, physical_cache) = load_project_files(Path::new(&path))?;
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace.reset_with_cache(project, Some(path), physical_cache);
    Ok(workspace.state())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Exercise the plugin boundary now so it cannot silently decay while dynamic
    // loading remains a later milestone.
    let registry = plugins::PluginRegistry::default();
    let _registered_plugin_ids = registry.ids();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            workspace: Mutex::new(Workspace::default()),
        })
        .invoke_handler(tauri::generate_handler![
            new_project,
            add_placeholder,
            add_resistor,
            add_component,
            move_component,
            move_wire,
            delete_wire,
            connect_terminals,
            connect_to_point,
            extend_wire,
            finish_wire,
            rotate_components,
            delete_components,
            rename_component,
            set_device_geometry,
            device_characteristics,
            rename_project,
            set_timing_target,
            set_high_fanout_warning_threshold,
            set_waveform_groups,
            validate_project,
            parse_verilog,
            read_verilog_source,
            import_verilog,
            export_verilog,
            save_text_file,
            generate_physical_ir,
            validate_physical_layout,
            validate_physical_lvs,
            inspect_physical_layout,
            save_physical_layout,
            export_gdsii,
            export_lef,
            save_physical_drc_report,
            capture_block,
            place_block,
            update_block_from_current,
            export_block,
            load_technology,
            reset_technology,
            simulate_project,
            generate_truth_table,
            simulate_waveform,
            undo,
            redo,
            save_project,
            load_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running OpenChippy");
}

#[cfg(test)]
mod tests {
    use super::{
        build_physical_artifact, load_block_library, load_project_files, matching_cached_layout,
        save_block_library, save_physical_drc_report, validate_physical_artifact,
        OchippyIncludedFile, OchippyManifest, Workspace, BLOCK_LIBRARY_DIRECTORY,
        CURRENT_PHYSICAL_IR_VERSION, OCHIPPY_FORMAT_VERSION,
    };
    use crate::model::{Project, TerminalRef};
    use std::fs;

    #[test]
    fn physical_drc_report_is_saved_verbatim() {
        let directory =
            std::env::temp_dir().join(format!("openchippy-drc-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("report.json");
        let data = r#"{"formatVersion":1,"report":{"errorCount":0}}"#;
        let saved = save_physical_drc_report(path.to_string_lossy().into(), data.into()).unwrap();
        assert_eq!(saved, path.to_string_lossy());
        assert_eq!(fs::read_to_string(&path).unwrap(), data);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn workspace_tracks_dirty_state_across_history() {
        let mut workspace = Workspace::default();
        assert!(!workspace.state().dirty);

        workspace
            .history
            .update(|project| project.add_placeholder());
        assert!(workspace.state().dirty);
        assert!(workspace.state().can_undo);

        workspace.history.undo();
        assert!(!workspace.state().dirty);
        assert!(workspace.state().can_redo);
    }

    #[test]
    fn reset_marks_loaded_project_clean_and_clears_history() {
        let mut workspace = Workspace::default();
        let mut loaded = workspace.history.current();
        loaded.add_placeholder();
        workspace.reset(loaded, Some("example.chippy".into()));

        let state = workspace.state();
        assert!(!state.dirty);
        assert!(!state.can_undo);
        assert!(!state.can_redo);
        assert_eq!(state.path.as_deref(), Some("example.chippy"));
    }

    #[test]
    fn adjacent_block_library_round_trips_and_deduplicates_definitions() {
        let root = std::env::temp_dir().join(format!("openchippy-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let project_path = root.join("logic.chippy");
        let mut source = Project::default();
        source.add_component("input", -2.0, 0.0).unwrap();
        source.add_component("output", 2.0, 0.0).unwrap();
        source.capture_block("Reusable Gate".into()).unwrap();

        save_block_library(&project_path, &source.block_definitions).unwrap();
        let library = root.join(BLOCK_LIBRARY_DIRECTORY);
        assert!(library.join("Reusable_Gate.chippyblock").is_file());

        let mut loaded = Project::default();
        load_block_library(&project_path, &mut loaded).unwrap();
        load_block_library(&project_path, &mut loaded).unwrap();
        assert_eq!(loaded.block_definitions.len(), 1);
        assert_eq!(loaded.block_definitions[0].name, "Reusable Gate");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loading_a_legacy_gf180_project_restores_pwell_export_mapping() {
        let root = std::env::temp_dir().join(format!("openchippy-gf180-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("legacy.chippy");
        let mut project = Project::default();
        project.technology = crate::technology::Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .unwrap();
        project.technology.gds_layers.layers.remove("pwell");
        fs::write(&path, serde_json::to_string_pretty(&project).unwrap()).unwrap();

        let (loaded, cache) = load_project_files(&path).unwrap();
        assert!(cache.is_none());
        assert_eq!(loaded.technology.gds_layers.layers["pwell"][0].layer, 204);
        assert!(loaded.technology.validate().is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ochippy_manifest_loads_circuit_and_matching_physical_cache() {
        let root =
            std::env::temp_dir().join(format!("openchippy-project-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let circuit_path = root.join("inverter.chippy");
        let physical_path = root.join("inverter.chippy_gds");
        let manifest_path = root.join("inverter.ochippy");
        let mut project = Project::default();
        project.name = "Cached inverter".into();
        let vdd = project.add_component("vdd", 0.0, -4.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 4.0).unwrap();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        let output = project.add_component("output", 4.0, 0.0).unwrap();
        let pmos = project.add_component("pmos", 0.0, -1.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 1.0).unwrap();
        let terminal = |component_id, terminal: &str| TerminalRef {
            component_id,
            terminal: terminal.into(),
        };
        project
            .connect(terminal(input, "out"), terminal(pmos, "gate"))
            .unwrap();
        project
            .connect(terminal(input, "out"), terminal(nmos, "gate"))
            .unwrap();
        project
            .connect(terminal(vdd, "out"), terminal(pmos, "source"))
            .unwrap();
        project
            .connect(terminal(gnd, "out"), terminal(nmos, "source"))
            .unwrap();
        project
            .connect(terminal(pmos, "drain"), terminal(nmos, "drain"))
            .unwrap();
        project
            .connect(terminal(pmos, "drain"), terminal(output, "in"))
            .unwrap();
        let artifact = build_physical_artifact(&project).unwrap();
        let mut stale_artifact = artifact.clone();
        stale_artifact.physical_ir.format_version = CURRENT_PHYSICAL_IR_VERSION.saturating_sub(1);
        assert!(matching_cached_layout(&Some(stale_artifact.clone()), &project).is_none());
        assert!(validate_physical_artifact(stale_artifact, &project)
            .unwrap_err()
            .to_string()
            .contains("physical IR version"));
        fs::write(
            &circuit_path,
            serde_json::to_string_pretty(&project).unwrap(),
        )
        .unwrap();
        fs::write(
            &physical_path,
            serde_json::to_string_pretty(&artifact).unwrap(),
        )
        .unwrap();
        let manifest = OchippyManifest {
            format_version: OCHIPPY_FORMAT_VERSION,
            project_name: project.name.clone(),
            included_files: vec![
                OchippyIncludedFile {
                    role: "circuit".into(),
                    path: "inverter.chippy".into(),
                },
                OchippyIncludedFile {
                    role: "physical".into(),
                    path: "inverter.chippy_gds".into(),
                },
            ],
        };
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();

        let (loaded, cache) = load_project_files(&manifest_path).unwrap();
        assert_eq!(loaded.name, project.name);
        let cached_layout = cache.unwrap().physical_ir;
        assert_eq!(
            cached_layout.format_version,
            artifact.physical_ir.format_version
        );
        assert_eq!(cached_layout.devices, artifact.physical_ir.devices);
        assert_eq!(cached_layout.nets, artifact.physical_ir.nets);
        assert_eq!(
            cached_layout.shapes.len(),
            artifact.physical_ir.shapes.len()
        );
        project.name = "Edited after physical generation".into();
        fs::write(
            &circuit_path,
            serde_json::to_string_pretty(&project).unwrap(),
        )
        .unwrap();
        let (edited, stale_cache) = load_project_files(&manifest_path).unwrap();
        assert_eq!(edited.name, project.name);
        assert!(stale_cache.is_none());

        let mut stale_artifact = artifact;
        stale_artifact.physical_ir.format_version = CURRENT_PHYSICAL_IR_VERSION.saturating_sub(1);
        fs::write(
            &physical_path,
            serde_json::to_string_pretty(&stale_artifact).unwrap(),
        )
        .unwrap();
        let (loaded_without_stale_cache, stale_cache) = load_project_files(&manifest_path).unwrap();
        assert_eq!(loaded_without_stale_cache.name, project.name);
        assert!(stale_cache.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ochippy_manifest_rejects_paths_outside_its_directory() {
        let root =
            std::env::temp_dir().join(format!("openchippy-project-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("unsafe.ochippy");
        let manifest = OchippyManifest {
            format_version: OCHIPPY_FORMAT_VERSION,
            project_name: "Unsafe".into(),
            included_files: vec![OchippyIncludedFile {
                role: "circuit".into(),
                path: "../outside.chippy".into(),
            }],
        };
        fs::write(
            &manifest_path,
            serde_json::to_string_pretty(&manifest).unwrap(),
        )
        .unwrap();
        assert!(load_project_files(&manifest_path).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
