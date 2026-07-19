mod history;
mod model;
mod physical_layout;
#[allow(dead_code)]
mod plugins;
mod simulation;
pub mod technology;
mod validation;

use history::ProjectHistory;
use model::{BlockDefinition, DeviceCharacteristics, Project, TerminalRef, CURRENT_FORMAT_VERSION};
use physical_layout::PhysicalLayoutIr;
use serde::Serialize;
use simulation::{LogicState, SimulationResult, TruthTableResult, WaveformConfig, WaveformResult};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use technology::Technology;
use thiserror::Error;
use validation::ValidationReport;

const BLOCK_LIBRARY_DIRECTORY: &str = "chippyblocks";

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
}

impl Default for Workspace {
    fn default() -> Self {
        let project = Project::default();
        Self {
            history: ProjectHistory::default(),
            path: None,
            saved: project,
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
fn generate_physical_ir(state: tauri::State<AppState>) -> Result<PhysicalLayoutIr, ProjectError> {
    let workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    let flattened = workspace
        .history
        .current()
        .flattened()
        .map_err(ProjectError::InvalidAction)?;
    physical_layout::normalize(&flattened).map_err(ProjectError::InvalidAction)
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
    let data = serde_json::to_string_pretty(&project)?;
    fs::write(Path::new(&destination), data)?;
    save_block_library(Path::new(&destination), &project.block_definitions)?;
    workspace.saved = project;
    workspace.path = Some(destination);
    Ok(workspace.state())
}

#[tauri::command]
fn load_project(
    path: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let project_path = Path::new(&path);
    let mut project: Project = serde_json::from_str(&fs::read_to_string(project_path)?)?;
    if project.format_version > CURRENT_FORMAT_VERSION {
        return Err(ProjectError::UnsupportedVersion {
            found: project.format_version,
            supported: CURRENT_FORMAT_VERSION,
        });
    }
    load_block_library(project_path, &mut project)?;
    let mut workspace = state
        .workspace
        .lock()
        .map_err(|_| ProjectError::StateUnavailable)?;
    workspace.reset(project, Some(path));
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
            validate_project,
            generate_physical_ir,
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
    use super::{load_block_library, save_block_library, Workspace, BLOCK_LIBRARY_DIRECTORY};
    use crate::model::Project;
    use std::fs;

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
}
