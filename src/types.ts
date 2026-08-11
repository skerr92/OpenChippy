export type Position = {
  x: number;
  y: number;
  z: number;
};

export type GdsExportReport = {
  formatVersion: number;
  libraryName: string;
  topCell: string;
  databaseUnitsPerMicron: number;
  boundaryCount: number;
  labelCount: number;
  structureCount: number;
  referenceCount: number;
  layerBoundaryCounts: Record<string, number>;
  bounds: { minX: number; minY: number; maxX: number; maxY: number };
  byteCount: number;
  structurallyValid: boolean;
  diagnostics: string[];
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
  waveformGroups: WaveformGroup[];
  timingTargetNs: number | null;
  highFanoutWarningThreshold: number;
  rtlDesign: RtlDesign | null;
};

export type RtlModule = {
  name: string;
  parameters: Array<{ name: string; defaultExpression: string; defaultValue: number }>;
  ports: Array<{ name: string; direction: "input" | "output"; range: RtlRange | null }>;
  nets: Array<{ name: string; range: RtlRange | null }>;
  instances: Array<{
    name: string;
    cell: string;
    primitive: "and" | "or" | "xor" | "nand" | "nor" | "xnor" | "not" | "buf" | null;
    parameterOverrides: Array<{ name: string | null; expression: string; value: number }>;
    connections: string[];
  }>;
  assignments: Array<{
    target: string;
    expression: string;
    referencedSignals: string[];
  }>;
  sequentialProcesses: Array<{
    edge: "posedge" | "negedge";
    clock: string;
    target: string;
    expression: string;
    referencedSignals: string[];
  }>;
};

export type RtlRange = { msb: number; lsb: number; msbExpression: string | null; lsbExpression: string | null };

export type RtlDesign = {
  module: RtlModule;
  placements: Array<{
    instanceName: string;
    x: number;
    y: number;
    level: number;
  }>;
};

export type WaveformGroup = {
  id: string;
  name: string;
  signals: string[];
  radix: "binary" | "hex";
  collapsed: boolean;
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
  process_id: string;
  deck_revision: string;
  source: string;
  supply_voltage: number;
  max_metal_layers: number;
  nmos: MosTechnology;
  pmos: MosTechnology;
  physical_parasitics: PhysicalParasiticRules;
  tapeout_window: TapeoutWindow;
  physical_rules: PhysicalRuleDeck;
  physical_planning: PhysicalPlanningRules;
  gds_layers: GdsLayerMap;
};

export type GdsLayerMap = {
  format_version: number;
  label_datatype: number;
  layers: Record<string, Array<{ purpose: string; layer: number; datatype: number }>>;
};

export type TapeoutWindow = {
  format_version: number;
  name: string;
  width_um: number;
  height_um: number;
  edge_margin_um: number;
};

export type PhysicalParasiticRules = {
  format_version: number;
  wire_capacitance_ff_per_um: number;
  via_capacitance_ff: number;
  layer_capacitance_ff_per_um: Record<string, number>;
  via_capacitance_overrides_ff: Record<string, number>;
};

export type LayerRule = {
  min_width_um: number;
  min_spacing_um: number;
  min_area_um2: number;
};

export type CutRule = {
  size_um: number;
  min_spacing_um: number;
  enclosure_um: number;
};

export type PhysicalRuleDeck = {
  format_version: number;
  database_units_per_micron: number;
  manufacturing_grid_um: number;
  diffusion: LayerRule;
  poly: LayerRule;
  well: LayerRule;
  metal: LayerRule;
  contact: CutRule;
  via: CutRule;
  gate_extension_um: number;
  well_enclosure_um: number;
  layer_overrides: Record<string, LayerRule>;
  via_overrides: Record<string, CutRule>;
};

export type RoutingDirection = "horizontal" | "vertical" | "any";

export type RoutingLayerResource = {
  pitch_um: number;
  offset_um: number;
  preferred_direction: RoutingDirection;
  capacity_adjustment: number;
  reserved_for_power: boolean;
};

