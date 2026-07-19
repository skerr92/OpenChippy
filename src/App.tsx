import { useEffect, useMemo, useState } from "react";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import * as backend from "./backend";
import type { ComponentKind, LogicState, SimulationResult, TerminalRef, TruthTableResult, ValidationReport, WaveformConfig, WaveformResult, WorkspaceState } from "./types";
import SchematicViewport from "./SchematicViewport";
import Viewport from "./Viewport";
import WaveformView from "./WaveformView";

type ViewMode = "schematic" | "3d" | "waveform";

export default function App() {
  const [workspace, setWorkspace] = useState<WorkspaceState | null>(null);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [selectedWireId, setSelectedWireId] = useState<string | null>(null);
  const [status, setStatus] = useState("Starting workspace…");
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>("schematic");
  const [placementKind, setPlacementKind] = useState<ComponentKind | null>(null);
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
    backend.createProject()
      .then((value) => {
        setWorkspace(value);
        setStatus(backend.inTauri() ? "Desktop backend ready" : "Browser preview");
      })
      .catch(showError);
  }, []);

  useEffect(() => {
    const cancelPlacement = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setPlacementKind(null);
        setPendingTerminal(null);
        setPendingWireId(null);
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

  const replaceWorkspace = (next: WorkspaceState, message: string) => {
    setWorkspace(next);
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
    if (!placementKind) return;
    try {
      replaceWorkspace(await backend.addComponent(placementKind, x, y), `Placed ${placementKind.toUpperCase()} at (${x}, ${y})`);
      setPlacementKind(null);
    } catch (reason) {
      showError(reason);
    }
  };

  const armPlacement = (kind: ComponentKind) => {
    setViewMode("schematic");
    setPlacementKind(kind);
    setPendingTerminal(null);
    setPendingWireId(null);
    setStatus(`Click a grid point to place ${kind.toUpperCase()} · Esc to cancel`);
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
    try {
      replaceWorkspace(await backend.renameComponent(selected.id, name), `Renamed ${selected.name} to ${name.trim()}`);
      setSelectedIds([selected.id]);
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

  useEffect(() => {
    const editSelection = (event: KeyboardEvent) => {
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
      if (typeof path === "string") replaceWorkspace(await backend.loadProject(path), "Project loaded");
    } catch (reason) {
      showError(reason);
    }
  };

  const newProject = async () => {
    try {
      if (await confirmDiscard()) replaceWorkspace(await backend.createProject(), "New project");
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
    <main className="app-shell">
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
          <button className="primary" onClick={() => armPlacement("nmos")}>+ Place NMOS</button>
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
      </aside>
      <section className="stage">
        <div className="stage-label">
          <span>{workspace.dirty ? "● " : ""}{project.name}</span>
          <small>{workspace.path?.split(/[/\\]/).pop() ?? "Not saved"} · {project.components.length} components</small>
        </div>
        <div className="view-switch" role="group" aria-label="Editor view">
          <button className={viewMode === "schematic" ? "active" : ""} onClick={() => setViewMode("schematic")}>2D Schematic</button>
          <button className={viewMode === "3d" ? "active" : ""} onClick={() => { setViewMode("3d"); setPlacementKind(null); }}>3D View</button>
          <button className={viewMode === "waveform" ? "active" : ""} onClick={() => { setViewMode("waveform"); setPlacementKind(null); }}>Waveforms</button>
        </div>
        {viewMode === "schematic" ? (
          <SchematicViewport
            components={project.components}
            wires={project.wires}
            selectedIds={selectedIds}
            selectedWireId={selectedWireId}
            placementKind={placementKind}
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
          <Viewport components={project.components} wires={project.wires} selectedIds={selectedIds} simulation={simulation} onSelect={selectComponent} />
        ) : (
          <WaveformView config={waveformConfig} result={waveform} running={waveformRunning}
            onConfig={setWaveformConfig} onRun={runWaveform} />
        )}
        {viewMode !== "waveform" && (digitalInputs.length > 0 || simulation) && (
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
        {viewMode !== "waveform" && (placementKind || pendingTerminal || pendingWireId) && <div className="placement-hint">
          {placementKind
            ? `Place ${placementKind.toUpperCase()} on grid`
            : pendingWireId
              ? "Select a terminal to finish this wire"
              : "Select a terminal or click the grid"}
          <button onClick={() => { setPlacementKind(null); setPendingTerminal(null); setPendingWireId(null); }}>Cancel</button>
        </div>}
        {viewMode === "schematic" && !placementKind && !pendingTerminal && !pendingWireId && (
          <div className="viewport-help">Drag empty canvas / two-finger / arrows to pan · Pinch or Ctrl-wheel to zoom · Click a terminal, then the grid, to leave a routed endpoint</div>
        )}
        {viewMode === "3d" && (
          <div className="viewport-help">Drag to pan · Right-drag to rotate · Arrows pan · Wheel or pinch to zoom · Click layers to select</div>
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
                  <span>{output.name}</span>
                  <strong className={`logic-${output.state.toLowerCase()}`}>{output.state}</strong>
                </button>
              ))}
              {!simulation.outputs.length && <p className="selection-note">No output probes are placed.</p>}
            </div>
            <p className="inspector-section">Transistors</p>
            <div className="state-list transistor-states">
              {simulation.transistors.map((transistor) => (
                <button key={transistor.componentId} onClick={() => selectComponent(transistor.componentId)}>
                  <span>{transistor.name}</span>
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
            <div className="empty compact"><span>⌁</span><p>Select an object in the viewport to inspect it.</p></div>
          </>
        )}
      </aside>
      <footer className={error ? "has-error" : ""}>
        <span className="ready-dot" />{error ?? status}
        <span className="footer-right">{workspace.dirty ? "Unsaved · " : ""}Format v{project.formatVersion}</span>
      </footer>
    </main>
  );
}
