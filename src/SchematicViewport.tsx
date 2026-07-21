import { useEffect, useRef, useState } from "react";
import type { BlockDefinition, Component, ComponentKind, LogicState, SimulationResult, TerminalRef, Wire } from "./types";
import { isEditingText } from "./dom";

type Props = {
  fitRevision: number;
  components: Component[];
  wires: Wire[];
  blockDefinitions: BlockDefinition[];
  selectedIds: string[];
  selectedWireId: string | null;
  placementKind: ComponentKind | null;
  placementActive: boolean;
  pendingTerminal: TerminalRef | null;
  routingActive: boolean;
  simulation: SimulationResult | null;
  inputStates: Record<string, LogicState>;
  onSelect: (id: string | null, additive?: boolean) => void;
  onPlace: (x: number, y: number) => void;
  onMove: (id: string, x: number, y: number) => void;
  onWireSelect: (id: string | null) => void;
  onWireMove: (id: string, routeX: number) => void;
  onTerminal: (terminal: TerminalRef) => void;
  onFreePoint: (x: number, y: number) => void;
  onDanglingEnd: (wireId: string) => void;
};

const VIEW_BOX = { x: -12, y: -8, width: 24, height: 16 };
const snap = (value: number) => Math.round(value);

export const terminalOffset = (kind: string, terminal: string): [number, number] => {
  if (kind === "nmos" || kind === "pmos") {
    if (terminal === "gate") return [-1.5, 0];
    return terminal === "drain" ? [0, -1.4] : [0, 1.4];
  }
  if (kind === "vdd") return [0, 1.3];
  if (kind === "gnd") return [0, -1.3];
  if (kind === "input") return [1.5, 0];
  if (kind === "output" || kind === "net_label") return [-1.5, 0];
  if (kind === "junction") return [0, 0];
  return terminal === "a" ? [-2.4, 0] : [2.4, 0];
};

export const terminalPosition = (component: Component, terminal: string): [number, number] => {
  const [dx, dy] = terminalOffset(component.kind, terminal);
  const cosine = Math.cos(component.rotation);
  const sine = Math.sin(component.rotation);
  return [
    component.position.x + dx * cosine - dy * sine,
    component.position.y + dx * sine + dy * cosine,
  ];
};

const terminalsFor = (kind: string) =>
  kind === "nmos" || kind === "pmos"
    ? ["gate", "drain", "source"]
    : kind === "resistor" ? ["a", "b"] : kind === "junction" ? ["node"] : [kind === "output" ? "in" : kind === "net_label" ? "node" : "out"];

const blockFor = (component: Component, definitions: BlockDefinition[]) =>
  definitions.find((definition) => definition.id === component.blockDefinitionId);

const componentTerminals = (component: Component, definitions: BlockDefinition[]) =>
  component.kind === "block"
    ? blockFor(component, definitions)?.pins.map((pin) => pin.name) ?? []
    : terminalsFor(component.kind);

const componentTerminalOffset = (
  component: Component,
  terminal: string,
  definitions: BlockDefinition[],
): [number, number] => {
  if (component.kind !== "block") return terminalOffset(component.kind, terminal);
  const pins = blockFor(component, definitions)?.pins ?? [];
  const pin = pins.find((candidate) => candidate.name === terminal);
  if (!pin) return [-1.8, 0];
  const rolePins = pins.filter((candidate) => candidate.role === pin.role);
  const index = Math.max(0, rolePins.findIndex((candidate) => candidate.name === terminal));
  const offset = (index - (rolePins.length - 1) / 2) * .7;
  if (pin.role === "input") return [-1.8, offset];
  if (pin.role === "output") return [1.8, offset];
  if (pin.role === "power") return [offset, -1.45];
  return [offset, 1.45];
};