export type PhysicalPlanningRules = {
  placement_site_width_um: number;
  row_height_um: number;
  target_device_density: number;
  target_routing_utilization: number;
  floorplan_growth_factor: number;
  max_floorplan_growth_passes: number;
  global_route_max_iterations: number;
  global_route_stall_iterations: number;
  detailed_route_max_iterations: number;
  routing_layers: Record<string, RoutingLayerResource>;
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
  technologyFingerprint: string;
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
  planning: {
    placementSiteWidthUm: number;
    rowHeightUm: number;
    targetDeviceDensity: number;
    targetRoutingUtilization: number;
    floorplanGrowthFactor: number;
    maxFloorplanGrowthPasses: number;
    globalRouteMaxIterations: number;
    globalRouteStallIterations: number;
    detailedRouteMaxIterations: number;
    routingLayers: Array<{
      layer: number;
      pitchUm: number;
      offsetUm: number;
      preferredDirection: RoutingDirection;
      capacityAdjustment: number;
      reservedForPower: boolean;
    }>;
    nets: Array<{
      net: number;
      name: string;
      class: "power" | "global" | "signal";
      priority: number;
      terminalCount: number;
      estimatedDemand: number;
    }>;
    candidates: Array<{
      id: number;
      strategy: "square" | "balanced" | "topology";
      widthUm: number;
      heightUm: number;
      columns: number;
      pmosRows: number;
      nmosRows: number;
      growthPasses: number;
      deviceDensity: number;
      estimatedRoutingUtilization: number;
      estimatedRoutingDemand: number;
      estimatedRoutingCapacity: number;
      pinAccessPoints: number;
      feasible: boolean;
      score: number;
    }>;
    selectedCandidate: number;
    coarseBinSizeUm: number;
    binColumns: number;
    binRows: number;
    routingBins: Array<{
      id: number;
      column: number;
      row: number;
      minX: number;
      minY: number;
      maxX: number;
      maxY: number;
      layerCapacities: Array<{
        layer: number;
        horizontalTracks: number;
        verticalTracks: number;
        reservedForPower: boolean;
      }>;
    }>;
  };
  placement: {
    devicePitchUm: number;
    candidates: Array<{
      id: number;
      strategy: "hierarchy" | "topology" | "diffusion" | "congestion" | "timing";
      devices: Array<{
        componentId: string;
        name: string;
        kind: "nmos" | "pmos";
        x: number;
        y: number;
        row: number;
        column: number;
        site: number;
      }>;
      blockRegions: Array<{
        name: string;
        minX: number;
        minY: number;
        maxX: number;
        maxY: number;
        deviceCount: number;
        immutable: boolean;
      }>;
      legal: boolean;
      estimatedWireLengthUm: number;
      peakBinUtilization: number;
      diffusionSharingPairs: number;
      gateStrapPairs: number;
      occupancyRetries: number;
      reservedDeviceShapes: number;
      standardCellInstances: number;
      topologyCompacted: boolean;
      score: number;
    }>;
    selectedCandidate: number;
  };
  globalRouting: {
    routes: Array<{
      net: number;
      name: string;
      class: "power" | "global" | "signal";
      priority: number;
      segments: Array<{ fromBin: number; toBin: number; layer: number }>;
      estimatedLengthUm: number;
      viaCount: number;
      ripUpCount: number;
    }>;
    iterations: Array<{
      iteration: number;
      overflow: number;
      reroutedNets: number;
      bestSoFar: boolean;
    }>;
    totalOverflow: number;
    converged: boolean;
    maxIterations: number;
    stallLimit: number;
  };
  detailedRouting: {
    routes: Array<{
      net: number;
      name: string;
      priority: number;
      polygons: Array<{
        layer: string;
        x: number;
        y: number;
        width: number;
        height: number;
        componentId: string | null;
        net: number | null;
      }>;
      pinAccessPoints: number;
      blockedPinAccessPoints: number;
      wireLengthUm: number;
      layerWireLengthsUm: Record<string, number>;
      viaCount: number;
      viaCounts: Record<string, number>;
      repairCount: number;
      rejectedGeometryCount: number;
      trackRetryCount: number;
      layerEscalationCount: number;
    }>;
    iterations: Array<{
      iteration: number;
      conflictCount: number;
      reroutedNets: number;
      bestSoFar: boolean;
    }>;
    conflictCount: number;
    blockedPinCount: number;
    converged: boolean;
    maxIterations: number;
    totalWireLengthUm: number;
    totalViaCount: number;
    rejectedGeometryCount: number;
    seededDeviceShapeCount: number;
    trackRetryCount: number;
    layerEscalationCount: number;
  };
  timing: {
    nets: Array<{
      net: number;
      name: string;
      fanout: number;
      wireLengthUm: number;
      viaCount: number;
      routedCapacitanceFf: number;
      deviceCapacitanceFf: number;
      totalCapacitanceFf: number;
      estimatedDelayNs: number;
    }>;
    paths: Array<{
      inputPin: string;
      outputPin: string;
      nets: number[];
      netNames: string[];
      deviceIds: string[];
      deviceNames: string[];
      estimatedDelayNs: number;
      requiredTimeNs: number | null;
      slackNs: number | null;
    }>;
    candidates: Array<{
      candidate: number;
      floorplanCandidate: number;
      placementCandidate: number;
      strategy: "square" | "balanced" | "topology";
      timingDriven: boolean;
      areaUm2: number;
      totalWireLengthUm: number;
      totalViaCount: number;
      routingOverflow: number;
      detailConflicts: number;
      estimatedWorstDelayNs: number;
      slackNs: number | null;
      meetsTiming: boolean | null;
    }>;
    selectedCandidate: number | null;
    criticalPath: number | null;
    timingTargetNs: number | null;
    worstSlackNs: number | null;
    criticalNet: number | null;
    criticalNetName: string | null;
    estimatedWorstDelayNs: number;
  };
  tapeout: {
    name: string;
    widthUm: number;
    heightUm: number;
    edgeMarginUm: number;
    usableWidthUm: number;
    usableHeightUm: number;
    geometryWidthUm: number;
    geometryHeightUm: number;
    areaUtilization: number;
    fits: boolean;
    shapesOutsideFloorplan: number;
    shapesOutsideTapeout: number;
  };
  density: {
    layers: Array<{
      material: string;
      preferredDensity: number;
      minimumGlobalDensity: number;
      maximumGlobalDensity: number | null;
      achievedGlobalDensity: number;
      minimumWindowDensity: number;
      maximumWindowDensity: number | null;
      achievedMinimumWindowDensity: number;
      achievedMaximumWindowDensity: number;
      windowWidthUm: number;
      windowHeightUm: number;
      windowCount: number;
      underfilledWindowCount: number;
      overfilledWindowCount: number;
      dummyShapeCount: number;
    }>;
  };
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
    purpose:
      | "unknown"
      | "fabric"
      | "active"
      | "gate"
      | "gate_access"
      | "contact"
      | "device_landing"
      | "pin"
      | "power_rail"
      | "route"
      | "route_fill"
      | "dummy_fill"
      | "via_landing"
      | "via"
      | "tap";
  }>;
  rowTopology: Array<{
    kind: "nmos" | "pmos";
    y: number;
    orderedDevices: string[];
    islands: Array<{
      layer: string;
      deviceIds: string[];
      terminalNets: number[];
      accesses: Array<{
        net: number;
        deviceIds: string[];
        x: number;
        y: number;
        sharedContact: boolean;
      }>;
      bounds: { minX: number; minY: number; maxX: number; maxY: number };
      geometry: Array<{
        minX: number;
        minY: number;
        maxX: number;
        maxY: number;
      }>;
    }>;
    gateStraps: Array<{
      net: number;
      deviceIds: string[];
      bounds: { minX: number; minY: number; maxX: number; maxY: number };
      geometry: Array<{
        minX: number;
        minY: number;
        maxX: number;
        maxY: number;
      }>;
    }>;
  }>;
  routeQuality: {
    routedShapeAreaUm2: number;
    routingBboxAreaUm2: number;
    maxDeviceEnvelopeExcursionUm: number;
    maxTerminalEnvelopeExcursionUm: number;
    largestTerminalExcursions: Array<{
      shapeIndex: number;
      net: number | null;
      layer: string;
      purpose: string;
      excursionUm: number;
      bounds: { minX: number; minY: number; maxX: number; maxY: number };
    }>;
    terminalFreeComponentCount: number;
    terminalFreeShapeCount: number;
    terminalFreeComponents: Array<{
      net: number;
      shapeIndices: number[];
      layers: string[];
      purposes: string[];
      bounds: { minX: number; minY: number; maxX: number; maxY: number };
    }>;
    exactDuplicateViaCount: number;
    unlandedViaCount: number;
    unjustifiedRouteEndpointCount: number;
    unjustifiedRouteEndpoints: Array<{
      shapeIndex: number;
      net: number;
      layer: string;
      purpose: string;
      x: number;
      y: number;
    }>;
  };
  orphanRoutingShapesRemoved: number;
  physicalBlocks: Array<{
    instanceName: string;
    bounds: { minX: number; minY: number; maxX: number; maxY: number };
    deviceIds: string[];
    localNets: number[];
    interfacePins: Array<{ name: string; net: number; x: number; y: number }>;
    shapeIndices: number[];
    localDrcErrors: number;
    verified: boolean;
    immutable: boolean;
  }>;
  standardCellLibrary: {
    formatVersion: number;
    libraryName: string;
    processId: string;
    deckRevision: string;
    source: string;
    generated: boolean;
    cells: Array<{
      definition: {
        name: string;
        class: "combinational" | "pass_gate" | "sequential";
        inputs: string[];
        outputs: string[];
        powerPins: string[];
        function: string;
        transistorCount: number;
      };
      widthUm: number;
      heightUm: number;
      placementSites: number;
      pinLayer: number;
      powerLayer: number;
      nmosWidthUm: number;
      pmosWidthUm: number;
      gateLengthUm: number;
    }>;
  };
};

