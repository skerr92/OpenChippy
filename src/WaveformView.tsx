import { useEffect, useMemo, useRef, useState } from "react";
import type { LogicState, WaveformConfig, WaveformGroup, WaveformResult } from "./types";

type Signal = WaveformResult["signals"][number];
type ScalarLane = { type: "scalar"; signal: Signal; member: boolean };
type BusLane = { type: "bus"; group: WaveformGroup; signals: Signal[] };
type Lane = ScalarLane | BusLane;

type Props = {
  config: WaveformConfig;
  result: WaveformResult | null;
  running: boolean;
  groups: WaveformGroup[];
  onConfig: (config: WaveformConfig) => void;
  onRun: () => void;
  onGroups: (groups: WaveformGroup[]) => void | Promise<void>;
};

const stateY = (state: LogicState, top: number) => {
  if (state === "HIGH") return top + 11;
  if (state === "LOW") return top + 39;
  return top + 25;
};

const valueAt = (samples: Array<{ timeNs: number; state: LogicState }>, timeNs: number) =>
  samples.reduce(
    (current, sample) => (sample.timeNs <= timeNs ? sample.state : current),
    samples[0]?.state ?? "UNKNOWN",
  );

const binaryValue = (state: LogicState) =>
  state === "HIGH" ? "1" : state === "LOW" ? "0" : "X";

const formatBus = (signals: Signal[], timeNs: number, radix: WaveformGroup["radix"]) => {
  const bits = signals.map((signal) => binaryValue(valueAt(signal.samples, timeNs))).join("");
  if (radix === "binary") return `0b${bits}`;
  const padded = bits.padStart(Math.ceil(bits.length / 4) * 4, "0");
  const digits = Array.from({ length: padded.length / 4 }, (_, index) => {
    const nibble = padded.slice(index * 4, index * 4 + 4);
    return nibble.includes("X") ? "X" : Number.parseInt(nibble, 2).toString(16).toUpperCase();
  }).join("");
  return `0x${digits}`;
};

const busIntervals = (signals: Signal[], durationNs: number, radix: WaveformGroup["radix"]) => {
  const times = [...new Set([
    0,
    durationNs,
    ...signals.flatMap((signal) => signal.samples.map((sample) => sample.timeNs)),
  ])].filter((time) => time >= 0 && time <= durationNs).sort((left, right) => left - right);
  const raw = times.slice(0, -1).map((time, index) => ({
    start: time,
    end: times[index + 1],
    value: formatBus(signals, time, radix),
  }));
  return raw.reduce<Array<{ start: number; end: number; value: string }>>((merged, interval) => {
    const previous = merged.at(-1);
    if (previous?.value === interval.value) {
      previous.end = interval.end;
    } else {
      merged.push({ ...interval });
    }
    return merged;
  }, []);
};