function DeviceShape({ component, definition }: { component: Component; definition?: BlockDefinition }) {
  if (component.kind === "nmos" || component.kind === "pmos") {
    return (
      <>
        <path className="symbol-body" d="M -.55 -1 V 1 M .15 -.8 V .8 M .15 -.65 H .8 V -1.4 M .15 .65 H .8 V 1.4 M -1.5 0 H -.55" />
        {component.kind === "pmos" && <circle className="symbol-bubble" cx="-.38" cy="0" r=".17" />}
      </>
    );
  }
  if (component.kind === "vdd") return <path className="symbol-body" d="M 0 1.3 V -.45 M -.5 .15 L 0 -.55 L .5 .15" />;
  if (component.kind === "gnd") return <path className="symbol-body" d="M 0 -1.3 V 0 M -.7 0 H .7 M -.45 .35 H .45 M -.2 .7 H .2" />;
  if (component.kind === "input") return <path className="symbol-body" d="M -1.1 -.7 H .5 L 1.5 0 L .5 .7 H -1.1 Z" />;
  if (component.kind === "output") return <path className="symbol-body" d="M -1.5 0 H -.65 M -.65 -.7 H .7 L 1.25 0 L .7 .7 H -.65 Z M .15 -.35 L .65 0 L .15 .35" />;
  if (component.kind === "net_label") return <path className="symbol-body" d="M -1.5 0 H -.8 L -.45 -.5 H 1.2 V .5 H -.45 L -.8 0" />;
  if (component.kind === "junction") return <circle className="junction-dot" cx="0" cy="0" r=".28" />;
  if (component.kind === "block") return (
    <>
      <rect className="symbol-body block-body" x="-1.45" y="-1.15" width="2.9" height="2.3" rx=".18" />
      <text className="block-kind-label" x="0" y=".15" textAnchor="middle">{definition?.name ?? "Missing"}</text>
    </>
  );
  return <path className="symbol-body" d="M -2.4 0 H -1.5 L -1.15 -.55 L -.65 .55 L -.15 -.55 L .35 .55 L .85 -.55 L 1.5 0 H 2.4" />;
}

function Symbol({ component, definition, blockDefinitions, selected, pendingTerminal, logicState, switchState, onSelect, onDragStart, onTerminal }: {
  component: Component;
  definition?: BlockDefinition;
  blockDefinitions: BlockDefinition[];
  selected: boolean;
  pendingTerminal: TerminalRef | null;
  logicState?: LogicState;
  switchState?: "on" | "off" | "unknown";
  onSelect: () => void;
  onDragStart: (event: React.PointerEvent) => void;
  onTerminal: (name: string) => void;
}) {
  return (
    <g className={`schematic-component device-${component.kind} ${selected ? "selected" : ""} ${logicState ? `logic-${logicState.toLowerCase()}` : ""} ${switchState ? `switch-${switchState}` : ""}`}
      transform={`translate(${component.position.x} ${component.position.y}) rotate(${component.rotation * 180 / Math.PI})`}
      onPointerDown={onDragStart}>
      <rect className="symbol-hitbox" x="-1.8" y="-1.55" width="3.6" height="3.1" rx=".18" />
      <DeviceShape component={component} definition={definition} />
      {componentTerminals(component, blockDefinitions).map((terminal) => {
        const [x, y] = componentTerminalOffset(component, terminal, blockDefinitions);
        const pending = pendingTerminal?.componentId === component.id && pendingTerminal.terminal === terminal;
        const pin = definition?.pins.find((candidate) => candidate.name === terminal);
        const label = pin?.role === "input"
          ? { x: x - .28, y: y + .11, anchor: "end" as const }
          : pin?.role === "output"
            ? { x: x + .28, y: y + .11, anchor: "start" as const }
            : pin?.role === "power"
              ? { x, y: y - .28, anchor: "middle" as const }
              : { x, y: y + .38, anchor: "middle" as const };
        return <g key={terminal}>
          <circle className={`terminal ${pending ? "pending" : ""}`} cx={x} cy={y} r=".18"
            onPointerDown={(event) => { event.stopPropagation(); onTerminal(terminal); }} />
          {component.kind === "block" && <text className={`block-pin-label pin-${pin?.role ?? "unknown"}`}
            x={label.x} y={label.y} textAnchor={label.anchor}>{terminal}</text>}
        </g>;
      })}
      <text x="0" y={component.kind === "block" ? "-2.08" : "-1.8"} textAnchor="middle">{component.name}</text>
      {logicState && <text className="simulation-state-label" x="0" y="2.05" textAnchor="middle">{logicState}</text>}
      {switchState && <text className="simulation-state-label" x="0" y="2.05" textAnchor="middle">{switchState.toUpperCase()}</text>}
    </g>
  );
}