export type PhysicalDrcDiagnostic = {
  ruleId: string;
  severity: "error" | "warning";
  message: string;
  layer: string;
  shapeIndices: number[];
  measured: number;
  required: number;
  category: "deviceOverlap" | "metalOverlap" | "minimumSpacing" | "viaEnclosure" | "powerCollision" | "routingCongestion" | "boundaryViolation" | "geometry";
  origin: "placement" | "powerRouting" | "signalRouting" | "geometryGeneration" | "import";
};

export type PhysicalDrcReport = {
  diagnostics: PhysicalDrcDiagnostic[];
  errorCount: number;
  warningCount: number;
  byCategory: Record<string, number>;
  byOrigin: Record<string, number>;
};

export type NativeLvsReport = {
  matched: boolean;
  expectedDeviceCount: number;
  recognizedDeviceCount: number;
  expectedNetCount: number;
  openNetCount: number;
  shortCount: number;
  missingDevices: string[];
  missingTerminals: string[];
};

export type PhysicalBuildReport = {
  elapsedMs: number;
  stages: string[];
  globalRoutingOverflow: number;
  detailedRoutingConflicts: number;
  rejectedGeometryCount: number;
  orphanRoutingShapesRemoved: number;
};

export type PhysicalBuildProgress = {
  stage: string;
  percent: number;
  elapsedMs: number;
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
  fanout: {
    highFanoutThreshold: number;
    highFanoutCount: number;
    undrivenCount: number;
    multiplyDrivenCount: number;
    nets: Array<{
      name: string;
      aliases: string[];
      drivers: FanoutEndpoint[];
      loads: FanoutEndpoint[];
      fanout: number;
      highFanout: boolean;
      undriven: boolean;
      multiplyDriven: boolean;
    }>;
    groups: Array<{
      name: string;
      signals: string[];
      totalFanout: number;
      maxFanout: number;
    }>;
  };
};

export type FanoutEndpoint = {
  componentId: string | null;
  name: string;
  kind: string;
  terminal: string;
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
