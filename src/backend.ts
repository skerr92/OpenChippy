import { invoke } from "@tauri-apps/api/core";
import type { DeviceCharacteristics, LogicState, PhysicalDrcReport, PhysicalLayoutIr, Project, RtlModule, SimulationResult, TruthTableResult, ValidationReport, WaveformConfig, WaveformGroup, WaveformResult, WorkspaceState } from "./types";

const browserFallback = (): WorkspaceState => ({
  project: {
    formatVersion: 1,
    name: "Untitled chip",
    components: [],
    wires: [],
    blockDefinitions: [],
    waveformGroups: [],
    timingTargetNs: null,
    highFanoutWarningThreshold: 8,
    rtlDesign: null,
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
      physical_parasitics: {
        format_version: 1,
        wire_capacitance_ff_per_um: .16,
        via_capacitance_ff: .05,
        layer_capacitance_ff_per_um: {},
        via_capacitance_overrides_ff: {},
      },
      tapeout_window: {
        format_version: 1,
        name: "Caravel SKY130 user area",
        width_um: 2920,
        height_um: 3520,
        edge_margin_um: 0,
      },
      physical_rules: {
        format_version: 1,
        database_units_per_micron: 1000,
        manufacturing_grid_um: .01,
        diffusion: { min_width_um: .3, min_spacing_um: .3, min_area_um2: .09 },
        poly: { min_width_um: .2, min_spacing_um: .2, min_area_um2: .04 },
        well: { min_width_um: .6, min_spacing_um: .6, min_area_um2: .36 },
        metal: { min_width_um: .2, min_spacing_um: .04, min_area_um2: .04 },
        contact: { size_um: .22, min_spacing_um: .08, enclosure_um: .03 },
        via: { size_um: .24, min_spacing_um: .08, enclosure_um: .02 },
        gate_extension_um: .2,
        well_enclosure_um: .3,
        layer_overrides: {},
        via_overrides: {},
      },
      physical_planning: {
        placement_site_width_um: .5,
        row_height_um: 1.7,
        target_device_density: .65,
        target_routing_utilization: .75,
        floorplan_growth_factor: .08,
        max_floorplan_growth_passes: 3,
        global_route_max_iterations: 30,
        global_route_stall_iterations: 3,
        detailed_route_max_iterations: 10,
        routing_layers: {
          metal1: { pitch_um: .32, offset_um: .16, preferred_direction: "horizontal", capacity_adjustment: .75, reserved_for_power: true },
          metal2: { pitch_um: .32, offset_um: .16, preferred_direction: "vertical", capacity_adjustment: .75, reserved_for_power: false },
          metal3: { pitch_um: .32, offset_um: .16, preferred_direction: "horizontal", capacity_adjustment: .75, reserved_for_power: false },
          metal4: { pitch_um: .32, offset_um: .16, preferred_direction: "vertical", capacity_adjustment: .75, reserved_for_power: false },
          metal5: { pitch_um: .32, offset_um: .16, preferred_direction: "horizontal", capacity_adjustment: .75, reserved_for_power: false },
        },
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

export async function setTimingTarget(targetNs: number | null): Promise<WorkspaceState> {
  return invoke("set_timing_target", { targetNs });
}

export async function setHighFanoutWarningThreshold(threshold: number): Promise<WorkspaceState> {
  return invoke("set_high_fanout_warning_threshold", { threshold });
}

export async function validateProject(): Promise<ValidationReport> {
  return invoke("validate_project");
}

export async function parseVerilog(source: string): Promise<RtlModule> {
  return invoke("parse_verilog", { source });
}

export async function readVerilogSource(path: string): Promise<string> {
  return invoke("read_verilog_source", { path });
}

export async function importVerilog(source: string): Promise<WorkspaceState> {
  return invoke("import_verilog", { source });
}

export async function exportVerilog(): Promise<string> {
  return invoke("export_verilog");
}

export async function saveTextFile(path: string, data: string): Promise<string> {
  return invoke("save_text_file", { path, data });
}

export async function generatePhysicalIr(): Promise<PhysicalLayoutIr> {
  return invoke("generate_physical_ir");
}

export async function validatePhysicalLayout(): Promise<PhysicalDrcReport> {
  return invoke("validate_physical_layout");
}

export async function inspectPhysicalLayout(): Promise<{ layout: PhysicalLayoutIr; drc: PhysicalDrcReport; buildReport: import("./types").PhysicalBuildReport }> {
  return invoke("inspect_physical_layout");
}

export async function savePhysicalLayout(path: string): Promise<string> {
  return invoke("save_physical_layout", { path });
}

export async function savePhysicalDrcReport(path: string, data: string): Promise<string> {
  return invoke("save_physical_drc_report", { path, data });
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

export async function setWaveformGroups(groups: WaveformGroup[]): Promise<WorkspaceState> {
  return invoke("set_waveform_groups", { groups });
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
