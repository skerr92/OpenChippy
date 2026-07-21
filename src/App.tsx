import { useEffect, useMemo, useRef, useState } from "react";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import * as backend from "./backend";
import type { ComponentKind, DeviceCharacteristics, LogicState, PhysicalDrcReport, PhysicalLayoutIr, SimulationResult, TerminalRef, TruthTableResult, ValidationReport, WaveformConfig, WaveformGroup, WaveformResult, WorkspaceState } from "./types";
import PhysicalViewport from "./PhysicalViewport";
import SchematicViewport from "./SchematicViewport";
import WaveformView from "./WaveformView";
import { isEditableTarget, isEditingText } from "./dom";

type ViewMode = "schematic" | "3d" | "waveform";

export default function App() {
  const editableFocusRef = useRef(false);
  const [workspace, setWorkspace] = useState<WorkspaceState | null>(null);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [selectedWireId, setSelectedWireId] = useState<string | null>(null);
  const [status, setStatus] = useState("Starting workspace…");
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>("schematic");
  const [placementKind, setPlacementKind] = useState<ComponentKind | null>(null);
  const [blockPlacementId, setBlockPlacementId] = useState<string | null>(null);
  const [pendingTerminal, setPendingTerminal] = useState<TerminalRef | null>(null);
  const [pendingWireId, setPendingWireId] = useState<string | null>(null);
  const [validation, setValidation] = useState<ValidationReport | null>(null);
  const [simulation, setSimulation] = useState<SimulationResult | null>(null);
  const [inputStates, setInputStates] = useState<Record<string, LogicState>>({});
  const [truthTable, setTruthTable] = useState<TruthTableResult | null>(null);
  const [waveformConfig, setWaveformConfig] = useState<WaveformConfig>({
    durationNs: 100,
    clockPeriodNs: 10,
    inputChangeNs: 20,
  });
  const [waveform, setWaveform] = useState<WaveformResult | null>(null);
  const [waveformRunning, setWaveformRunning] = useState(false);
  const [deviceCharacteristics, setDeviceCharacteristics] = useState<DeviceCharacteristics | null>(null);
  const [physicalIr, setPhysicalIr] = useState<PhysicalLayoutIr | null>(null);
  const [physicalDrc, setPhysicalDrc] = useState<PhysicalDrcReport | null>(null);
  const [blockDialogOpen, setBlockDialogOpen] = useState(false);
  const [blockName, setBlockName] = useState("");
  const [blockDialogError, setBlockDialogError] = useState<string | null>(null);
  const [blockSaving, setBlockSaving] = useState(false);
  const [schematicFitRevision, setSchematicFitRevision] = useState(0);
  const project = workspace?.project ?? null;
  const selectedComponents = useMemo(
    () => project?.components.filter(({ id }) => selectedIds.includes(id)) ?? [],
    [project, selectedIds],
  );
  const selected = selectedComponents.length === 1 ? selectedComponents[0] : null;
  const selectedWire = project?.wires.find(({ id }) => id === selectedWireId) ?? null;
  const digitalInputs = useMemo(
    () => project?.components.filter(({ kind }) => kind === "input") ?? [],
    [project],
  );

  useEffect(() => {
    setInputStates((current) => Object.fromEntries(
      digitalInputs.map((input) => [input.name, current[input.name] ?? "LOW"]),
    ));
  }, [digitalInputs]);

  useEffect(() => {
    if (!selected || (selected.kind !== "nmos" && selected.kind !== "pmos")) {
      setDeviceCharacteristics(null);
      return;
    }
    let active = true;
    backend.deviceCharacteristics(selected.id)
      .then((characteristics) => {
        if (active) setDeviceCharacteristics(characteristics);
      })
      .catch((reason) => {
        if (active) showError(reason);
      });
    return () => {
      active = false;
    };
  }, [selected]);

  useEffect(() => {
    if (viewMode !== "3d" || !project) {
      setPhysicalIr(null);
      setPhysicalDrc(null);
      return;
    }
    setPhysicalIr(null);
    setPhysicalDrc(null);
    let active = true;
    backend.inspectPhysicalLayout()
      .then(({ layout: ir, drc: report }) => {
        if (active) {
          setPhysicalIr(ir);
          setPhysicalDrc(report);
        }
      })
      .catch((reason) => {
        if (active) showError(reason);
      });
    return () => {
      active = false;
    };
  }, [viewMode, project]);

  const savePhysicalReport = async () => {
    if (!physicalIr || !physicalDrc) return;
    try {
      const destination = await save({
        defaultPath: `${project?.name ?? "OpenChippy"}.physical-drc.json`,
        filters: [{ name: "Physical DRC report", extensions: ["json"] }],
      });
      if (!destination) return;
      const candidate = physicalIr.planning.candidates[physicalIr.planning.selectedCandidate];
      const placement = physicalIr.placement.candidates[physicalIr.placement.selectedCandidate];
      const artifact = {
        formatVersion: 1,
        generatedBy: "OpenChippy",
        project: physicalIr.sourceProjectName,
        technology: physicalIr.technologyName,
        physicalIrVersion: physicalIr.formatVersion,
        physicalIr,
        selectedCandidate: {
          planning: candidate,
          placement,
          globalRouting: physicalIr.globalRouting,
          detailedRouting: physicalIr.detailedRouting,
        },
        report: physicalDrc,
      };
      await backend.savePhysicalDrcReport(destination, JSON.stringify(artifact, null, 2));
      setStatus(`Saved physical DRC report to ${destination}`);
    } catch (reason) {
      showError(reason);
    }
  };

  useEffect(() => {
    backend.createProject()
      .then((value) => {
        setWorkspace(value);
        setSchematicFitRevision((revision) => revision + 1);
        setStatus(backend.inTauri() ? "Desktop backend ready" : "Browser preview");
      })
      .catch(showError);
  }, []);

  useEffect(() => {
    const cancelPlacement = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setPlacementKind(null);
        setBlockPlacementId(null);
        setPendingTerminal(null);
        setPendingWireId(null);
        setBlockDialogOpen(false);
        setBlockDialogError(null);
        setStatus("Placement cancelled");
      }
    };
    window.addEventListener("keydown", cancelPlacement);
    return () => window.removeEventListener("keydown", cancelPlacement);
  }, []);

  const showError = (reason: unknown) => {
    setError(reason instanceof Error ? reason.message : String(reason));
    setStatus("Action failed");
  };

  const replaceWorkspace = (next: WorkspaceState, message: string, fitDesign = false) => {
    setWorkspace(next);
    if (fitDesign) setSchematicFitRevision((revision) => revision + 1);
    setSelectedIds([]);
    setSelectedWireId(null);
    setError(null);
    setValidation(null);
    setSimulation(null);
    setTruthTable(null);
    setWaveform(null);
    setStatus(message);
  };

  const placeComponent = async (x: number, y: number) => {
    if (!placementKind && !blockPlacementId) return;
    try {
      if (blockPlacementId) {
        const definition = project?.blockDefinitions.find(({ id }) => id === blockPlacementId);
        replaceWorkspace(await backend.placeBlock(blockPlacementId, x, y), `Placed ${definition?.name ?? "block"} at (${x}, ${y})`);
      } else if (placementKind) {
        replaceWorkspace(await backend.addComponent(placementKind, x, y), `Placed ${placementKind.toUpperCase()} at (${x}, ${y})`);
      }
      setPlacementKind(null);
      setBlockPlacementId(null);
    } catch (reason) {
      showError(reason);
    }
  };

  const armPlacement = (kind: ComponentKind) => {
    setViewMode("schematic");
    setPlacementKind(kind);
    setBlockPlacementId(null);
    setPendingTerminal(null);
    setPendingWireId(null);
    setStatus(`Click a grid point to place ${kind.toUpperCase()} · Esc to cancel`);
  };

  const armBlockPlacement = (definitionId: string) => {
    const definition = project?.blockDefinitions.find(({ id }) => id === definitionId);
    setViewMode("schematic");
    setPlacementKind(null);
    setBlockPlacementId(definitionId);
    setPendingTerminal(null);
    setPendingWireId(null);
    setStatus(`Click a grid point to place ${definition?.name ?? "block"} · Esc to cancel`);
  };

  const openBlockDialog = () => {
    setBlockName(project?.name.replaceAll(" ", "_") ?? "Device_Block");
    setBlockDialogError(null);
    setBlockDialogOpen(true);
  };

  const captureCurrentBlock = async () => {
    const name = blockName.trim();
    if (!name) {
      setBlockDialogError("Enter a name for the reusable block.");
      return;
    }
    setBlockSaving(true);
    setBlockDialogError(null);
    try {
      replaceWorkspace(await backend.captureBlock(name), `Saved ${name.trim()} as a reusable block`);
      setBlockDialogOpen(false);
    } catch (reason) {
      setBlockDialogError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setBlockSaving(false);
    }
  };

  const exportDefinition = async (definitionId: string, name: string) => {
    try {
      const path = await backend.exportBlock(definitionId);
      setStatus(`Exported ${name} to ${path}`);
    } catch (reason) {
      showError(reason);
    }
  };

  const moveComponent = async (id: string, x: number, y: number) => {
    try {
      replaceWorkspace(await backend.moveComponent(id, x, y), `Moved component to (${x}, ${y})`);
      setSelectedIds([id]);
    } catch (reason) {
      showError(reason);
    }
  };

  const moveWire = async (id: string, routeX: number) => {
    try {
      replaceWorkspace(await backend.moveWire(id, routeX), `Moved wire route to X=${routeX}`);
      setSelectedWireId(id);
    } catch (reason) {
      showError(reason);
    }
  };

  const deleteSelectedWire = async () => {
    if (!selectedWireId) return;
    try {
      replaceWorkspace(await backend.deleteWire(selectedWireId), "Deleted wire");
    } catch (reason) {
      showError(reason);
    }
  };

  const chooseTerminal = async (terminal: TerminalRef) => {
    if (pendingWireId) {
      try {
        replaceWorkspace(await backend.finishWire(
          pendingWireId,
          terminal.componentId,
          terminal.terminal,
        ), "Connected wire to terminal");
        setPendingWireId(null);
      } catch (reason) {
        setPendingWireId(null);
        showError(reason);
      }
      return;
    }
    if (!pendingTerminal) {
      setPendingTerminal(terminal);
      setStatus("Select a terminal or click the grid to end the wire · Esc to cancel");
      return;
    }
    try {
      replaceWorkspace(await backend.connectTerminals(
        pendingTerminal.componentId, pendingTerminal.terminal,
        terminal.componentId, terminal.terminal,
      ), "Connected terminals");
      setPendingTerminal(null);
    } catch (reason) {
      setPendingTerminal(null);
      showError(reason);
    }
  };

  const connectToFreePoint = async (x: number, y: number) => {
    try {
      if (pendingWireId) {
        const next = await backend.extendWire(pendingWireId, x, y);
        replaceWorkspace(next, `Added route bend at (${x}, ${y})`);
        setPendingWireId(pendingWireId);
        setSelectedWireId(pendingWireId);
      } else if (pendingTerminal) {
        const next = await backend.connectToPoint(
          pendingTerminal.componentId,
          pendingTerminal.terminal,
          x,
          y,
        );
        const wire = [...next.project.wires].reverse().find((item) => !item.to);
        replaceWorkspace(next, `Started wire route at (${x}, ${y})`);
        setPendingTerminal(null);
        setPendingWireId(wire?.id ?? null);
        setSelectedWireId(wire?.id ?? null);
      }
    } catch (reason) {
      setPendingTerminal(null);
      setPendingWireId(null);
      showError(reason);
    }
  };

  const chooseDanglingEnd = (wireId: string) => {
    setPendingTerminal(null);
    setPendingWireId(wireId);
    setSelectedWireId(wireId);
    setStatus("Select a terminal to finish this wire · Esc to cancel");
  };

  const selectComponent = (id: string | null, additive = false) => {
    if (!id) {
      setSelectedIds([]);
      return;
    }
    setSelectedWireId(null);
    setSelectedIds((current) => {
      if (!additive) return [id];
      return current.includes(id) ? current.filter((selected) => selected !== id) : [...current, id];
    });
  };

  const transformSelection = async (action: "rotate" | "delete") => {
    if (!selectedIds.length) return;
    try {
      const next = action === "rotate"
        ? await backend.rotateComponents(selectedIds)
        : await backend.deleteComponents(selectedIds);
      replaceWorkspace(next, `${action === "rotate" ? "Rotated" : "Deleted"} ${selectedIds.length} component${selectedIds.length === 1 ? "" : "s"}`);
    } catch (reason) {
      showError(reason);
    }
  };

  const renameSelected = async (name: string) => {
    if (!selected || name.trim() === selected.name) return;
    const componentId = selected.id;
    const previousName = selected.name;
    try {
      const next = await backend.renameComponent(componentId, name);
      setWorkspace(next);
      setError(null);
      setValidation(null);
      setSimulation(null);
      setTruthTable(null);
      setWaveform(null);
      setStatus(`Renamed ${previousName} to ${name.trim()}`);
    } catch (reason) {
      showError(reason);
    }
  };

  const renameProject = async (name: string) => {
    if (!project || name.trim() === project.name) return;
    try {
      replaceWorkspace(await backend.renameProject(name), `Renamed circuit to ${name.trim()}`);
    } catch (reason) {
      showError(reason);
    }
  };

  const updateDeviceGeometry = async (widthUm: number, lengthUm: number) => {
    if (!selected || !deviceCharacteristics) return;
    if (
      widthUm === deviceCharacteristics.widthUm
      && lengthUm === deviceCharacteristics.lengthUm
    ) return;
    try {
      replaceWorkspace(
        await backend.setDeviceGeometry(selected.id, widthUm, lengthUm),
        `Updated ${selected.name} geometry to W=${widthUm} µm, L=${lengthUm} µm`,
      );
      setSelectedIds([selected.id]);
    } catch (reason) {
      showError(reason);
    }
  };

  const runDrc = async () => {
    try {
      const report = await backend.validateProject();
      setValidation(report);
      setSelectedIds([]);
      setError(null);
      setStatus(
        report.diagnostics.length
          ? `DRC found ${report.errorCount} errors and ${report.warningCount} warnings`
          : "DRC passed with no issues",
      );
    } catch (reason) {
      showError(reason);
    }
  };

  const runSimulation = async (inputs = inputStates) => {
    try {
      const result = await backend.simulateProject(inputs);
      setSimulation(result);
      setValidation(null);
      setError(null);
      setStatus(result.converged ? "Simulation converged" : "Simulation did not converge");
    } catch (reason) {
      showError(reason);
    }
  };

  const toggleInput = (name: string) => {
    const sequence: LogicState[] = ["LOW", "HIGH", "UNKNOWN"];
    const current = inputStates[name] ?? "LOW";
    const next: Record<string, LogicState> = {
      ...inputStates,
      [name]: sequence[(sequence.indexOf(current) + 1) % sequence.length],
    };
    setInputStates(next);
    void runSimulation(next);
  };

  const generateTruthTable = async () => {
    try {
      const table = await backend.generateTruthTable();
      setTruthTable(table);
      setError(null);
      setStatus(`Generated ${table.rows.length}-row truth table`);
    } catch (reason) {
      showError(reason);
    }
  };

  const runWaveform = async () => {
    setWaveformRunning(true);
    try {
      const result = await backend.simulateWaveform(waveformConfig);
      setWaveform(result);
      setError(null);
      setStatus(`Generated ${result.durationNs} ns waveform`);
    } catch (reason) {
      showError(reason);
    } finally {
      setWaveformRunning(false);
    }
  };

  const updateWaveformGroups = async (groups: WaveformGroup[]) => {
    try {
      const next = await backend.setWaveformGroups(groups);
      setWorkspace(next);
      setError(null);
      setStatus("Updated waveform groups");
    } catch (reason) {
      showError(reason);
    }
  };

  useEffect(() => {
    const editSelection = (event: KeyboardEvent) => {
      if (editableFocusRef.current || isEditingText(event)) return;
      if (event.key === "Delete" || event.key === "Backspace") {
        if (!selectedIds.length && !selectedWireId) return;
        event.preventDefault();
        if (selectedWireId) void deleteSelectedWire();
        else void transformSelection("delete");
      } else if (event.key.toLowerCase() === "r" && !event.metaKey && !event.ctrlKey) {
        if (!selectedIds.length) return;
        event.preventDefault();
        void transformSelection("rotate");
      }
    };
    window.addEventListener("keydown", editSelection);
    return () => window.removeEventListener("keydown", editSelection);
  }, [selectedIds, selectedWireId]);

  const confirmDiscard = async () => {
    if (!workspace?.dirty) return true;
    return ask("This project has unsaved changes. Discard them?", {
      title: "Unsaved changes",
      kind: "warning",
      okLabel: "Discard",
      cancelLabel: "Keep editing",
    });
  };

  const saveCurrent = async (saveAs = false) => {
    try {
      let path = saveAs ? null : workspace?.path ?? null;
      if (!path) {
        path = await save({
          defaultPath: `${project?.name ?? "project"}.chippy`,
          filters: [{ name: "OpenChippy", extensions: ["chippy"] }],
        });
      }
      if (path) {
        replaceWorkspace(
          await backend.saveProject(path),
          `Saved ${path.split(/[/\\]/).pop()}`,
        );
      }
    } catch (reason) {
      showError(reason);
    }
  };

  const loadExisting = async () => {
    try {
      if (!(await confirmDiscard())) return;
      const path = await open({ multiple: false, filters: [{ name: "OpenChippy", extensions: ["chippy"] }] });
      if (typeof path === "string") replaceWorkspace(await backend.loadProject(path), "Project loaded", true);
    } catch (reason) {
      showError(reason);
    }
  };

  const newProject = async () => {
    try {
      if (await confirmDiscard()) replaceWorkspace(await backend.createProject(), "New project", true);
    } catch (reason) {
      showError(reason);
    }
  };

  const loadTechnology = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "Technology YAML", extensions: ["yaml", "yml"] }],
      });
      if (typeof path === "string") {
        const next = await backend.loadTechnology(path);
        replaceWorkspace(next, `Loaded technology ${next.project.technology.name}`);
      }
    } catch (reason) {
      showError(reason);
    }
  };

  const resetTechnology = async () => {
    try {
      const next = await backend.resetTechnology();
      replaceWorkspace(next, "Restored built-in educational technology");
    } catch (reason) {
      showError(reason);
    }
  };

  const stepHistory = async (direction: "undo" | "redo") => {
    try {
      const next = direction === "undo" ? await backend.undo() : await backend.redo();
      replaceWorkspace(next, direction === "undo" ? "Undid last change" : "Redid last change");
    } catch (reason) {
      showError(reason);
    }
  };

  if (!project || !workspace) return <main className="loading">Preparing OpenChippy…</main>;

  return (
    <main className="app-shell"
      onPointerDownCapture={(event) => {
        const active = document.activeElement;
        if (
          isEditableTarget(active)
          && active instanceof HTMLElement
          && !active.contains(event.target as Node)
        ) {
          active.blur();
        }
      }}
      onFocusCapture={(event) => {
        if (isEditableTarget(event.target)) editableFocusRef.current = true;
      }}
      onBlurCapture={() => {
        queueMicrotask(() => {
          editableFocusRef.current = isEditableTarget(document.activeElement);
        });
      }}
      onKeyDown={(event) => {
        if (editableFocusRef.current || isEditableTarget(event.target)) event.stopPropagation();
      }}>
      <header>
        <div className="brand"><span className="mark">OC</span><div><strong>OpenChippy</strong><small>Silicon design studio</small></div></div>
        <nav>
          <button onClick={newProject}>New</button>
          <button onClick={loadExisting}>Open</button>
          <button onClick={() => saveCurrent()}>Save</button>
          <button onClick={() => saveCurrent(true)}>Save As</button>
          <span className="separator" />
          <button disabled={!workspace.canUndo} onClick={() => stepHistory("undo")}>Undo</button>
          <button disabled={!workspace.canRedo} onClick={() => stepHistory("redo")}>Redo</button>
          <button disabled={!selectedIds.length} onClick={() => transformSelection("rotate")}>Rotate</button>
          <button className="drc-button" onClick={runDrc}>Run DRC</button>
          <details className="simulation-menu">
            <summary className="simulate-button">Simulate <span>▾</span></summary>
            <div>
              <button disabled={!project.components.length} onClick={(event) => {
                event.currentTarget.closest("details")?.removeAttribute("open");
                void runSimulation();
              }}><strong>Operating point</strong><small>Resolve current input states</small></button>
              <button onClick={(event) => {
                event.currentTarget.closest("details")?.removeAttribute("open");
                setViewMode("waveform");
                setPlacementKind(null);
              }}><strong>Waveform view</strong><small>Run a timed input sequence</small></button>
            </div>
          </details>
          <button className="truth-table-button" disabled={!digitalInputs.length} onClick={generateTruthTable}>Truth Table</button>
          <button disabled={!project.components.length} onClick={openBlockDialog}>Save as Block</button>
        </nav>
      </header>
      <aside className="library">
        <p className="eyebrow">Library</p>
        <h2>CMOS Devices</h2>
        <button className={`component-card ${placementKind === "nmos" ? "active" : ""}`} onClick={() => armPlacement("nmos")}>
          <span className="component-icon transistor-glyph">N</span>
          <span><strong>NMOS</strong><small>Enhancement transistor</small></span>
        </button>
        <button className={`component-card ${placementKind === "pmos" ? "active" : ""}`} onClick={() => armPlacement("pmos")}>
          <span className="component-icon transistor-glyph pmos">P</span>
          <span><strong>PMOS</strong><small>Enhancement transistor</small></span>
        </button>
        <p className="library-section">Sources & Rails</p>
        {(["vdd", "gnd", "input"] as ComponentKind[]).map((kind) => (
          <button key={kind} className={`component-card ${placementKind === kind ? "active" : ""}`} onClick={() => armPlacement(kind)}>
            <span className="component-icon source-glyph">{kind === "vdd" ? "↑" : kind === "gnd" ? "⏚" : "→"}</span>
            <span><strong>{kind === "input" ? "Digital Input" : kind.toUpperCase()}</strong><small>{kind === "input" ? "Gate stimulus" : "Power rail"}</small></span>
          </button>
        ))}
        <p className="library-section">Nets & Analysis</p>
        {(["junction", "net_label", "output"] as ComponentKind[]).map((kind) => (
          <button key={kind} className={`component-card ${placementKind === kind ? "active" : ""}`} onClick={() => armPlacement(kind)}>
            <span className="component-icon net-glyph">{kind === "junction" ? "●" : kind === "net_label" ? "N" : "◈"}</span>
            <span>
              <strong>{kind === "junction" ? "Junction" : kind === "net_label" ? "Net Label" : "Output Probe"}</strong>
              <small>{kind === "junction" ? "Wire branch point" : kind === "net_label" ? "Named electrical net" : "Observe node state"}</small>
            </span>
          </button>
        ))}
        <p className="library-section">Characteristics</p>
        <button className={`component-card ${placementKind === "resistor" ? "active" : ""}`} onClick={() => armPlacement("resistor")}>
          <span className="component-icon resistor-glyph">╱╲╱</span>
          <span><strong>Resistor</strong><small>Two-terminal passive</small></span>
        </button>
        <p className="library-section">Reusable Blocks</p>
        {project.blockDefinitions.map((definition) => (
          <button key={definition.id} className={`component-card ${blockPlacementId === definition.id ? "active" : ""}`}
            onClick={() => armBlockPlacement(definition.id)}>
            <span className="component-icon block-glyph">▣</span>
            <span><strong>{definition.name}</strong><small>{definition.pins.length} pins · shared definition</small></span>
          </button>
        ))}
        {!project.blockDefinitions.length && <p className="library-empty">Save the current transistor circuit as a reusable block.</p>}
      </aside>
      <section className="stage">
        {viewMode !== "3d" && <div className="stage-label">
          <span>{workspace.dirty ? "● " : ""}{project.name}</span>
          <small>{workspace.path?.split(/[/\\]/).pop() ?? "Not saved"} · {project.components.length} components · {project.technology.name}</small>
        </div>}
        {viewMode !== "3d" && <div className="view-switch" role="group" aria-label="Editor view">
          <button className={viewMode === "schematic" ? "active" : ""} onClick={() => setViewMode("schematic")}>2D Schematic</button>
          <button onClick={() => { setViewMode("3d"); setPlacementKind(null); }}>3D View</button>
          <button className={viewMode === "waveform" ? "active" : ""} onClick={() => { setViewMode("waveform"); setPlacementKind(null); }}>Waveforms</button>
        </div>}
        {viewMode === "schematic" ? (
          <SchematicViewport
            fitRevision={schematicFitRevision}
            components={project.components}
            wires={project.wires}
            blockDefinitions={project.blockDefinitions}
            selectedIds={selectedIds}
            selectedWireId={selectedWireId}
            placementKind={placementKind}
            placementActive={Boolean(placementKind || blockPlacementId)}
            pendingTerminal={pendingTerminal}
            routingActive={Boolean(pendingTerminal || pendingWireId)}
            simulation={simulation}
            inputStates={inputStates}
            onSelect={selectComponent}
            onPlace={placeComponent}
            onMove={moveComponent}
            onWireSelect={setSelectedWireId}
            onWireMove={moveWire}
            onTerminal={chooseTerminal}
            onFreePoint={connectToFreePoint}
            onDanglingEnd={chooseDanglingEnd}
          />
        ) : viewMode === "3d" ? (
          <PhysicalViewport
            layout={physicalIr}
            drc={physicalDrc}
            selectedIds={selectedIds}
            projectName={project.name}
            technologyName={project.technology.name}
            onSelect={selectComponent}
            onSaveDrc={savePhysicalReport}
            onView={(view) => {
              setViewMode(view);
              setPlacementKind(null);
            }}
          />
        ) : (
          <WaveformView config={waveformConfig} result={waveform} running={waveformRunning}
            groups={project.waveformGroups} onGroups={updateWaveformGroups}
            onConfig={setWaveformConfig} onRun={runWaveform} />
        )}
        {viewMode === "schematic" && (digitalInputs.length > 0 || simulation) && (
          <div className="simulation-panel">
            <span className="simulation-title">Switch simulation</span>
            {digitalInputs.map((input) => (
              <button key={input.id} className={`logic-pill logic-${(inputStates[input.name] ?? "LOW").toLowerCase()}`}
                onClick={() => toggleInput(input.name)} title="Cycle LOW → HIGH → UNKNOWN">
                {input.name} <strong>{inputStates[input.name] ?? "LOW"}</strong>
              </button>
            ))}
            {simulation?.outputs.map((output) => (
              <span key={output.name} className={`logic-pill output logic-${output.state.toLowerCase()}`}>
                {output.name} <strong>{output.state}</strong>
              </span>
            ))}
            {simulation && <span className={`convergence ${simulation.converged ? "ok" : "failed"}`}>{simulation.converged ? "stable" : "unstable"}</span>}
          </div>
        )}
        {viewMode !== "waveform" && truthTable && (
          <div className="truth-table-panel">
            <div className="truth-table-heading">
              <div><strong>Truth table</strong><small>{truthTable.rows.length} combinations</small></div>
              <button onClick={() => setTruthTable(null)} aria-label="Close truth table">×</button>
            </div>
            <div className="truth-table-scroll">
              <table>
                <thead><tr>
                  {truthTable.inputNames.map((name) => <th key={`input-${name}`}>{name}</th>)}
                  {truthTable.outputNames.map((name) => <th key={`output-${name}`} className="output">{name}</th>)}
                </tr></thead>
                <tbody>
                  {truthTable.rows.map((row, rowIndex) => <tr key={rowIndex} className={row.converged ? "" : "unstable"}
                    onClick={() => {
                      const inputs = Object.fromEntries(
                        truthTable.inputNames.map((name, index) => [name, row.inputs[index]]),
                      );
                      setInputStates(inputs);
                      void runSimulation(inputs);
                    }}>
                    {row.inputs.map((state, index) => <td key={`input-${index}`} className={`logic-${state.toLowerCase()}`}>{state}</td>)}
                    {row.outputs.map((state, index) => <td key={`output-${index}`} className={`output logic-${state.toLowerCase()}`}>{state}</td>)}
                  </tr>)}
                </tbody>
              </table>
            </div>
          </div>
        )}
        {viewMode !== "waveform" && (placementKind || blockPlacementId || pendingTerminal || pendingWireId) && <div className="placement-hint">
          {placementKind || blockPlacementId
            ? `Place ${placementKind?.toUpperCase() ?? project.blockDefinitions.find(({ id }) => id === blockPlacementId)?.name ?? "BLOCK"} on grid`
            : pendingWireId
              ? "Select a terminal to finish this wire"
              : "Select a terminal or click the grid"}
          <button onClick={() => { setPlacementKind(null); setBlockPlacementId(null); setPendingTerminal(null); setPendingWireId(null); }}>Cancel</button>
        </div>}
        {viewMode === "schematic" && !placementKind && !blockPlacementId && !pendingTerminal && !pendingWireId && (
          <div className="viewport-help">Drag empty canvas / two-finger / arrows to pan · Shift-arrows pan faster · Pinch or Ctrl-wheel to zoom · F or Home fits the circuit · Click a terminal, then the grid, to leave a routed endpoint</div>
        )}
      </section>
      <aside className="properties">
        <p className="eyebrow">{validation ? "Design Rules" : simulation && !selectedWire && !selectedComponents.length ? "Simulation" : "Inspector"}</p>
        {validation ? (
          <>
            <div className={`drc-summary ${validation.errorCount ? "failed" : "passed"}`}>
              <strong>{validation.diagnostics.length ? `${validation.errorCount} errors · ${validation.warningCount} warnings` : "DRC passed"}</strong>
              <small>{validation.diagnostics.length ? "Review the issues below." : "No basic connectivity issues found."}</small>
            </div>
            <div className="diagnostic-list">
              {validation.diagnostics.map((diagnostic, index) => (
                <button key={`${diagnostic.code}-${index}`} className={`diagnostic ${diagnostic.severity}`}
                  onClick={() => {
                    setValidation(null);
                    setSelectedIds(diagnostic.componentIds);
                  }}>
                  <span>{diagnostic.severity}</span>
                  <strong>{diagnostic.code.replaceAll("_", " ")}</strong>
                  <small>{diagnostic.message}</small>
                </button>
              ))}
            </div>
            <button className="close-results" onClick={() => setValidation(null)}>Close results</button>
          </>
        ) : simulation && !selectedWire && !selectedComponents.length ? (
          <>
            <div className={`simulation-summary ${simulation.converged ? "passed" : "failed"}`}>
              <strong>{simulation.converged ? "Stable solution" : "Solver did not converge"}</strong>
              <small>Digital rails: 0 V / {simulation.supplyVoltage} V</small>
              <small>
                {simulation.outputs.some(({ state }) => state === "CONTENDED")
                  ? "One or more outputs have conflicting drivers."
                  : simulation.outputs.some(({ state }) => state === "FLOATING")
                    ? "One or more outputs have no active driver."
                    : simulation.outputs.some(({ state }) => state === "UNKNOWN")
                      ? "One or more outputs depend on an unknown condition."
                      : "All output probes resolved to a digital level."}
              </small>
            </div>
            <p className="inspector-section">Inputs</p>
            <div className="state-list">
              {digitalInputs.map((input) => (
                <button key={input.id} onClick={() => toggleInput(input.name)}>
                  <span>{input.name}</span>
                  <strong className={`logic-${(inputStates[input.name] ?? "LOW").toLowerCase()}`}>{inputStates[input.name] ?? "LOW"}</strong>
                </button>
              ))}
            </div>
            <p className="inspector-section">Outputs</p>
            <div className="state-list">
              {simulation.outputs.map((output) => (
                <button key={output.name} onClick={() => selectComponent(project.components.find((component) => component.name === output.name)?.id ?? null)}>
                  <span>
                    {output.name}
                    <small>
                      {output.voltage === null ? "no resolved voltage" : `${output.voltage} V`}
                      {output.state === "HIGH" && output.highDriveResistanceOhms !== null
                        ? ` · pull-up ${output.highDriveResistanceOhms.toLocaleString(undefined, { maximumFractionDigits: 1 })} Ω`
                        : output.state === "LOW" && output.lowDriveResistanceOhms !== null
                          ? ` · pull-down ${output.lowDriveResistanceOhms.toLocaleString(undefined, { maximumFractionDigits: 1 })} Ω`
                          : ""}
                      {` · ${output.loadCapacitanceFf.toLocaleString(undefined, { maximumFractionDigits: 3 })} fF`}
                      {output.estimatedDelayNs !== null
                        ? ` · ${output.estimatedDelayNs.toLocaleString(undefined, { maximumFractionDigits: 4 })} ns`
                        : ""}
                    </small>
                  </span>
                  <strong className={`logic-${output.state.toLowerCase()}`}>{output.state}</strong>
                </button>
              ))}
              {!simulation.outputs.length && <p className="selection-note">No output probes are placed.</p>}
            </div>
            <p className="inspector-section">Transistors</p>
            <div className="state-list transistor-states">
              {simulation.transistors.map((transistor) => (
                <button key={transistor.componentId} onClick={() => selectComponent(transistor.componentId)}>
                  <span>
                    {transistor.name}
                    <small>
                      Vg {transistor.gateVoltage === null ? "?" : `${transistor.gateVoltage} V`}
                      {" · "}Vt {transistor.thresholdVoltage} V
                      {" · "}{transistor.effectiveOnResistanceOhms.toLocaleString(undefined, { maximumFractionDigits: 1 })} Ω
                    </small>
                  </span>
                  <strong className={`switch-${transistor.state}`}>{transistor.state.toUpperCase()}</strong>
                </button>
              ))}
            </div>
            <details className="net-state-details">
              <summary>Nets ({simulation.nets.length})</summary>
              <div className="state-list">
                {simulation.nets.map((net) => (
                  <div key={net.name}><span>{net.name}</span><strong className={`logic-${net.state.toLowerCase()}`}>{net.state}</strong></div>
                ))}
              </div>
            </details>
            <button className="close-results" onClick={() => setSimulation(null)}>Clear simulation</button>
          </>
        ) : selectedWire ? (
          <>
            <h2>Wire route</h2>
            <dl>
              <dt>From</dt><dd>{selectedWire.from.terminal}</dd>
              <dt>To</dt><dd>{selectedWire.to?.terminal ?? `Grid (${selectedWire.end?.x}, ${selectedWire.end?.y})`}</dd>
              <dt>Route X</dt><dd>{selectedWire.routeX ?? "Automatic"}</dd>
            </dl>
            <p className="selection-note">
              Drag the wire to move its orthogonal routing channel.
              {selectedWire.to ? " Endpoints remain attached." : " Select the open endpoint, then a terminal, to finish it."}
            </p>
            <div className="selection-actions">
              <button className="danger" onClick={deleteSelectedWire}>Delete wire</button>
            </div>
          </>
        ) : selectedComponents.length > 1 ? (
          <>
            <h2>{selectedComponents.length} components</h2>
            <p className="selection-note">Shift-click devices to add or remove them from this selection.</p>
            <div className="selection-actions">
              <button onClick={() => transformSelection("rotate")}>Rotate</button>
              <button className="danger" onClick={() => transformSelection("delete")}>Delete</button>
            </div>
          </>
        ) : selected ? (
          <>
            <h2>{selected.name}</h2>
            <dl>
              <dt>Name</dt><dd><input className="property-input" key={selected.id} defaultValue={selected.name} onBlur={(event) => renameSelected(event.currentTarget.value)} /></dd>
              <dt>Type</dt><dd>{selected.kind}</dd>
              <dt>ID</dt><dd>{selected.id.slice(0, 8)}</dd>
              <dt>Position</dt><dd>{selected.position.x}, {selected.position.y}</dd>
            </dl>
            {deviceCharacteristics && (
              <>
                <p className="inspector-section">Device geometry</p>
                <dl>
                  <dt>Width</dt>
                  <dd>
                    <input
                      className="property-input numeric-property"
                      type="number"
                      min="0.001"
                      step="0.1"
                      key={`${selected.id}-w-${deviceCharacteristics.widthUm}`}
                      defaultValue={deviceCharacteristics.widthUm}
                      onBlur={(event) => updateDeviceGeometry(
                        event.currentTarget.valueAsNumber,
                        deviceCharacteristics.lengthUm,
                      )}
                    /> µm
                  </dd>
                  <dt>Length</dt>
                  <dd>
                    <input
                      className="property-input numeric-property"
                      type="number"
                      min="0.001"
                      step="0.1"
                      key={`${selected.id}-l-${deviceCharacteristics.lengthUm}`}
                      defaultValue={deviceCharacteristics.lengthUm}
                      onBlur={(event) => updateDeviceGeometry(
                        deviceCharacteristics.widthUm,
                        event.currentTarget.valueAsNumber,
                      )}
                    /> µm
                  </dd>
                </dl>
                <p className="inspector-section">Derived characteristics</p>
                <dl>
                  <dt>Effective Ron</dt>
                  <dd>{deviceCharacteristics.effectiveOnResistanceOhms.toLocaleString(undefined, { maximumFractionDigits: 2 })} Ω</dd>
                  <dt>Gate cap.</dt>
                  <dd>{deviceCharacteristics.gateCapacitanceFf.toLocaleString(undefined, { maximumFractionDigits: 3 })} fF</dd>
                  <dt>Diffusion cap.</dt>
                  <dd>{deviceCharacteristics.diffusionCapacitanceFf.toLocaleString(undefined, { maximumFractionDigits: 3 })} fF</dd>
                </dl>
                <p className="selection-note">Educational estimates from the active technology. Timing application begins in Milestone 3.5.</p>
              </>
            )}
            {selected.kind === "block" && (() => {
              const definition = project.blockDefinitions.find(({ id }) => id === selected.blockDefinitionId);
              return definition ? (
                <div className="block-inspector">
                  <p className="inspector-section">Shared definition</p>
                  <strong>{definition.name}</strong>
                  <small>{definition.components.length} source components · {definition.wires.length} wires</small>
                  <div className="block-pin-list">
                    {definition.pins.map((pin) => <span key={pin.name}><b>{pin.name}</b>{pin.role}</span>)}
                  </div>
                  <p className="selection-note">Instances share this transistor-level source. Analysis and physical generation flatten it temporarily.</p>
                  <button className="close-results" onClick={() => exportDefinition(definition.id, definition.name)}>Export portable block</button>
                </div>
              ) : <p className="selection-note">This instance references a missing definition.</p>;
            })()}
            <div className="selection-actions">
              <button onClick={() => transformSelection("rotate")}>Rotate</button>
              <button className="danger" onClick={() => transformSelection("delete")}>Delete</button>
            </div>
          </>
        ) : (
          <>
            <h2>Circuit</h2>
            <label className="property-label" htmlFor="circuit-name">Name</label>
            <input
              id="circuit-name"
              className="property-input circuit-name-input"
              key={project.name}
              defaultValue={project.name}
              onBlur={(event) => renameProject(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") event.currentTarget.blur();
              }}
            />
            <div className="technology-card">
              <p className="inspector-section">Active technology</p>
              <strong>{project.technology.name}</strong>
              <dl>
                <dt>Format</dt><dd>v{project.technology.format_version}</dd>
                <dt>Supply</dt><dd>{project.technology.supply_voltage} V</dd>
                <dt>Routing metals</dt><dd>{project.technology.max_metal_layers}</dd>
                <dt>Rule deck</dt><dd>v{project.technology.physical_rules.format_version}</dd>
                <dt>Grid</dt><dd>{project.technology.physical_rules.manufacturing_grid_um} µm</dd>
                <dt>Contact</dt><dd>{project.technology.physical_rules.contact.size_um} µm</dd>
                <dt>Via</dt><dd>{project.technology.physical_rules.via.size_um} µm</dd>
                <dt>Placement density</dt><dd>{Math.round(project.technology.physical_planning.target_device_density * 100)}%</dd>
                <dt>Route utilization</dt><dd>{Math.round(project.technology.physical_planning.target_routing_utilization * 100)}%</dd>
                <dt>NMOS Vt</dt><dd>{project.technology.nmos.threshold_voltage} V</dd>
                <dt>NMOS Ron</dt><dd>{project.technology.nmos.nominal_on_resistance_ohms.toLocaleString()} Ω</dd>
                <dt>PMOS Vt</dt><dd>{project.technology.pmos.threshold_voltage} V</dd>
                <dt>PMOS Ron</dt><dd>{project.technology.pmos.nominal_on_resistance_ohms.toLocaleString()} Ω</dd>
              </dl>
              <div className="selection-actions">
                <button onClick={loadTechnology}>Load YAML</button>
                <button onClick={resetTechnology}>Use built-in</button>
              </div>
            </div>
            <div className="empty compact"><span>⌁</span><p>Select an object in the viewport to inspect it.</p></div>
          </>
        )}
      </aside>
      {blockDialogOpen && (
        <div className="modal-backdrop" role="presentation" onPointerDown={() => {
          if (!blockSaving) setBlockDialogOpen(false);
        }}>
          <form className="block-dialog" role="dialog" aria-modal="true" aria-labelledby="block-dialog-title"
            onPointerDown={(event) => event.stopPropagation()}
            onSubmit={(event) => {
              event.preventDefault();
              void captureCurrentBlock();
            }}>
            <p className="eyebrow">Reusable Device Block</p>
            <h2 id="block-dialog-title">Save current circuit as a block</h2>
            <p>The current schematic will be copied into one shared definition. Inputs, outputs, VDD, and GND become instance pins.</p>
            <label htmlFor="block-name">Block name</label>
            <input id="block-name" className="property-input" autoFocus value={blockName}
              onChange={(event) => {
                setBlockName(event.currentTarget.value);
                setBlockDialogError(null);
              }} />
            {blockDialogError && <div className="block-dialog-error">{blockDialogError}</div>}
            <div className="block-dialog-actions">
              <button type="button" disabled={blockSaving} onClick={() => setBlockDialogOpen(false)}>Cancel</button>
              <button className="primary" type="submit" disabled={blockSaving}>
                {blockSaving ? "Saving…" : "Save Block"}
              </button>
            </div>
          </form>
        </div>
      )}
      <footer className={error ? "has-error" : ""}>
        <span className="ready-dot" />{error ?? status}
        <span className="footer-right">{workspace.dirty ? "Unsaved · " : ""}Format v{project.formatVersion}</span>
      </footer>
    </main>
  );
}