export default function WaveformView({
  config,
  result,
  running,
  groups,
  onConfig,
  onRun,
  onGroups,
}: Props) {
  const rowHeight = 52;
  const baseChartWidth = 1000;
  const [timeZoom, setTimeZoom] = useState(1);
  const chartWidth = baseChartWidth * timeZoom;
  const [cursorNs, setCursorNs] = useState(0);
  const [edgeTimeNs, setEdgeTimeNs] = useState(0);
  const [groupName, setGroupName] = useState("");
  const [selectedSignals, setSelectedSignals] = useState<string[]>([]);
  const waveformSvg = useRef<SVGSVGElement>(null);
  const waveformScroll = useRef<HTMLDivElement>(null);

  useEffect(() => {
    setCursorNs(0);
    setEdgeTimeNs(0);
    setSelectedSignals([]);
    setTimeZoom(1);
  }, [result]);

  const signalByName = useMemo(
    () => new Map(result?.signals.map((signal) => [signal.name, signal]) ?? []),
    [result],
  );
  const validGroups = useMemo(() => groups.map((group) => ({
    group,
    signals: group.signals.map((name) => signalByName.get(name)).filter((signal): signal is Signal => Boolean(signal)),
  })).filter(({ signals }) => signals.length >= 2), [groups, signalByName]);
  const lanes = useMemo(() => {
    if (!result) return [];
    const claimed = new Set(validGroups.flatMap(({ group }) => group.signals));
    const next: Lane[] = [];
    for (const { group, signals } of validGroups) {
      next.push({ type: "bus", group, signals });
      if (!group.collapsed) {
        next.push(...signals.map((signal) => ({ type: "scalar", signal, member: true } as ScalarLane)));
      }
    }
    next.push(...result.signals
      .filter((signal) => !claimed.has(signal.name))
      .map((signal) => ({ type: "scalar", signal, member: false } as ScalarLane)));
    return next;
  }, [result, validGroups]);
  const chartHeight = Math.max(lanes.length * rowHeight + 34, 180);

  const update = (field: keyof WaveformConfig, value: string) => {
    onConfig({ ...config, [field]: Math.max(1, Number(value) || 1) });
  };
  const moveCursor = (event: React.PointerEvent<SVGSVGElement>) => {
    if (!result) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const fraction = Math.min(1, Math.max(0, (event.clientX - bounds.left) / Math.max(bounds.width, 1)));
    setCursorNs(fraction * result.durationNs);
  };
  const saveGroups = (next: WaveformGroup[]) => void onGroups(next);
  const patchGroup = (id: string, patch: Partial<WaveformGroup>) => {
    saveGroups(groups.map((group) => group.id === id ? { ...group, ...patch } : group));
  };
  const moveMember = (group: WaveformGroup, index: number, direction: -1 | 1) => {
    const target = index + direction;
    if (target < 0 || target >= group.signals.length) return;
    const signals = [...group.signals];
    [signals[index], signals[target]] = [signals[target], signals[index]];
    patchGroup(group.id, { signals });
  };
  const createGroup = () => {
    if (selectedSignals.length < 2) return;
    const name = groupName.trim() || `BUS${groups.length + 1}`;
    saveGroups([...groups, {
      id: globalThis.crypto.randomUUID(),
      name,
      signals: selectedSignals,
      radix: "hex",
      collapsed: true,
    }]);
    setGroupName("");
    setSelectedSignals([]);
  };
  const groupedSignals = new Set(groups.flatMap((group) => group.signals));
  const zoomTo = (nextZoom: number) => {
    const next = Math.min(32, Math.max(1, nextZoom));
    const scroll = waveformScroll.current;
    const oldZoom = timeZoom;
    const viewportWidth = Math.max(1, (scroll?.clientWidth ?? baseChartWidth) - 120);
    const focus = (scroll?.scrollLeft ?? 0) + viewportWidth / 2;
    setTimeZoom(next);
    requestAnimationFrame(() => {
      if (scroll) scroll.scrollLeft = focus * next / oldZoom - viewportWidth / 2;
    });
  };
  const tickCount = Math.max(10, Math.round(10 * timeZoom));

  return (
    <div className="waveform-view">
      <aside className="waveform-settings">
        <p className="eyebrow">Waveform Setup</p>
        <h2>Time-domain run</h2>
        <label>Simulation length <span>ns</span><input type="number" min="1" value={config.durationNs}
          onChange={(event) => update("durationNs", event.currentTarget.value)} /></label>
        <label>Clock period <span>ns</span><input type="number" min="2" value={config.clockPeriodNs}
          onChange={(event) => update("clockPeriodNs", event.currentTarget.value)} /></label>
        <label>Input changes <span>ns</span><input type="number" min="1" value={config.inputChangeNs}
          onChange={(event) => update("inputChangeNs", event.currentTarget.value)} /></label>
        <button className="run-waveform" disabled={running} onClick={onRun}>{running ? "Running…" : "Run waveform"}</button>
        <p className="waveform-note">Inputs named CLK or CLOCK use the clock period. Output edges include the educational 0.69 × R × C propagation estimate.</p>

        {result && <section className="waveform-group-editor">
          <p className="eyebrow">Signal groups</p>
          <input className="waveform-group-name" value={groupName} placeholder={`BUS${groups.length + 1}`}
            onChange={(event) => setGroupName(event.currentTarget.value)} />
          <div className="waveform-signal-picker">
            {result.signals.filter((signal) => !groupedSignals.has(signal.name)).map((signal) => (
              <label key={signal.name}><input type="checkbox" checked={selectedSignals.includes(signal.name)}
                onChange={() => setSelectedSignals((current) => current.includes(signal.name)
                  ? current.filter((name) => name !== signal.name) : [...current, signal.name])} />
                <span>{signal.name}</span></label>
            ))}
          </div>
          <button className="run-waveform" disabled={selectedSignals.length < 2} onClick={createGroup}>Create bus</button>
          <div className="waveform-groups">
            {groups.map((group) => <div className="waveform-group-card" key={group.id}>
              <div><input className="waveform-group-card-name" defaultValue={group.name}
                onBlur={(event) => { const name = event.currentTarget.value.trim(); if (name && name !== group.name) patchGroup(group.id, { name }); }} />
                <button onClick={() => saveGroups(groups.filter(({ id }) => id !== group.id))}>×</button></div>
              <div className="waveform-group-actions">
                <button onClick={() => patchGroup(group.id, { collapsed: !group.collapsed })}>{group.collapsed ? "Expand" : "Collapse"}</button>
                <button onClick={() => patchGroup(group.id, { radix: group.radix === "hex" ? "binary" : "hex" })}>{group.radix === "hex" ? "HEX" : "BIN"}</button>
              </div>
              <ol>{group.signals.map((signal, index) => <li key={signal}><span>{signal}</span><button onClick={() => moveMember(group, index, -1)}>↑</button><button onClick={() => moveMember(group, index, 1)}>↓</button>{group.signals.length > 2 && <button onClick={() => patchGroup(group.id, { signals: group.signals.filter((name) => name !== signal) })}>×</button>}</li>)}</ol>
            </div>)}
          </div>
        </section>}

        <div className="waveform-key">{(["HIGH", "LOW", "FLOATING", "CONTENDED", "UNKNOWN"] as LogicState[]).map((state) => (
          <span key={state} className={`logic-${state.toLowerCase()}`}>{state}</span>
        ))}</div>
      </aside>

      <section className="waveform-canvas">
        <div className="waveform-header"><div><strong>Waveforms</strong><small>{result ? `${lanes.length} lanes · ${result.durationNs} ns · cursor ${cursorNs.toFixed(3)} ns · ${timeZoom.toFixed(1)}×` : "Configure and run a simulation"}</small></div>
          {result && <div className="waveform-zoom-controls"><button onClick={() => zoomTo(1)}>Fit</button><button disabled={timeZoom <= 1} onClick={() => zoomTo(timeZoom / 1.5)}>−</button><button disabled={timeZoom >= 32} onClick={() => zoomTo(timeZoom * 1.5)}>+</button></div>}
        </div>
        {result ? <div ref={waveformScroll} className="waveform-scroll" onWheel={(event) => {
          if (!event.ctrlKey && !event.metaKey) return;
          event.preventDefault();
          zoomTo(timeZoom * (event.deltaY < 0 ? 1.25 : 0.8));
        }} onScroll={(event) => {
          const width = waveformSvg.current?.getBoundingClientRect().width ?? chartWidth;
          setEdgeTimeNs(Math.min(result.durationNs, Math.max(0, event.currentTarget.scrollLeft / Math.max(width, 1) * result.durationNs)));
        }}>
          <div className="waveform-labels" style={{ height: chartHeight }}><div className="time-label">Signal</div>
            {lanes.map((lane) => lane.type === "bus" ? <div key={`bus-${lane.group.id}`} className="waveform-label bus">
              <small>{lane.group.radix} · {lane.signals.length} bits · MSB→LSB</small>
              <div className="waveform-label-heading"><strong>{lane.group.name}</strong><span className="waveform-value logic-bus">{formatBus(lane.signals, cursorNs, lane.group.radix)}</span></div>
              <span className="waveform-edge-value edge-bus">{formatBus(lane.signals, edgeTimeNs, lane.group.radix)}</span>
            </div> : <div key={`scalar-${lane.signal.kind}-${lane.signal.name}`} className={`waveform-label ${lane.signal.kind} ${lane.member ? "bus-member" : ""}`}>
              <small>{lane.member ? `member · ${lane.signal.kind}` : lane.signal.kind}</small>
              <div className="waveform-label-heading"><strong>{lane.signal.name}</strong><span className={`waveform-value logic-${valueAt(lane.signal.samples, cursorNs).toLowerCase()}`}>{valueAt(lane.signal.samples, cursorNs)}</span></div>
              {(() => { const state = valueAt(lane.signal.samples, edgeTimeNs); return <span className={`waveform-edge-value edge-${state.toLowerCase()}`}>{binaryValue(state)}</span>; })()}
            </div>)}
          </div>
          <svg ref={waveformSvg} className="waveform-svg" viewBox={`0 0 ${chartWidth} ${chartHeight}`} preserveAspectRatio="none" style={{ width: chartWidth, height: chartHeight }}
            onPointerDown={(event) => { event.currentTarget.setPointerCapture(event.pointerId); moveCursor(event); }}
            onPointerMove={(event) => { if (event.buttons & 1) moveCursor(event); }}>
            <g className="time-grid">{Array.from({ length: tickCount + 1 }, (_, index) => { const x = index * chartWidth / tickCount; return <g key={index}><line x1={x} y1="25" x2={x} y2={chartHeight} /><text x={x + 3} y="14">{Number((result.durationNs * index / tickCount).toFixed(3))} ns</text></g>; })}</g>
            {lanes.map((lane, laneIndex) => {
              const top = 27 + laneIndex * rowHeight;
              if (lane.type === "bus") {
                const intervals = busIntervals(lane.signals, result.durationNs, lane.group.radix);
                return <g key={`bus-${lane.group.id}`} className="waveform-bus-trace"><line className="row-guide" x1="0" y1={top + rowHeight - 1} x2={chartWidth} y2={top + rowHeight - 1} />
                  {intervals.map((interval, index) => { const x = interval.start / result.durationNs * chartWidth; const nextX = interval.end / result.durationNs * chartWidth; const changed = index > 0 && intervals[index - 1].value !== interval.value; return <g key={`${interval.start}-${interval.value}`}>
                    {changed && <><path d={`M ${x - 5} ${top + 12} L ${x + 5} ${top + 38}`} /><path d={`M ${x - 5} ${top + 38} L ${x + 5} ${top + 12}`} /></>}
                    <path d={`M ${x + (changed ? 5 : 0)} ${top + 12} H ${nextX - 5}`} /><path d={`M ${x + (changed ? 5 : 0)} ${top + 38} H ${nextX - 5}`} />
                    {nextX - x > 48 && <text x={(x + nextX) / 2} y={top + 29}>{interval.value}</text>}
                  </g>; })}
                </g>;
              }
              const signal = lane.signal;
              return <g key={`scalar-${signal.kind}-${signal.name}`} className="waveform-trace"><line className="row-guide" x1="0" y1={top + rowHeight - 1} x2={chartWidth} y2={top + rowHeight - 1} />
                {signal.samples.map((sample, index) => { const next = signal.samples[index + 1]; const x = sample.timeNs / result.durationNs * chartWidth; const nextX = (next?.timeNs ?? result.durationNs) / result.durationNs * chartWidth; const y = stateY(sample.state, top); const nextY = next ? stateY(next.state, top) : y; return <g key={`${sample.timeNs}-${index}`} className={`trace-${sample.state.toLowerCase()}`}><path d={`M ${x} ${y} H ${nextX}`} />{next && nextY !== y && <path d={`M ${nextX} ${y} V ${nextY}`} />}</g>; })}
              </g>;
            })}
            <line className="waveform-cursor" x1={cursorNs / result.durationNs * chartWidth} x2={cursorNs / result.durationNs * chartWidth} y1="0" y2={chartHeight} />
          </svg>
        </div> : <div className="waveform-empty"><span>⌁</span><p>Run the waveform simulation to inspect input and output transitions.</p></div>}
      </section>
    </div>
  );
}
