export type Position = {
  x: number;
  y: number;
  z: number;
};

export type Component = {
  id: string;
  kind: string;
  name: string;
  position: Position;
  rotation: number;
  deviceGeometry: DeviceGeometry | null;
  blockDefinitionId: string | null;
};

export type DeviceGeometry = {
  widthUm: number;
  lengthUm: number;
};

export type DeviceCharacteristics = DeviceGeometry & {
  effectiveOnResistanceOhms: number;
  gateCapacitanceFf: number;
  diffusionCapacitanceFf: number;
};

export type ComponentKind =
  | "nmos"
  | "pmos"
  | "vdd"
  | "gnd"
  | "input"
  | "output"
  | "junction"
  | "net_label"
  | "resistor";

export type TerminalRef = {
  componentId: string;
  terminal: string;
};

export type Wire = {
  id: string;
  from: TerminalRef;
  to: TerminalRef | null;
  end: Position | null;
  waypoints: Position[];
  routeX: number | null;
};

export type Project = {
  formatVersion: number;
  name: string;
  components: Component[];
  wires: Wire[];
  technology: Technology;
  blockDefinitions: BlockDefinition[];
};

export type BlockPin = {
  name: string;
  role: "input" | "output" | "power" | "ground";
  componentId: string;
  terminal: string;
};

export type BlockDefinition = {
  id: string;
  name: string;
  components: Component[];
  wires: Wire[];
  pins: BlockPin[];
};

export type MosTechnology = {
  threshold_voltage: number;
  nominal_on_resistance_ohms: number;
  reference_width_um: number;
  reference_length_um: number;
  gate_capacitance_ff_per_um: number;
  diffusion_capacitance_ff_per_um: number;
};

export type Technology = {
  format_version: number;
  name: string;
  supply_voltage: number;
  max_metal_layers: number;
  nmos: MosTechnology;
  pmos: MosTechnology;
};

export type WorkspaceState = {
  project: Project;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
};

export type PhysicalLayoutIr = {
  formatVersion: number;
  sourceProjectName: string;
  technologyName: string;
  maxMetalLayers: number;
  devices: Array<{
    componentId: string;
    name: string;
    kind: "nmos" | "pmos";
    gateNet: number;
    drainNet: number;
    sourceNet: number;
    widthUm: number;
    lengthUm: number;
  }>;
  nets: Array<{
    id: number;
    name: string;
    role: "power" | "ground" | "input" | "output" | "internal";
    terminals: Array<{
      componentId: string;
      componentName: string;
      terminal: string;
    }>;
  }>;
  pins: Array<{
    componentId: string;
    name: string;
    role: "power" | "ground" | "input" | "output" | "internal";
    net: number;
  }>;
  bounds: {
    minX: number;
    minY: number;
    maxX: number;
    maxY: number;
  };
  shapes: Array<{
    layer: string;
    x: number;
    y: number;
    width: number;
    height: number;
    componentId: string | null;
    net: number | null;
  }>;
};

export type Diagnostic = {
  code: string;
  severity: "error" | "warning";
  message: string;
  componentIds: string[];
};

export type ValidationReport = {
  diagnostics: Diagnostic[];
  errorCount: number;
  warningCount: number;
};

export type LogicState = "HIGH" | "LOW" | "FLOATING" | "CONTENDED" | "UNKNOWN";

export type NamedLogicState = {
  name: string;
  state: LogicState;
  voltage: number | null;
  highDriveResistanceOhms: number | null;
  lowDriveResistanceOhms: number | null;
  loadCapacitanceFf: number;
  estimatedDelayNs: number | null;
};

export type TransistorSimulationState = {
  componentId: string;
  name: string;
  state: "on" | "off" | "unknown";
  gateVoltage: number | null;
  thresholdVoltage: number;
  effectiveOnResistanceOhms: number;
};

export type SimulationResult = {
  nets: NamedLogicState[];
  outputs: NamedLogicState[];
  transistors: TransistorSimulationState[];
  wires: Array<{ wireId: string; state: LogicState }>;
  converged: boolean;
  supplyVoltage: number;
};

export type TruthTableResult = {
  inputNames: string[];
  outputNames: string[];
  rows: Array<{
    inputs: LogicState[];
    outputs: LogicState[];
    converged: boolean;
  }>;
};

export type WaveformConfig = {
  durationNs: number;
  clockPeriodNs: number;
  inputChangeNs: number;
};

export type WaveformResult = {
  durationNs: number;
  signals: Array<{
    name: string;
    kind: "input" | "output";
    samples: Array<{ timeNs: number; state: LogicState }>;
  }>;
};
