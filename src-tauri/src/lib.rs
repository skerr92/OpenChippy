mod history;
mod model;
#[allow(dead_code)]
mod plugins;
mod validation;

use history::ProjectHistory;
use model::{Project, TerminalRef, CURRENT_FORMAT_VERSION};
use serde::Serialize;
use std::{fs, path::Path, sync::Mutex};
use thiserror::Error;
use validation::ValidationReport;

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
    Ok(validation::validate(&workspace.history.current()))
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
    workspace.saved = project;
    workspace.path = Some(destination);
    Ok(workspace.state())
}

#[tauri::command]
fn load_project(
    path: String,
    state: tauri::State<AppState>,
) -> Result<WorkspaceState, ProjectError> {
    let project: Project = serde_json::from_str(&fs::read_to_string(Path::new(&path))?)?;
    if project.format_version > CURRENT_FORMAT_VERSION {
        return Err(ProjectError::UnsupportedVersion {
            found: project.format_version,
            supported: CURRENT_FORMAT_VERSION,
        });
    }
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
            rename_project,
            validate_project,
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
    use super::Workspace;

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
}
