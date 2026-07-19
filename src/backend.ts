import { invoke } from "@tauri-apps/api/core";
import type { DeviceCharacteristics, LogicState, PhysicalLayoutIr, Project, SimulationResult, TruthTableResult, ValidationReport, WaveformConfig, WaveformResult, WorkspaceState } from "./types";

const browserFallback = (): WorkspaceState => ({
  project: {
    formatVersion: 1,
    name: "Untitled chip",
    components: [],
    wires: [],
    blockDefinitions: [],
    technology: {
      format_version: 1,
      name: "OpenChippy EDU CMOS",
      supply_voltage: 1.8,
      max_metal_layers: 5,
      nmos: {
        threshold_voltage: .45,
        nominal_on_resistance_ohms: 12000,
        reference_width_um: 1,
        reference_length_um: 1,
        gate_capacitance_ff_per_um: 2,
        diffusion_capacitance_ff_per_um: 1,
      },
      pmos: {
        threshold_voltage: -.45,
        nominal_on_resistance_ohms: 22000,
        reference_width_um: 1,
        reference_length_um: 1,
        gate_capacitance_ff_per_um: 2.2,
        diffusion_capacitance_ff_per_um: 1.2,
      },
    },
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

export async function setDeviceGeometry(id: string, widthUm: number, lengthUm: number): Promise<WorkspaceState> {
  return invoke("set_device_geometry", { id, widthUm, lengthUm });
}

export async function deviceCharacteristics(id: string): Promise<DeviceCharacteristics> {
  return invoke("device_characteristics", { id });
}

export async function renameProject(name: string): Promise<WorkspaceState> {
  return invoke("rename_project", { name });
}

export async function validateProject(): Promise<ValidationReport> {
  return invoke("validate_project");
}

export async function generatePhysicalIr(): Promise<PhysicalLayoutIr> {
  return invoke("generate_physical_ir");
}

export async function captureBlock(name: string): Promise<WorkspaceState> {
  return invoke("capture_block", { name });
}

export async function placeBlock(definitionId: string, x: number, y: number): Promise<WorkspaceState> {
  return invoke("place_block", { definitionId, x, y });
}

export async function updateBlockFromCurrent(definitionId: string): Promise<WorkspaceState> {
  return invoke("update_block_from_current", { definitionId });
}

export async function exportBlock(definitionId: string): Promise<string> {
  return invoke("export_block", { definitionId });
}

export async function simulateProject(inputs: Record<string, LogicState>): Promise<SimulationResult> {
  return invoke("simulate_project", { inputs });
}

export async function generateTruthTable(): Promise<TruthTableResult> {
  return invoke("generate_truth_table");
}

export async function simulateWaveform(config: WaveformConfig): Promise<WaveformResult> {
  return invoke("simulate_waveform", { config });
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

export async function loadTechnology(path: string): Promise<WorkspaceState> {
  return invoke("load_technology", { path });
}

export async function resetTechnology(): Promise<WorkspaceState> {
  return invoke("reset_technology");
}
