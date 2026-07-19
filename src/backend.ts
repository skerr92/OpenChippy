import { invoke } from "@tauri-apps/api/core";
import type { Project, ValidationReport, WorkspaceState } from "./types";

const browserFallback = (): WorkspaceState => ({
  project: {
    formatVersion: 1,
    name: "Untitled chip",
    components: [],
    wires: [],
  },
  path: null,
  dirty: false,
  canUndo: false,
  canRedo: false,
});

export const inTauri = () => "__TAURI_INTERNALS__" in window;

export async function createProject(): Promise<WorkspaceState> {
  return inTauri() ? invoke("new_project") : browserFallback();
}

export async function addPlaceholder(): Promise<WorkspaceState> {
  if (inTauri()) return invoke("add_placeholder");
  throw new Error("Desktop backend unavailable in browser preview");
}

export async function addResistor(x: number, y: number): Promise<WorkspaceState> {
  if (inTauri()) return invoke("add_resistor", { x, y });
  throw new Error("Desktop backend unavailable in browser preview");
}

export async function addComponent(kind: string, x: number, y: number): Promise<WorkspaceState> {
  return invoke("add_component", { kind, x, y });
}

export async function moveComponent(id: string, x: number, y: number): Promise<WorkspaceState> {
  return invoke("move_component", { id, x, y });
}

export async function moveWire(id: string, routeX: number): Promise<WorkspaceState> {
  return invoke("move_wire", { id, routeX });
}

export async function deleteWire(id: string): Promise<WorkspaceState> {
  return invoke("delete_wire", { id });
}

export async function connectTerminals(
  fromComponentId: string,
  fromTerminal: string,
  toComponentId: string,
  toTerminal: string,
): Promise<WorkspaceState> {
  return invoke("connect_terminals", {
    fromComponentId,
    fromTerminal,
    toComponentId,
    toTerminal,
  });
}

export async function connectToPoint(
  fromComponentId: string,
  fromTerminal: string,
  x: number,
  y: number,
): Promise<WorkspaceState> {
  return invoke("connect_to_point", { fromComponentId, fromTerminal, x, y });
}

export async function finishWire(
  id: string,
  toComponentId: string,
  toTerminal: string,
): Promise<WorkspaceState> {
  return invoke("finish_wire", { id, toComponentId, toTerminal });
}

export async function extendWire(id: string, x: number, y: number): Promise<WorkspaceState> {
  return invoke("extend_wire", { id, x, y });
}

export async function rotateComponents(ids: string[]): Promise<WorkspaceState> {
  return invoke("rotate_components", { ids });
}

export async function deleteComponents(ids: string[]): Promise<WorkspaceState> {
  return invoke("delete_components", { ids });
}

export async function renameComponent(id: string, name: string): Promise<WorkspaceState> {
  return invoke("rename_component", { id, name });
}

export async function renameProject(name: string): Promise<WorkspaceState> {
  return invoke("rename_project", { name });
}

export async function validateProject(): Promise<ValidationReport> {
  return invoke("validate_project");
}

export async function undo(): Promise<WorkspaceState> {
  return invoke("undo");
}

export async function redo(): Promise<WorkspaceState> {
  return invoke("redo");
}

export async function saveProject(path: string | null): Promise<WorkspaceState> {
  return invoke("save_project", { path });
}

export async function loadProject(path: string): Promise<WorkspaceState> {
  return invoke("load_project", { path });
}
