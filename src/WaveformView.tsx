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

export default function WaveformView({ config, result, running, onConfig, onRun }: Props) {
  const rowHeight = 52;
  const chartWidth = 1000;
  const chartHeight = Math.max((result?.signals.length ?? 1) * rowHeight + 34, 180);
  const update = (field: keyof WaveformConfig, value: string) => {
    onConfig({ ...config, [field]: Math.max(1, Number(value) || 1) });
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
          Inputs named CLK or CLOCK use the clock period. Other inputs advance as a binary sequence.
        </p>
        <div className="waveform-key">
          {(["HIGH", "LOW", "FLOATING", "CONTENDED", "UNKNOWN"] as LogicState[]).map((state) => (
            <span key={state} className={`logic-${state.toLowerCase()}`}>{state}</span>
          ))}
        </div>
      </aside>
      <section className="waveform-canvas">
        <div className="waveform-header">
          <div><strong>Waveforms</strong><small>{result ? `${result.signals.length} signals · ${result.durationNs} ns` : "Configure and run a simulation"}</small></div>
        </div>
        {result ? (
          <div className="waveform-scroll">
            <div className="waveform-labels" style={{ height: chartHeight }}>
              <div className="time-label">Signal</div>
              {result.signals.map((signal) => (
                <div key={`${signal.kind}-${signal.name}`} className={`waveform-label ${signal.kind}`}>
                  <small>{signal.kind}</small><strong>{signal.name}</strong>
                </div>
              ))}
            </div>
            <svg className="waveform-svg" viewBox={`0 0 ${chartWidth} ${chartHeight}`}
              preserveAspectRatio="none" style={{ width: chartWidth, height: chartHeight }}>
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
            </svg>
          </div>
        ) : (
          <div className="waveform-empty"><span>⌁</span><p>Run the waveform simulation to inspect input and output transitions.</p></div>
        )}
      </section>
    </div>
  );
}
