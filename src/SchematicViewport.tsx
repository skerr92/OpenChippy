import { useEffect, useRef, useState } from "react";
import type { Component, ComponentKind, TerminalRef, Wire } from "./types";

type Props = {
  components: Component[];
  wires: Wire[];
  selectedIds: string[];
  selectedWireId: string | null;
  placementKind: ComponentKind | null;
  pendingTerminal: TerminalRef | null;
  routingActive: boolean;
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

function DeviceShape({ component }: { component: Component }) {
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
  return <path className="symbol-body" d="M -2.4 0 H -1.5 L -1.15 -.55 L -.65 .55 L -.15 -.55 L .35 .55 L .85 -.55 L 1.5 0 H 2.4" />;
}

function Symbol({ component, selected, pendingTerminal, onSelect, onDragStart, onTerminal }: {
  component: Component;
  selected: boolean;
  pendingTerminal: TerminalRef | null;
  onSelect: () => void;
  onDragStart: (event: React.PointerEvent) => void;
  onTerminal: (name: string) => void;
}) {
  return (
    <g className={`schematic-component device-${component.kind} ${selected ? "selected" : ""}`}
      transform={`translate(${component.position.x} ${component.position.y}) rotate(${component.rotation * 180 / Math.PI})`}
      onPointerDown={onDragStart}>
      <rect className="symbol-hitbox" x="-1.8" y="-1.55" width="3.6" height="3.1" rx=".18" />
      <DeviceShape component={component} />
      {terminalsFor(component.kind).map((terminal) => {
        const [x, y] = terminalOffset(component.kind, terminal);
        const pending = pendingTerminal?.componentId === component.id && pendingTerminal.terminal === terminal;
        return <circle key={terminal} className={`terminal ${pending ? "pending" : ""}`} cx={x} cy={y} r=".18"
          onPointerDown={(event) => { event.stopPropagation(); onTerminal(terminal); }} />;
      })}
      <text x="0" y="-1.8" textAnchor="middle">{component.name}</text>
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
  const wirePoint = (terminal: TerminalRef) => {
    const component = componentById.get(terminal.componentId);
    if (!component) return [0, 0] as const;
    return terminalPosition(component, terminal.terminal);
  };

  useEffect(() => {
    const panWithArrows = (event: KeyboardEvent) => {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
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
        x: current.x + delta[0] * current.width * .06,
        y: current.y + delta[1] * current.height * .06,
      }));
    };
    window.addEventListener("keydown", panWithArrows);
    return () => window.removeEventListener("keydown", panWithArrows);
  }, []);

  return (
    <svg ref={svgRef} className={`schematic-viewport ${props.placementKind ? "placing" : ""}`}
      viewBox={`${viewBox.x} ${viewBox.y} ${viewBox.width} ${viewBox.height}`} preserveAspectRatio="xMidYMid slice" aria-label="2D CMOS schematic editor"
      onWheel={(event) => {
        event.preventDefault();
        if (event.ctrlKey || event.metaKey) {
          const point = toPoint(event as unknown as React.PointerEvent);
          const factor = event.deltaY > 0 ? 1.08 : .92;
          const width = Math.min(60, Math.max(8, viewBox.width * factor));
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
        if (!props.placementKind && (event.button === 0 || event.button === 1)) {
          event.preventDefault();
          pan.current = { x: event.clientX, y: event.clientY, viewX: viewBox.x, viewY: viewBox.y };
          props.onSelect(null);
          props.onWireSelect(null);
          event.currentTarget.setPointerCapture(event.pointerId);
          return;
        }
        if (props.placementKind) {
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
          return <g key={wire.id} className={wire.id === props.selectedWireId ? "selected" : ""}>
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
        selected={props.selectedIds.includes(component.id)} pendingTerminal={props.pendingTerminal}
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