export default function SchematicViewport(props: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const drag = useRef<{ id: string; moved: boolean } | null>(null);
  const wireDrag = useRef<{ id: string; moved: boolean } | null>(null);
  const pan = useRef<{ x: number; y: number; viewX: number; viewY: number } | null>(null);
  const [viewBox, setViewBox] = useState(VIEW_BOX);
  const toPoint = (event: React.PointerEvent) => {
    const svg = svgRef.current!;
    const point = svg.createSVGPoint();
    point.x = event.clientX; point.y = event.clientY;
    return point.matrixTransform(svg.getScreenCTM()!.inverse());
  };
  const componentById = new Map(props.components.map((component) => [component.id, component]));
  const wireStates = new Map(props.simulation?.wires.map((wire) => [wire.wireId, wire.state]) ?? []);
  const outputStates = new Map(props.simulation?.outputs.map((output) => [output.name, output.state]) ?? []);
  const transistorStates = new Map(props.simulation?.transistors.map((transistor) => [transistor.componentId, transistor.state]) ?? []);
  const wirePoint = (terminal: TerminalRef) => {
    const component = componentById.get(terminal.componentId);
    if (!component) return [0, 0] as const;
    const [dx, dy] = componentTerminalOffset(component, terminal.terminal, props.blockDefinitions);
    const cosine = Math.cos(component.rotation);
    const sine = Math.sin(component.rotation);
    return [
      component.position.x + dx * cosine - dy * sine,
      component.position.y + dx * sine + dy * cosine,
    ] as const;
  };
  const fitToDesign = () => {
    const points = [
      ...props.components.map((component) => component.position),
      ...props.wires.flatMap((wire) => [
        ...(wire.end ? [wire.end] : []),
        ...(wire.waypoints ?? []),
      ]),
    ];
    if (!points.length) {
      setViewBox(VIEW_BOX);
      return;
    }
    const minX = Math.min(...points.map((point) => point.x));
    const maxX = Math.max(...points.map((point) => point.x));
    const minY = Math.min(...points.map((point) => point.y));
    const maxY = Math.max(...points.map((point) => point.y));
    const contentWidth = Math.max(maxX - minX + 8, 12);
    const contentHeight = Math.max(maxY - minY + 8, 8);
    const width = Math.max(
      contentWidth,
      contentHeight * (VIEW_BOX.width / VIEW_BOX.height),
    );
    const height = width * (VIEW_BOX.height / VIEW_BOX.width);
    setViewBox({
      x: (minX + maxX - width) / 2,
      y: (minY + maxY - height) / 2,
      width,
      height,
    });
  };

  useEffect(() => {
    fitToDesign();
    // A revision is issued only for project create/open, not ordinary edits.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.fitRevision]);

  useEffect(() => {
    const panWithArrows = (event: KeyboardEvent) => {
      if (isEditingText(event)) return;
      if (event.key === "Home" || event.key.toLowerCase() === "f") {
        event.preventDefault();
        fitToDesign();
        return;
      }
      const direction: Record<string, [number, number]> = {
        ArrowLeft: [-1, 0],
        ArrowRight: [1, 0],
        ArrowUp: [0, -1],
        ArrowDown: [0, 1],
      };
      const delta = direction[event.key];
      if (!delta) return;
      event.preventDefault();
      setViewBox((current) => ({
        ...current,
        x: current.x + delta[0] * current.width * (event.shiftKey ? .18 : .06),
        y: current.y + delta[1] * current.height * (event.shiftKey ? .18 : .06),
      }));
    };
    window.addEventListener("keydown", panWithArrows);
    return () => window.removeEventListener("keydown", panWithArrows);
  }, [props.components, props.wires]);

  return (
    <svg ref={svgRef} className={`schematic-viewport ${props.placementActive ? "placing" : ""}`}
      viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}`} preserveAspectRatio="xMidYMid slice" aria-label="2D CMOS schematic editor"
      onWheel={(event) => {
        event.preventDefault();
        if (event.ctrlKey || event.metaKey) {
          const point = toPoint(event as unknown as React.PointerEvent);
          const factor = event.deltaY > 0 ? 1.08 : .92;
          const width = Math.min(4096, Math.max(2, viewBox.width * factor));
          const height = width * (VIEW_BOX.height / VIEW_BOX.width);
          const ratioX = (point.x - viewBox.x) / viewBox.width;
          const ratioY = (point.y - viewBox.y) / viewBox.height;
          setViewBox({ x: point.x - ratioX * width, y: point.y - ratioY * height, width, height });
        } else {
          setViewBox((current) => ({
            ...current,
            x: current.x + event.deltaX * current.width / 900,
            y: current.y + event.deltaY * current.height / 700,
          }));
        }
      }}
      onPointerDown={(event) => {
        if (props.routingActive && event.button === 0) {
          const point = toPoint(event);
          props.onFreePoint(snap(point.x), snap(point.y));
          return;
        }
        if (!props.placementActive && (event.button === 0 || event.button === 1)) {
          event.preventDefault();
          pan.current = { x: event.clientX, y: event.clientY, viewX: viewBox.x, viewY: viewBox.y };
          props.onSelect(null);
          props.onWireSelect(null);
          event.currentTarget.setPointerCapture(event.pointerId);
          return;
        }
        if (props.placementActive) {
          const point = toPoint(event);
          props.onPlace(snap(point.x), snap(point.y));
        } else props.onSelect(null);
      }}
      onPointerMove={(event) => {
        if (pan.current) {
          const bounds = event.currentTarget.getBoundingClientRect();
          setViewBox((current) => ({
            ...current,
            x: pan.current!.viewX - (event.clientX - pan.current!.x) * current.width / bounds.width,
            y: pan.current!.viewY - (event.clientY - pan.current!.y) * current.height / bounds.height,
          }));
          return;
        }
        if (!(event.buttons & 1)) return;
        if (wireDrag.current) {
          wireDrag.current.moved = true;
          return;
        }
        if (drag.current) drag.current.moved = true;
      }}
      onPointerUp={(event) => {
        if (pan.current) { pan.current = null; return; }
        if (wireDrag.current) {
          const current = wireDrag.current; wireDrag.current = null;
          if (current.moved) props.onWireMove(current.id, snap(toPoint(event).x));
          return;
        }
        if (!drag.current) return;
        const current = drag.current; drag.current = null;
        if (current.moved) {
          const point = toPoint(event);
          props.onMove(current.id, snap(point.x), snap(point.y));
        }
      }}>
      <defs>
        <pattern id="minor-grid" width=".25" height=".25" patternUnits="userSpaceOnUse"><circle r=".018" className="minor-dot" /></pattern>
        <pattern id="major-grid" width="1" height="1" patternUnits="userSpaceOnUse"><rect width="1" height="1" fill="url(#minor-grid)" /><circle r=".035" className="major-dot" /></pattern>
      </defs>
      <rect x={viewBox.x} y={viewBox.y} width={viewBox.width} height={viewBox.height} className="schematic-background" />
      <rect x={viewBox.x} y={viewBox.y} width={viewBox.width} height={viewBox.height} fill="url(#major-grid)" />
      <g className="wires">
        {props.wires.map((wire) => {
          const [x1, y1] = wirePoint(wire.from);
          const [x2, y2] = wire.to
            ? wirePoint(wire.to)
            : [wire.end?.x ?? x1, wire.end?.y ?? y1];
          const waypoints = wire.waypoints ?? [];
          const path = waypoints.length
            ? `M ${x1} ${y1} ${waypoints.map((point) => `H ${point.x} V ${point.y}`).join(" ")}${wire.to ? ` H ${x2} V ${y2}` : ""}`
            : `M ${x1} ${y1} H ${wire.routeX ?? (x1 + x2) / 2} V ${y2} H ${x2}`;
          const logicState = wireStates.get(wire.id);
          return <g key={wire.id} className={`${wire.id === props.selectedWireId ? "selected" : ""} ${logicState ? `logic-${logicState.toLowerCase()}` : ""}`}>
            <path className="wire-hitbox" d={path} onPointerDown={(event) => {
              event.stopPropagation();
              props.onSelect(null);
              props.onWireSelect(wire.id);
              if (!waypoints.length) {
                wireDrag.current = { id: wire.id, moved: false };
                event.currentTarget.setPointerCapture(event.pointerId);
              }
            }} />
            <path className="wire-line" d={path} />
            {!wire.to && <circle className="dangling-end" cx={x2} cy={y2} r=".24"
              onPointerDown={(event) => {
                event.stopPropagation();
                props.onDanglingEnd(wire.id);
              }} />}
          </g>;
        })}
      </g>
      {props.components.map((component) => <Symbol key={component.id} component={component}
        definition={blockFor(component, props.blockDefinitions)}
        blockDefinitions={props.blockDefinitions}
        selected={props.selectedIds.includes(component.id)} pendingTerminal={props.pendingTerminal}
        logicState={component.kind === "input" ? props.inputStates[component.name] : component.kind === "output" ? outputStates.get(component.name) : undefined}
        switchState={transistorStates.get(component.id)}
        onSelect={() => props.onSelect(component.id)}
        onDragStart={(event) => {
          event.stopPropagation(); props.onSelect(component.id, event.shiftKey);
          drag.current = { id: component.id, moved: false };
          event.currentTarget.setPointerCapture(event.pointerId);
        }}
        onTerminal={(terminal) => props.onTerminal({ componentId: component.id, terminal })} />)}
    </svg>
  );
}
