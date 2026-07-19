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
};

export type WorkspaceState = {
  project: Project;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
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
};

export type TransistorSimulationState = {
  componentId: string;
  name: string;
  state: "on" | "off" | "unknown";
};

export type SimulationResult = {
  nets: NamedLogicState[];
  outputs: NamedLogicState[];
  transistors: TransistorSimulationState[];
  wires: Array<{ wireId: string; state: LogicState }>;
  converged: boolean;
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
