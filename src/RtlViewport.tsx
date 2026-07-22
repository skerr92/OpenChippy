import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { RtlDesign } from "./types";

type Props = {
  design: RtlDesign;
};

type Camera = { x: number; y: number; scale: number };

export default function RtlViewport({ design }: Props) {
  const [camera, setCamera] = useState<Camera>({ x: 0, y: 0, scale: 1 });
  const [selected, setSelected] = useState<string | null>(null);
  const canvas = useRef<HTMLDivElement>(null);
  const drag = useRef<{ x: number; y: number; camera: Camera } | null>(null);
  const module = design.module;
  const placement = useMemo(
    () => new Map(design.placements.map((item) => [item.instanceName, item])),
    [design.placements],
  );
  const maxLevel = Math.max(0, ...design.placements.map(({ level }) => level));
  const assignmentX = 8 + (maxLevel + 1) * 8;
  const logicalNodeCount = module.assignments.length + module.sequentialProcesses.length;
  const outputX = assignmentX + (logicalNodeCount ? 10 : 2);
  const edges = useMemo(() => {
    const sources = new Map<string, { x: number; y: number }>();
    module.ports.filter(({ direction }) => direction === "input").forEach((port, index) =>
      sources.set(port.name, { x: 5, y: index * 4 }));
    module.instances.forEach((instance) => {
      const point = placement.get(instance.name);
      const output = instance.connections[0];
      if (point && output) sources.set(output, { x: point.x + 2.6, y: point.y });
    });
    module.assignments.forEach((assignment, index) =>
      sources.set(assignment.target, { x: assignmentX + 3.2, y: 6 + index * 5 }));
    module.sequentialProcesses.forEach((process, index) =>
      sources.set(process.target, { x: assignmentX + 3.2, y: 6 + (module.assignments.length + index) * 5 }));
    const result: Array<{ key: string; from: { x: number; y: number }; to: { x: number; y: number }; net: string }> = [];
    module.instances.forEach((instance) => {
      const point = placement.get(instance.name);
      if (!point) return;
      instance.connections.slice(1).forEach((net, index) => {
        const from = sources.get(net);
        if (from) result.push({ key: `${instance.name}:${index}`, from, to: { x: point.x - 2.6, y: point.y + (index - (instance.connections.length - 2) / 2) * .6 }, net });
      });
    });
    module.assignments.forEach((assignment, index) => assignment.referencedSignals.forEach((net, refIndex) => {
      const from = sources.get(net);
      if (from) result.push({ key: `assign:${index}:${refIndex}`, from, to: { x: assignmentX - 3.2, y: 6 + index * 5 + refIndex * .45 }, net });
    }));
    module.sequentialProcesses.forEach((process, index) => {
      const y = 6 + (module.assignments.length + index) * 5;
      [process.clock, ...process.referencedSignals].forEach((net, refIndex) => {
        const from = sources.get(net);
        if (from) result.push({ key: `process:${index}:${refIndex}`, from, to: { x: assignmentX - 3.2, y: y + refIndex * .45 }, net });
      });
    });
    module.ports.filter(({ direction }) => direction === "output").forEach((port, index) => {
      const from = sources.get(port.name);
      if (from) result.push({ key: `output:${port.name}`, from, to: { x: outputX, y: index * 4 }, net: port.name });
    });
    return result;
  }, [assignmentX, module, outputX, placement]);
  const selectedInstance = module.instances.find(({ name }) => name === selected) ?? null;
  const selectedAssignment = module.assignments.find((_, index) => `assign:${index}` === selected) ?? null;
  const selectedProcess = module.sequentialProcesses.find((_, index) => `process:${index}` === selected) ?? null;

  const designBounds = useMemo(() => {
    const inputCount = module.ports.filter(({ direction }) => direction === "input").length;
    const outputCount = module.ports.filter(({ direction }) => direction === "output").length;
    const placementYs = design.placements.map(({ y }) => y);
    const logicalBottom = logicalNodeCount ? 6 + (logicalNodeCount - 1) * 5 + 1.5 : 0;
    return {
      minX: -1,
      minY: -2,
      maxX: outputX + 6,
      maxY: Math.max(
        (inputCount - 1) * 4 + 2,
        (outputCount - 1) * 4 + 2,
        ...placementYs.map((y) => y + 2),
        logicalBottom,
        2,
      ),
    };
  }, [design.placements, logicalNodeCount, module.ports, outputX]);

  const fit = useCallback(() => {
    if (!canvas.current) return;
    const width = Math.max(designBounds.maxX - designBounds.minX, 1);
    const height = Math.max(designBounds.maxY - designBounds.minY, 1);
    const scale = Math.max(1, Math.min(30,
      Math.min(
        Math.max(canvas.current.clientWidth - 72, 1) / width,
        Math.max(canvas.current.clientHeight - 72, 1) / height,
      ),
    ));
    const centerX = (designBounds.minX + designBounds.maxX) / 2;
    const centerY = (designBounds.minY + designBounds.maxY) / 2;
    setCamera({
      x: canvas.current.clientWidth / 2 - centerX * scale,
      y: canvas.current.clientHeight / 2 - centerY * scale,
      scale,
    });
  }, [designBounds]);

  useEffect(() => {
    const frame = requestAnimationFrame(fit);
    if (!canvas.current) return () => cancelAnimationFrame(frame);
    const observer = new ResizeObserver(fit);
    observer.observe(canvas.current);
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [fit]);

  return <div className="rtl-workspace">
    <aside className="rtl-view-sidebar">
      <p className="eyebrow">Logical RTL</p>
      <h2>{module.name}</h2>
      <small>{module.ports.length} ports · {module.instances.length} instances · {module.assignments.length} assignments · {module.sequentialProcesses.length} registers</small>
      <button type="button" onClick={fit}>Fit design</button>
      <p className="rtl-view-section">Selection</p>
      {selectedInstance ? <div className="rtl-selection-card">
        <strong>{selectedInstance.name}</strong>
        <span>{selectedInstance.cell}</span>
        <small>{selectedInstance.connections.join(" · ")}</small>
        {selectedInstance.parameterOverrides.map((parameter, index) =>
          <em key={index}>{parameter.name ? `.${parameter.name}` : `#${index}` } = {parameter.expression}</em>)}
      </div> : selectedAssignment ? <div className="rtl-selection-card">
        <strong>{selectedAssignment.target}</strong>
        <span>continuous assignment</span>
        <small>{selectedAssignment.expression}</small>
        <em>{selectedAssignment.referencedSignals.join(" · ")}</em>
      </div> : selectedProcess ? <div className="rtl-selection-card">
        <strong>{selectedProcess.target}</strong>
        <span>{selectedProcess.edge} register</span>
        <small>{selectedProcess.expression}</small>
        <em>{selectedProcess.clock} · {selectedProcess.referencedSignals.join(" · ")}</em>
      </div> : <p className="rtl-view-empty">Select a gate, cell, assignment, or register to inspect its source identity.</p>}
      <p className="rtl-view-section">Hierarchy</p>
      <div className="rtl-hierarchy-root"><strong>{module.name}</strong><small>top module</small></div>
      {module.instances.filter(({ primitive }) => primitive === null).map((instance) =>
        <button key={instance.name} className="rtl-hierarchy-item" onClick={() => setSelected(instance.name)}>
          <span>{instance.name}</span><small>{instance.cell} · external definition</small>
        </button>)}
    </aside>
    <div className="rtl-canvas" ref={canvas}
      onWheel={(event) => {
        event.preventDefault();
        setCamera((current) => ({ ...current, scale: Math.max(1, Math.min(30, current.scale * Math.exp(-event.deltaY * .001))) }));
      }}
      onPointerDown={(event) => {
        if (event.target === event.currentTarget || (event.target as Element).classList.contains("rtl-background")) {
          drag.current = { x: event.clientX, y: event.clientY, camera };
          event.currentTarget.setPointerCapture(event.pointerId);
          setSelected(null);
        }
      }}
      onPointerMove={(event) => {
        if (!drag.current) return;
        setCamera({ ...drag.current.camera, x: drag.current.camera.x + event.clientX - drag.current.x, y: drag.current.camera.y + event.clientY - drag.current.y });
      }}
      onPointerUp={() => { drag.current = null; }}>
      <svg width="100%" height="100%" role="img" aria-label={`Logical RTL view for ${module.name}`}>
        <rect className="rtl-background" width="100%" height="100%" />
        <g transform={`translate(${camera.x} ${camera.y}) scale(${camera.scale})`}>
          <g className="rtl-edges">{edges.map((edge) => {
            const middle = (edge.from.x + edge.to.x) / 2;
            return <path key={edge.key} d={`M${edge.from.x},${edge.from.y} H${middle} V${edge.to.y} H${edge.to.x}`}><title>{edge.net}</title></path>;
          })}</g>
          {module.ports.filter(({ direction }) => direction === "input").map((port, index) =>
            <g key={port.name} className="rtl-port" transform={`translate(0 ${index * 4})`}>
              <path d="M0,-1.2 H4 L5,0 L4,1.2 H0 Z" /><text x=".5" y=".35">{port.name}</text>
            </g>)}
          {module.instances.map((instance) => {
            const point = placement.get(instance.name) ?? { x: 8, y: 6, level: 0 };
            return <g key={instance.name} className={`rtl-node ${selected === instance.name ? "selected" : ""}`}
              transform={`translate(${point.x} ${point.y})`} onClick={(event) => { event.stopPropagation(); setSelected(instance.name); }}>
              <rect x="-2.6" y="-1.5" width="5.2" height="3" rx=".4" />
              <text className="rtl-node-kind" textAnchor="middle" y="-.2">{instance.cell}</text>
              <text className="rtl-node-name" textAnchor="middle" y=".75">{instance.name}</text>
            </g>;
          })}
          {module.assignments.map((assignment, index) => <g key={index}
            className={`rtl-node rtl-assignment ${selected === `assign:${index}` ? "selected" : ""}`}
            transform={`translate(${assignmentX} ${6 + index * 5})`}
            onClick={(event) => { event.stopPropagation(); setSelected(`assign:${index}`); }}>
            <rect x="-3.2" y="-1.5" width="6.4" height="3" rx=".4" />
            <text className="rtl-node-kind" textAnchor="middle" y="-.2">assign</text>
            <text className="rtl-node-name" textAnchor="middle" y=".75">{assignment.target} = {assignment.expression}</text>
          </g>)}
          {module.sequentialProcesses.map((process, index) => <g key={`process-${index}`}
            className={`rtl-node rtl-register ${selected === `process:${index}` ? "selected" : ""}`}
            transform={`translate(${assignmentX} ${6 + (module.assignments.length + index) * 5})`}
            onClick={(event) => { event.stopPropagation(); setSelected(`process:${index}`); }}>
            <rect x="-3.2" y="-1.5" width="6.4" height="3" rx=".4" />
            <text className="rtl-node-kind" textAnchor="middle" y="-.2">{process.edge} register</text>
            <text className="rtl-node-name" textAnchor="middle" y=".75">{process.target} ≤ {process.expression}</text>
          </g>)}
          {module.ports.filter(({ direction }) => direction === "output").map((port, index) =>
            <g key={port.name} className="rtl-port output" transform={`translate(${outputX} ${index * 4})`}>
              <path d="M0,-1.2 H4 L5,0 L4,1.2 H0 Z" /><text x=".5" y=".35">{port.name}</text>
            </g>)}
        </g>
      </svg>
      <div className="rtl-canvas-help">Drag to pan · Wheel or pinch to zoom · Fit design restores the complete module</div>
    </div>
  </div>;
}
