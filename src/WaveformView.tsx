import { useEffect, useRef, useState } from "react";
import type { LogicState, WaveformConfig, WaveformResult } from "./types";

type Props = {
  config: WaveformConfig;
  result: WaveformResult | null;
  running: boolean;
  onConfig: (config: WaveformConfig) => void;
  onRun: () => void;
};

const stateY = (state: LogicState, top: number) => {
  if (state === "HIGH") return top + 11;
  if (state === "LOW") return top + 39;
  return top + 25;
};

const valueAt = (
  samples: Array<{ timeNs: number; state: LogicState }>,
  timeNs: number,
) => samples.reduce(
  (current, sample) => sample.timeNs <= timeNs ? sample.state : current,
  samples[0]?.state ?? "UNKNOWN",
);

const binaryValue = (state: LogicState) =>
  state === "HIGH" ? "1" : state === "LOW" ? "0" : "X";

export default function WaveformView({ config, result, running, onConfig, onRun }: Props) {
  const rowHeight = 52;
  const chartWidth = 1000;
  const chartHeight = Math.max((result?.signals.length ?? 1) * rowHeight + 34, 180);
  const [cursorNs, setCursorNs] = useState(0);
  const [edgeTimeNs, setEdgeTimeNs] = useState(0);
  const waveformSvg = useRef<SVGSVGElement>(null);
  useEffect(() => {
    setCursorNs(0);
    setEdgeTimeNs(0);
  }, [result]);
  const update = (field: keyof WaveformConfig, value: string) => {
    onConfig({ ...config, [field]: Math.max(1, Number(value) || 1) });
  };
  const moveCursor = (event: React.PointerEvent<SVGSVGElement>) => {
    if (!result) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const fraction = Math.min(
      1,
      Math.max(0, (event.clientX - bounds.left) / Math.max(bounds.width, 1)),
    );
    setCursorNs(fraction * result.durationNs);
  };

  return (
    <div className="waveform-view">
      <aside className="waveform-settings">
        <p className="eyebrow">Waveform Setup</p>
        <h2>Time-domain run</h2>
        <label>Simulation length <span>ns</span>
          <input type="number" min="1" value={config.durationNs}
            onChange={(event) => update("durationNs", event.currentTarget.value)} />
        </label>
        <label>Clock period <span>ns</span>
          <input type="number" min="2" value={config.clockPeriodNs}
            onChange={(event) => update("clockPeriodNs", event.currentTarget.value)} />
        </label>
        <label>Input changes <span>ns</span>
          <input type="number" min="1" value={config.inputChangeNs}
            onChange={(event) => update("inputChangeNs", event.currentTarget.value)} />
        </label>
        <button className="run-waveform" disabled={running} onClick={onRun}>
          {running ? "Running…" : "Run waveform"}
        </button>
        <p className="waveform-note">
          Inputs named CLK or CLOCK use the clock period. Output edges include the educational 0.69 × R × C propagation estimate.
        </p>
        <div className="waveform-key">
          {(["HIGH", "LOW", "FLOATING", "CONTENDED", "UNKNOWN"] as LogicState[]).map((state) => (
            <span key={state} className={`logic-${state.toLowerCase()}`}>{state}</span>
          ))}
        </div>
      </aside>
      <section className="waveform-canvas">
        <div className="waveform-header">
          <div><strong>Waveforms</strong><small>{result ? `${result.signals.length} signals · ${result.durationNs} ns · cursor ${cursorNs.toFixed(3)} ns` : "Configure and run a simulation"}</small></div>
        </div>
        {result ? (
          <div className="waveform-scroll" onScroll={(event) => {
            const width = waveformSvg.current?.getBoundingClientRect().width ?? chartWidth;
            setEdgeTimeNs(Math.min(
              result.durationNs,
              Math.max(0, event.currentTarget.scrollLeft / Math.max(width, 1) * result.durationNs),
            ));
          }}>
            <div className="waveform-labels" style={{ height: chartHeight }}>
              <div className="time-label">Signal</div>
              {result.signals.map((signal) => (
                <div key={`${signal.kind}-${signal.name}`} className={`waveform-label ${signal.kind}`}>
                  <small>{signal.kind}</small>
                  <div className="waveform-label-heading">
                    <strong>{signal.name}</strong>
                    <span className={`waveform-value logic-${valueAt(signal.samples, cursorNs).toLowerCase()}`}>
                      {valueAt(signal.samples, cursorNs)}
                    </span>
                  </div>
                  {(() => {
                    const state = valueAt(signal.samples, edgeTimeNs);
                    return (
                      <span className={`waveform-edge-value edge-${state.toLowerCase()}`}>
                        {binaryValue(state)}
                      </span>
                    );
                  })()}
                </div>
              ))}
            </div>
            <svg ref={waveformSvg} className="waveform-svg" viewBox={`0 0 ${chartWidth} ${chartHeight}`}
              preserveAspectRatio="none" style={{ width: chartWidth, height: chartHeight }}
              onPointerDown={(event) => {
                event.currentTarget.setPointerCapture(event.pointerId);
                moveCursor(event);
              }}
              onPointerMove={(event) => {
                if (event.buttons & 1) moveCursor(event);
              }}>
              <g className="time-grid">
                {Array.from({ length: 11 }, (_, index) => {
                  const x = index * chartWidth / 10;
                  return <g key={index}><line x1={x} y1="25" x2={x} y2={chartHeight} /><text x={x + 3} y="14">{Math.round(result.durationNs * index / 10)} ns</text></g>;
                })}
              </g>
              {result.signals.map((signal, signalIndex) => {
                const top = 27 + signalIndex * rowHeight;
                return <g key={`${signal.kind}-${signal.name}`} className="waveform-trace">
                  <line className="row-guide" x1="0" y1={top + rowHeight - 1} x2={chartWidth} y2={top + rowHeight - 1} />
                  {signal.samples.map((sample, index) => {
                    const next = signal.samples[index + 1];
                    const x = sample.timeNs / result.durationNs * chartWidth;
                    const nextX = (next?.timeNs ?? result.durationNs) / result.durationNs * chartWidth;
                    const y = stateY(sample.state, top);
                    const nextY = next ? stateY(next.state, top) : y;
                    return <g key={`${sample.timeNs}-${index}`} className={`trace-${sample.state.toLowerCase()}`}>
                      <path d={`M ${x} ${y} H ${nextX}`} />
                      {next && nextY !== y && <path d={`M ${nextX} ${y} V ${nextY}`} />}
                    </g>;
                  })}
                </g>;
              })}
              <line
                className="waveform-cursor"
                x1={cursorNs / result.durationNs * chartWidth}
                x2={cursorNs / result.durationNs * chartWidth}
                y1="0"
                y2={chartHeight}
              />
            </svg>
          </div>
        ) : (
          <div className="waveform-empty"><span>⌁</span><p>Run the waveform simulation to inspect input and output transitions.</p></div>
        )}
      </section>
    </div>
  );
}
