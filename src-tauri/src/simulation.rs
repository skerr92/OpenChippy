use crate::model::{Component, Project, TerminalRef};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogicState {
    High,
    Low,
    Floating,
    Contended,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SwitchState {
    On,
    Off,
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedState {
    pub name: String,
    pub state: LogicState,
    pub voltage: Option<f64>,
    pub high_drive_resistance_ohms: Option<f64>,
    pub low_drive_resistance_ohms: Option<f64>,
    pub load_capacitance_ff: f64,
    pub estimated_delay_ns: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransistorState {
    pub component_id: Uuid,
    pub name: String,
    pub state: SwitchState,
    pub gate_voltage: Option<f64>,
    pub threshold_voltage: f64,
    pub effective_on_resistance_ohms: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WireState {
    pub wire_id: Uuid,
    pub state: LogicState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationResult {
    pub nets: Vec<NamedState>,
    pub outputs: Vec<NamedState>,
    pub transistors: Vec<TransistorState>,
    pub wires: Vec<WireState>,
    pub converged: bool,
    pub supply_voltage: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruthTableRow {
    pub inputs: Vec<LogicState>,
    pub outputs: Vec<LogicState>,
    pub converged: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TruthTableResult {
    pub input_names: Vec<String>,
    pub output_names: Vec<String>,
    pub rows: Vec<TruthTableRow>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformConfig {
    pub duration_ns: u32,
    pub clock_period_ns: u32,
    pub input_change_ns: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformSample {
    pub time_ns: f64,
    pub state: LogicState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformSignal {
    pub name: String,
    pub kind: &'static str,
    pub samples: Vec<WaveformSample>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformResult {
    pub duration_ns: u32,
    pub signals: Vec<WaveformSignal>,
}

type TerminalKey = (Uuid, String);

fn terminals(component: &Component) -> &'static [&'static str] {
    match component.kind.as_str() {
        "nmos" | "pmos" => &["gate", "drain", "source"],
        "resistor" => &["a", "b"],
        "output" => &["in"],
        "junction" | "net_label" => &["node"],
        _ => &["out"],
    }
}

fn terminal_key(reference: &TerminalRef) -> TerminalKey {
    (reference.component_id, reference.terminal.clone())
}

#[derive(Default)]
struct DisjointSet {
    parents: Vec<usize>,
}

impl DisjointSet {
    fn add(&mut self) -> usize {
        let index = self.parents.len();
        self.parents.push(index);
        index
    }

    fn find(&mut self, index: usize) -> usize {
        if self.parents[index] != index {
            self.parents[index] = self.find(self.parents[index]);
        }
        self.parents[index]
    }

    fn union(&mut self, left: usize, right: usize) {
        let left = self.find(left);
        let right = self.find(right);
        if left != right {
            self.parents[right] = left;
        }
    }
}

#[derive(Default)]
struct Drives {
    high: bool,
    low: bool,
    unknown: bool,
}

impl Drives {
    fn add(&mut self, state: LogicState) {
        match state {
            LogicState::High => self.high = true,
            LogicState::Low => self.low = true,
            LogicState::Unknown | LogicState::Contended => self.unknown = true,
            LogicState::Floating => {}
        }
    }

    fn state(&self) -> LogicState {
        if self.high && self.low {
            LogicState::Contended
        } else if self.unknown {
            LogicState::Unknown
        } else if self.high {
            LogicState::High
        } else if self.low {
            LogicState::Low
        } else {
            LogicState::Floating
        }
    }
}

struct Transistor {
    component_id: Uuid,
    name: String,
    pmos: bool,
    gate: usize,
    drain: usize,
    source: usize,
    threshold_voltage: f64,
    effective_on_resistance_ohms: f64,
}

fn switch_state(transistor: &Transistor, gate: LogicState, supply_voltage: f64) -> SwitchState {
    let gate_voltage = match gate {
        LogicState::High => Some(supply_voltage),
        LogicState::Low => Some(0.0),
        // An undriven internal gate starts FLOATING while the combinational
        // network settles. Keeping it open lets resolved upstream stages
        // propagate without injecting provisional UNKNOWN drives into rails.
        LogicState::Floating => return SwitchState::Off,
        LogicState::Unknown | LogicState::Contended => None,
    };
    let Some(gate_voltage) = gate_voltage else {
        return SwitchState::Unknown;
    };
    let overdrive = if transistor.pmos {
        supply_voltage - gate_voltage - transistor.threshold_voltage.abs()
    } else {
        gate_voltage - transistor.threshold_voltage
    };
    let selected = if transistor.pmos {
        gate == LogicState::Low
    } else {
        gate == LogicState::High
    };
    if selected && overdrive > 0.0 {
        SwitchState::On
    } else {
        SwitchState::Off
    }
}

fn shortest_drive_resistances(
    net_count: usize,
    transistors: &[Transistor],
    switches: &[SwitchState],
    direct_drives: &[Vec<LogicState>],
    drive: LogicState,
) -> Vec<Option<f64>> {
    let mut distances = vec![f64::INFINITY; net_count];
    let mut visited = vec![false; net_count];
    for (net, states) in direct_drives.iter().enumerate() {
        if states.contains(&drive) {
            distances[net] = 0.0;
        }
    }
    for _ in 0..net_count {
        let Some(current) = (0..net_count)
            .filter(|net| !visited[*net] && distances[*net].is_finite())
            .min_by(|left, right| distances[*left].total_cmp(&distances[*right]))
        else {
            break;
        };
        visited[current] = true;
        for (transistor, state) in transistors.iter().zip(switches) {
            if *state != SwitchState::On {
                continue;
            }
            let neighbor = if transistor.drain == current {
                Some(transistor.source)
            } else if transistor.source == current {
                Some(transistor.drain)
            } else {
                None
            };
            if let Some(neighbor) = neighbor {
                distances[neighbor] = distances[neighbor]
                    .min(distances[current] + transistor.effective_on_resistance_ohms);
            }
        }
    }
    distances
        .into_iter()
        .map(|distance| distance.is_finite().then_some(distance))
        .collect()
}

pub fn simulate(project: &Project, inputs: &HashMap<String, LogicState>) -> SimulationResult {
    let mut terminal_sets = DisjointSet::default();
    let mut terminal_indices = HashMap::new();
    for component in &project.components {
        for terminal in terminals(component) {
            terminal_indices.insert((component.id, (*terminal).into()), terminal_sets.add());
        }
    }

    for wire in &project.wires {
        let Some(to) = &wire.to else {
            continue;
        };
        let from = terminal_indices.get(&terminal_key(&wire.from));
        let to = terminal_indices.get(&terminal_key(to));
        if let (Some(from), Some(to)) = (from, to) {
            terminal_sets.union(*from, *to);
        }
    }

    let mut labels: HashMap<&str, usize> = HashMap::new();
    for component in project
        .components
        .iter()
        .filter(|component| component.kind == "net_label")
    {
        let index = terminal_indices[&(component.id, "node".into())];
        if let Some(previous) = labels.insert(&component.name, index) {
            terminal_sets.union(previous, index);
        }
    }

    let mut roots = HashMap::new();
    let mut terminal_nets = HashMap::new();
    for (terminal, index) in &terminal_indices {
        let root = terminal_sets.find(*index);
        let next = roots.len();
        let net = *roots.entry(root).or_insert(next);
        terminal_nets.insert(terminal.clone(), net);
    }
    let net_count = roots.len();
    let mut direct_drives = vec![Vec::new(); net_count];
    let mut net_names = vec![Vec::new(); net_count];

    for component in &project.components {
        let driven_state = match component.kind.as_str() {
            "vdd" => Some(LogicState::High),
            "gnd" => Some(LogicState::Low),
            "input" => Some(
                inputs
                    .get(&component.name)
                    .copied()
                    .unwrap_or(LogicState::Unknown),
            ),
            _ => None,
        };
        for terminal in terminals(component) {
            let net = terminal_nets[&(component.id, (*terminal).into())];
            if let Some(state) = driven_state {
                direct_drives[net].push(state);
            }
            if matches!(
                component.kind.as_str(),
                "vdd" | "gnd" | "input" | "output" | "net_label"
            ) {
                net_names[net].push(component.name.clone());
            }
        }
    }

    let transistors: Vec<_> = project
        .components
        .iter()
        .filter(|component| component.kind == "nmos" || component.kind == "pmos")
        .map(|component| Transistor {
            component_id: component.id,
            name: component.name.clone(),
            pmos: component.kind == "pmos",
            gate: terminal_nets[&(component.id, "gate".into())],
            drain: terminal_nets[&(component.id, "drain".into())],
            source: terminal_nets[&(component.id, "source".into())],
            threshold_voltage: if component.kind == "pmos" {
                project.technology.pmos.threshold_voltage
            } else {
                project.technology.nmos.threshold_voltage
            },
            effective_on_resistance_ohms: project
                .device_characteristics(component.id)
                .expect("transistor characteristics are valid")
                .effective_on_resistance_ohms,
        })
        .collect();

    let mut states = direct_drives
        .iter()
        .map(|drives| {
            let mut combined = Drives::default();
            drives.iter().for_each(|state| combined.add(*state));
            combined.state()
        })
        .collect::<Vec<_>>();
    let mut switches = vec![SwitchState::Unknown; transistors.len()];
    let mut converged = false;

    for _ in 0..64 {
        let next_switches = transistors
            .iter()
            .map(|transistor| {
                switch_state(
                    transistor,
                    states[transistor.gate],
                    project.technology.supply_voltage,
                )
            })
            .collect::<Vec<_>>();

        let mut conductive = DisjointSet::default();
        for _ in 0..net_count {
            conductive.add();
        }
        for (transistor, state) in transistors.iter().zip(&next_switches) {
            if *state == SwitchState::On {
                conductive.union(transistor.drain, transistor.source);
            }
        }

        let mut group_drives: HashMap<usize, Drives> = HashMap::new();
        for (net, drives) in direct_drives.iter().enumerate() {
            let group = conductive.find(net);
            for state in drives {
                group_drives.entry(group).or_default().add(*state);
            }
        }
        for (transistor, state) in transistors.iter().zip(&next_switches) {
            if *state != SwitchState::Unknown {
                continue;
            }
            if matches!(
                states[transistor.drain],
                LogicState::Floating | LogicState::Unknown
            ) && states[transistor.source] != LogicState::Floating
            {
                let drain_group = conductive.find(transistor.drain);
                group_drives
                    .entry(drain_group)
                    .or_default()
                    .add(LogicState::Unknown);
            }
            if matches!(
                states[transistor.source],
                LogicState::Floating | LogicState::Unknown
            ) && states[transistor.drain] != LogicState::Floating
            {
                let source_group = conductive.find(transistor.source);
                group_drives
                    .entry(source_group)
                    .or_default()
                    .add(LogicState::Unknown);
            }
        }
        let next_states = (0..net_count)
            .map(|net| {
                group_drives
                    .get(&conductive.find(net))
                    .map_or(LogicState::Floating, Drives::state)
            })
            .collect::<Vec<_>>();
        if next_states == states && next_switches == switches {
            converged = true;
            break;
        }
        states = next_states;
        switches = next_switches;
    }

    let high_resistance = shortest_drive_resistances(
        net_count,
        &transistors,
        &switches,
        &direct_drives,
        LogicState::High,
    );
    let low_resistance = shortest_drive_resistances(
        net_count,
        &transistors,
        &switches,
        &direct_drives,
        LogicState::Low,
    );
    let mut net_capacitance_ff = vec![0.0; net_count];
    for transistor in &transistors {
        let characteristics = project
            .device_characteristics(transistor.component_id)
            .expect("transistor characteristics are valid");
        net_capacitance_ff[transistor.gate] += characteristics.gate_capacitance_ff;
        net_capacitance_ff[transistor.drain] += characteristics.diffusion_capacitance_ff;
        net_capacitance_ff[transistor.source] += characteristics.diffusion_capacitance_ff;
    }
    let named_state = |net: usize, name: String| NamedState {
        name,
        state: states[net],
        voltage: match states[net] {
            LogicState::High => Some(project.technology.supply_voltage),
            LogicState::Low => Some(0.0),
            LogicState::Floating | LogicState::Contended | LogicState::Unknown => None,
        },
        high_drive_resistance_ohms: high_resistance[net],
        low_drive_resistance_ohms: low_resistance[net],
        load_capacitance_ff: net_capacitance_ff[net],
        estimated_delay_ns: match states[net] {
            LogicState::High => high_resistance[net],
            LogicState::Low => low_resistance[net],
            LogicState::Floating | LogicState::Contended | LogicState::Unknown => None,
        }
        .map(|resistance| 0.69 * resistance * net_capacitance_ff[net] * 1e-6),
    };

    let mut nets = (0..net_count)
        .map(|net| NamedState {
            ..named_state(
                net,
                net_names[net]
                    .first()
                    .cloned()
                    .unwrap_or_else(|| format!("net-{}", net + 1)),
            )
        })
        .collect::<Vec<_>>();
    nets.sort_by(|left, right| left.name.cmp(&right.name));

    let mut outputs = project
        .components
        .iter()
        .filter(|component| component.kind == "output")
        .map(|component| {
            named_state(
                terminal_nets[&(component.id, "in".into())],
                component.name.clone(),
            )
        })
        .collect::<Vec<_>>();
    outputs.sort_by(|left, right| left.name.cmp(&right.name));

    SimulationResult {
        nets,
        outputs,
        transistors: transistors
            .iter()
            .zip(switches)
            .map(|(transistor, state)| TransistorState {
                component_id: transistor.component_id,
                name: transistor.name.clone(),
                state,
                gate_voltage: match states[transistor.gate] {
                    LogicState::High => Some(project.technology.supply_voltage),
                    LogicState::Low => Some(0.0),
                    _ => None,
                },
                threshold_voltage: transistor.threshold_voltage,
                effective_on_resistance_ohms: transistor.effective_on_resistance_ohms,
            })
            .collect(),
        wires: project
            .wires
            .iter()
            .map(|wire| WireState {
                wire_id: wire.id,
                state: states[terminal_nets[&terminal_key(&wire.from)]],
            })
            .collect(),
        converged,
        supply_voltage: project.technology.supply_voltage,
    }
}

pub fn truth_table(project: &Project) -> Result<TruthTableResult, String> {
    let input_names = project
        .components
        .iter()
        .filter(|component| component.kind == "input")
        .map(|component| component.name.clone())
        .collect::<Vec<_>>();
    if input_names.len() > 8 {
        return Err("truth tables are limited to 8 digital inputs".into());
    }
    let output_names = project
        .components
        .iter()
        .filter(|component| component.kind == "output")
        .map(|component| component.name.clone())
        .collect::<Vec<_>>();
    let rows = (0..(1usize << input_names.len()))
        .map(|combination| {
            let input_states = input_names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let state = if combination & (1 << (input_names.len() - index - 1)) == 0 {
                        LogicState::Low
                    } else {
                        LogicState::High
                    };
                    (name.clone(), state)
                })
                .collect::<HashMap<_, _>>();
            let result = simulate(project, &input_states);
            TruthTableRow {
                inputs: input_names.iter().map(|name| input_states[name]).collect(),
                outputs: output_names
                    .iter()
                    .map(|name| {
                        result
                            .outputs
                            .iter()
                            .find(|output| output.name == *name)
                            .map_or(LogicState::Unknown, |output| output.state)
                    })
                    .collect(),
                converged: result.converged,
            }
        })
        .collect();
    Ok(TruthTableResult {
        input_names,
        output_names,
        rows,
    })
}

pub fn waveform(project: &Project, config: WaveformConfig) -> Result<WaveformResult, String> {
    if config.duration_ns == 0 {
        return Err("simulation length must be greater than zero".into());
    }
    if config.clock_period_ns < 2 {
        return Err("clock period must be at least 2 ns".into());
    }
    if config.input_change_ns == 0 {
        return Err("input change interval must be greater than zero".into());
    }
    if config.duration_ns > 1_000_000 {
        return Err("simulation length is limited to 1,000,000 ns".into());
    }

    let input_names = project
        .components
        .iter()
        .filter(|component| component.kind == "input")
        .map(|component| component.name.clone())
        .collect::<Vec<_>>();
    let output_names = project
        .components
        .iter()
        .filter(|component| component.kind == "output")
        .map(|component| component.name.clone())
        .collect::<Vec<_>>();
    let is_clock = |name: &str| {
        let uppercase = name.to_ascii_uppercase();
        uppercase == "CLK" || uppercase.contains("CLOCK")
    };
    let ordinary_inputs = input_names
        .iter()
        .filter(|name| !is_clock(name))
        .cloned()
        .collect::<Vec<_>>();

    let mut times = BTreeSet::from([0, config.duration_ns]);
    let half_clock = config.clock_period_ns / 2;
    for time in (half_clock..config.duration_ns).step_by(half_clock as usize) {
        times.insert(time);
    }
    for time in
        (config.input_change_ns..config.duration_ns).step_by(config.input_change_ns as usize)
    {
        times.insert(time);
    }
    if times.len() > 10_000 {
        return Err("waveform would exceed 10,000 transition points".into());
    }

    let mut signal_samples = input_names
        .iter()
        .chain(&output_names)
        .map(|name| (name.clone(), Vec::new()))
        .collect::<HashMap<_, _>>();
    for time in times {
        let step = time / config.input_change_ns;
        let inputs = input_names
            .iter()
            .map(|name| {
                let state = if is_clock(name) {
                    if (time / half_clock) % 2 == 0 {
                        LogicState::Low
                    } else {
                        LogicState::High
                    }
                } else {
                    let index = ordinary_inputs
                        .iter()
                        .position(|candidate| candidate == name)
                        .unwrap_or(0);
                    let shift = ordinary_inputs.len().saturating_sub(index + 1);
                    if (step >> shift) & 1 == 0 {
                        LogicState::Low
                    } else {
                        LogicState::High
                    }
                };
                signal_samples
                    .get_mut(name)
                    .expect("input signal exists")
                    .push(WaveformSample {
                        time_ns: f64::from(time),
                        state,
                    });
                (name.clone(), state)
            })
            .collect::<HashMap<_, _>>();
        let result = simulate(project, &inputs);
        for output in result.outputs {
            let samples = signal_samples
                .get_mut(&output.name)
                .expect("output signal exists");
            let previous = samples.last().map(|sample| sample.state);
            if previous == Some(output.state) {
                continue;
            }
            let delay = if time == 0 {
                0.0
            } else {
                output.estimated_delay_ns.unwrap_or(0.0)
            };
            let transition_time = f64::from(time) + delay;
            if transition_time <= f64::from(config.duration_ns) {
                samples.push(WaveformSample {
                    time_ns: transition_time,
                    state: output.state,
                });
            }
        }
    }

    let mut signals = Vec::new();
    for name in &input_names {
        signals.push(WaveformSignal {
            name: name.clone(),
            kind: "input",
            samples: signal_samples.remove(name).unwrap_or_default(),
        });
    }
    for name in &output_names {
        let mut samples = signal_samples.remove(name).unwrap_or_default();
        samples.sort_by(|left, right| left.time_ns.total_cmp(&right.time_ns));
        signals.push(WaveformSignal {
            name: name.clone(),
            kind: "output",
            samples,
        });
    }
    Ok(WaveformResult {
        duration_ns: config.duration_ns,
        signals,
    })
}

#[cfg(test)]
mod tests {
    use super::{simulate, truth_table, waveform, LogicState, WaveformConfig};
    use crate::model::{Project, TerminalRef};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn terminal(component_id: Uuid, terminal: &str) -> TerminalRef {
        TerminalRef {
            component_id,
            terminal: terminal.into(),
        }
    }

    fn connect(project: &mut Project, left: (Uuid, &str), right: (Uuid, &str)) {
        project
            .connect(terminal(left.0, left.1), terminal(right.0, right.1))
            .unwrap();
    }

    fn output(result: &super::SimulationResult) -> LogicState {
        result.outputs.first().unwrap().state
    }

    #[test]
    fn cmos_inverter_truth_table() {
        let mut project = Project::default();
        let vdd = project.add_component("vdd", 0.0, -4.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 4.0).unwrap();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        let output_probe = project.add_component("output", 4.0, 0.0).unwrap();
        let pmos = project.add_component("pmos", 0.0, -1.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 1.0).unwrap();
        connect(&mut project, (vdd, "out"), (pmos, "source"));
        connect(&mut project, (gnd, "out"), (nmos, "source"));
        connect(&mut project, (input, "out"), (pmos, "gate"));
        connect(&mut project, (input, "out"), (nmos, "gate"));
        connect(&mut project, (pmos, "drain"), (nmos, "drain"));
        connect(&mut project, (pmos, "drain"), (output_probe, "in"));

        for (input_state, expected) in [
            (LogicState::Low, LogicState::High),
            (LogicState::High, LogicState::Low),
            (LogicState::Unknown, LogicState::Unknown),
        ] {
            let result = simulate(&project, &HashMap::from([("IN1".into(), input_state)]));
            assert!(result.converged);
            assert_eq!(output(&result), expected);
        }
    }

    #[test]
    fn reusable_block_instances_flatten_through_the_production_solver() {
        let mut source = Project::default();
        let vdd = source.add_component("vdd", 0.0, -4.0).unwrap();
        let gnd = source.add_component("gnd", 0.0, 4.0).unwrap();
        let input = source.add_component("input", -4.0, 0.0).unwrap();
        let output_probe = source.add_component("output", 4.0, 0.0).unwrap();
        let pmos = source.add_component("pmos", 0.0, -1.0).unwrap();
        let nmos = source.add_component("nmos", 0.0, 1.0).unwrap();
        connect(&mut source, (vdd, "out"), (pmos, "source"));
        connect(&mut source, (gnd, "out"), (nmos, "source"));
        connect(&mut source, (input, "out"), (pmos, "gate"));
        connect(&mut source, (input, "out"), (nmos, "gate"));
        connect(&mut source, (pmos, "drain"), (nmos, "drain"));
        connect(&mut source, (pmos, "drain"), (output_probe, "in"));
        let definition = source.capture_block("INV".into()).unwrap();

        let mut parent = Project::default();
        parent.block_definitions = source.block_definitions.clone();
        let instance = parent.place_block(definition, 0.0, 0.0).unwrap();
        let top_input = parent.add_component("input", -4.0, 0.0).unwrap();
        let top_output = parent.add_component("output", 4.0, 0.0).unwrap();
        let top_vdd = parent.add_component("vdd", 0.0, -4.0).unwrap();
        let top_gnd = parent.add_component("gnd", 0.0, 4.0).unwrap();
        connect(&mut parent, (top_input, "out"), (instance, "IN1"));
        connect(&mut parent, (instance, "OUT1"), (top_output, "in"));
        connect(&mut parent, (top_vdd, "out"), (instance, "VDD1"));
        connect(&mut parent, (top_gnd, "out"), (instance, "GND1"));

        let flattened = parent.flattened().unwrap();
        assert!(!flattened
            .components
            .iter()
            .any(|component| component.kind == "block"));
        for (input_state, expected) in [
            (LogicState::Low, LogicState::High),
            (LogicState::High, LogicState::Low),
        ] {
            let result = simulate(&flattened, &HashMap::from([("IN1".into(), input_state)]));
            assert_eq!(
                result
                    .outputs
                    .iter()
                    .find(|output| output.name == "OUT1")
                    .unwrap()
                    .state,
                expected
            );
        }
        let serialized = serde_json::to_string(&parent).unwrap();
        let loaded: Project = serde_json::from_str(&serialized).unwrap();
        assert_eq!(loaded.block_definitions.len(), 1);
        assert_eq!(loaded.components[0].block_definition_id, Some(definition));
    }

    #[test]
    fn cascaded_reusable_nands_settle_before_propagating_unknowns() {
        let mut source = Project::default();
        let vdd = source.add_component("vdd", 0.0, -5.0).unwrap();
        let gnd = source.add_component("gnd", 0.0, 5.0).unwrap();
        let input_a = source.add_component("input", -5.0, -1.0).unwrap();
        let input_b = source.add_component("input", -5.0, 1.0).unwrap();
        let output_probe = source.add_component("output", 5.0, 0.0).unwrap();
        let pmos_a = source.add_component("pmos", -1.0, -2.0).unwrap();
        let pmos_b = source.add_component("pmos", 1.0, -2.0).unwrap();
        let nmos_a = source.add_component("nmos", 0.0, 1.0).unwrap();
        let nmos_b = source.add_component("nmos", 0.0, 3.0).unwrap();
        connect(&mut source, (vdd, "out"), (pmos_a, "source"));
        connect(&mut source, (vdd, "out"), (pmos_b, "source"));
        connect(&mut source, (pmos_a, "drain"), (output_probe, "in"));
        connect(&mut source, (pmos_b, "drain"), (output_probe, "in"));
        connect(&mut source, (nmos_a, "drain"), (output_probe, "in"));
        connect(&mut source, (nmos_a, "source"), (nmos_b, "drain"));
        connect(&mut source, (nmos_b, "source"), (gnd, "out"));
        connect(&mut source, (input_a, "out"), (pmos_a, "gate"));
        connect(&mut source, (input_a, "out"), (nmos_a, "gate"));
        connect(&mut source, (input_b, "out"), (pmos_b, "gate"));
        connect(&mut source, (input_b, "out"), (nmos_b, "gate"));
        let definition = source.capture_block("NAND2".into()).unwrap();

        let mut parent = Project::default();
        parent.block_definitions = source.block_definitions;
        let first = parent.place_block(definition, -2.0, 0.0).unwrap();
        let second = parent.place_block(definition, 2.0, 0.0).unwrap();
        let top_vdd = parent.add_component("vdd", 0.0, -5.0).unwrap();
        let top_gnd = parent.add_component("gnd", 0.0, 5.0).unwrap();
        let top_a = parent.add_component("input", -6.0, -1.0).unwrap();
        let top_b = parent.add_component("input", -6.0, 1.0).unwrap();
        let top_output = parent.add_component("output", 6.0, 0.0).unwrap();
        for instance in [first, second] {
            connect(&mut parent, (top_vdd, "out"), (instance, "VDD1"));
            connect(&mut parent, (top_gnd, "out"), (instance, "GND1"));
        }
        connect(&mut parent, (top_a, "out"), (first, "IN1"));
        connect(&mut parent, (top_b, "out"), (first, "IN2"));
        connect(&mut parent, (top_a, "out"), (second, "IN1"));
        connect(&mut parent, (first, "OUT1"), (second, "IN2"));
        connect(&mut parent, (second, "OUT1"), (top_output, "in"));

        let flattened = parent.flattened().unwrap();
        let result = simulate(
            &flattened,
            &HashMap::from([
                ("IN1".into(), LogicState::Low),
                ("IN2".into(), LogicState::Low),
            ]),
        );
        assert!(result.converged);
        assert_eq!(
            result
                .outputs
                .iter()
                .find(|output| output.name == "OUT1")
                .unwrap()
                .state,
            LogicState::High
        );
        assert!(result
            .transistors
            .iter()
            .all(|transistor| transistor.state != super::SwitchState::Unknown));
    }

    #[test]
    fn insufficient_supply_overdrive_prevents_conduction() {
        let mut project = Project::default();
        project.technology.supply_voltage = 0.3;
        let vdd = project.add_component("vdd", 0.0, -4.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 4.0).unwrap();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        let output_probe = project.add_component("output", 4.0, 0.0).unwrap();
        let pmos = project.add_component("pmos", 0.0, -1.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 1.0).unwrap();
        connect(&mut project, (vdd, "out"), (pmos, "source"));
        connect(&mut project, (gnd, "out"), (nmos, "source"));
        connect(&mut project, (input, "out"), (pmos, "gate"));
        connect(&mut project, (input, "out"), (nmos, "gate"));
        connect(&mut project, (pmos, "drain"), (nmos, "drain"));
        connect(&mut project, (pmos, "drain"), (output_probe, "in"));

        let result = simulate(&project, &HashMap::from([("IN1".into(), LogicState::High)]));
        assert_eq!(output(&result), LogicState::Floating);
        assert!(result
            .transistors
            .iter()
            .all(|transistor| transistor.state == super::SwitchState::Off));
    }

    #[test]
    fn series_paths_report_more_resistance_than_parallel_paths() {
        fn pull_down(parallel: bool) -> super::SimulationResult {
            let mut project = Project::default();
            let gnd = project.add_component("gnd", 0.0, 4.0).unwrap();
            let input = project.add_component("input", -4.0, 0.0).unwrap();
            let output_probe = project.add_component("output", 4.0, 0.0).unwrap();
            let first = project.add_component("nmos", 0.0, 1.0).unwrap();
            let second = project.add_component("nmos", 0.0, 3.0).unwrap();
            connect(&mut project, (input, "out"), (first, "gate"));
            connect(&mut project, (input, "out"), (second, "gate"));
            connect(&mut project, (first, "drain"), (output_probe, "in"));
            if parallel {
                connect(&mut project, (second, "drain"), (output_probe, "in"));
                connect(&mut project, (first, "source"), (gnd, "out"));
                connect(&mut project, (second, "source"), (gnd, "out"));
            } else {
                connect(&mut project, (first, "source"), (second, "drain"));
                connect(&mut project, (second, "source"), (gnd, "out"));
            }
            simulate(&project, &HashMap::from([("IN1".into(), LogicState::High)]))
        }

        let series = pull_down(false).outputs[0]
            .low_drive_resistance_ohms
            .unwrap();
        let parallel = pull_down(true).outputs[0]
            .low_drive_resistance_ohms
            .unwrap();
        assert!(series > parallel);
        assert_eq!(series, 24_000.0);
        assert_eq!(parallel, 12_000.0);
    }

    #[test]
    fn geometry_changes_reported_drive_resistance() {
        let mut project = Project::default();
        let gnd = project.add_component("gnd", 0.0, 4.0).unwrap();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        let output_probe = project.add_component("output", 4.0, 0.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 1.0).unwrap();
        project.set_device_geometry(nmos, 2.0, 1.0).unwrap();
        connect(&mut project, (input, "out"), (nmos, "gate"));
        connect(&mut project, (nmos, "drain"), (output_probe, "in"));
        connect(&mut project, (nmos, "source"), (gnd, "out"));

        let result = simulate(&project, &HashMap::from([("IN1".into(), LogicState::High)]));
        assert_eq!(result.outputs[0].low_drive_resistance_ohms, Some(6_000.0));
        assert_eq!(result.transistors[0].effective_on_resistance_ohms, 6_000.0);
    }

    fn inverter_with_fanout(fanout: usize, pmos_width: f64) -> Project {
        let mut project = Project::default();
        let vdd = project.add_component("vdd", 0.0, -4.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 4.0).unwrap();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        let output_probe = project.add_component("output", 4.0, 0.0).unwrap();
        let pmos = project.add_component("pmos", 0.0, -1.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 1.0).unwrap();
        project.set_device_geometry(pmos, pmos_width, 1.0).unwrap();
        connect(&mut project, (vdd, "out"), (pmos, "source"));
        connect(&mut project, (gnd, "out"), (nmos, "source"));
        connect(&mut project, (input, "out"), (pmos, "gate"));
        connect(&mut project, (input, "out"), (nmos, "gate"));
        connect(&mut project, (pmos, "drain"), (nmos, "drain"));
        connect(&mut project, (pmos, "drain"), (output_probe, "in"));
        for index in 0..fanout {
            let load = project
                .add_component("nmos", 6.0, index as f64 * 2.0)
                .unwrap();
            connect(&mut project, (pmos, "drain"), (load, "gate"));
        }
        project
    }

    #[test]
    fn fanout_increases_delay_and_stronger_drive_reduces_it() {
        let input = HashMap::from([("IN1".into(), LogicState::Low)]);
        let unloaded = simulate(&inverter_with_fanout(0, 1.0), &input).outputs[0]
            .estimated_delay_ns
            .unwrap();
        let loaded = simulate(&inverter_with_fanout(4, 1.0), &input).outputs[0]
            .estimated_delay_ns
            .unwrap();
        let stronger = simulate(&inverter_with_fanout(4, 2.0), &input).outputs[0]
            .estimated_delay_ns
            .unwrap();
        assert!(loaded > unloaded);
        assert!(stronger < loaded);
    }

    #[test]
    fn waveform_edges_shift_by_the_estimated_delay() {
        let project = inverter_with_fanout(4, 1.0);
        let static_low = simulate(&project, &HashMap::from([("IN1".into(), LogicState::High)]));
        let expected_delay = static_low.outputs[0].estimated_delay_ns.unwrap();
        let result = waveform(
            &project,
            WaveformConfig {
                duration_ns: 40,
                clock_period_ns: 10,
                input_change_ns: 10,
            },
        )
        .unwrap();
        let output = result
            .signals
            .iter()
            .find(|signal| signal.kind == "output")
            .unwrap();
        assert_eq!(output.samples[0].state, LogicState::High);
        assert_eq!(output.samples[1].state, LogicState::Low);
        assert!((output.samples[1].time_ns - (10.0 + expected_delay)).abs() < 1e-9);
    }

    #[test]
    fn cmos_nand_truth_table() {
        let mut project = Project::default();
        let vdd = project.add_component("vdd", 0.0, -5.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 5.0).unwrap();
        let input_a = project.add_component("input", -5.0, -1.0).unwrap();
        let input_b = project.add_component("input", -5.0, 1.0).unwrap();
        let output_probe = project.add_component("output", 5.0, 0.0).unwrap();
        let pmos_a = project.add_component("pmos", -1.0, -2.0).unwrap();
        let pmos_b = project.add_component("pmos", 1.0, -2.0).unwrap();
        let nmos_a = project.add_component("nmos", 0.0, 1.0).unwrap();
        let nmos_b = project.add_component("nmos", 0.0, 3.0).unwrap();
        connect(&mut project, (vdd, "out"), (pmos_a, "source"));
        connect(&mut project, (vdd, "out"), (pmos_b, "source"));
        connect(&mut project, (pmos_a, "drain"), (output_probe, "in"));
        connect(&mut project, (pmos_b, "drain"), (output_probe, "in"));
        connect(&mut project, (nmos_a, "drain"), (output_probe, "in"));
        connect(&mut project, (nmos_a, "source"), (nmos_b, "drain"));
        connect(&mut project, (nmos_b, "source"), (gnd, "out"));
        connect(&mut project, (input_a, "out"), (pmos_a, "gate"));
        connect(&mut project, (input_a, "out"), (nmos_a, "gate"));
        connect(&mut project, (input_b, "out"), (pmos_b, "gate"));
        connect(&mut project, (input_b, "out"), (nmos_b, "gate"));

        for (a, b, expected) in [
            (LogicState::Low, LogicState::Low, LogicState::High),
            (LogicState::Low, LogicState::High, LogicState::High),
            (LogicState::High, LogicState::Low, LogicState::High),
            (LogicState::High, LogicState::High, LogicState::Low),
        ] {
            let result = simulate(
                &project,
                &HashMap::from([("IN1".into(), a), ("IN2".into(), b)]),
            );
            assert!(result.converged);
            assert_eq!(output(&result), expected);
        }
    }

    #[test]
    fn reports_floating_and_contended_outputs() {
        let mut project = Project::default();
        let vdd = project.add_component("vdd", 0.0, -2.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 2.0).unwrap();
        let contended = project.add_component("output", 3.0, 0.0).unwrap();
        project.add_component("output", 5.0, 0.0).unwrap();
        connect(&mut project, (vdd, "out"), (contended, "in"));
        connect(&mut project, (gnd, "out"), (contended, "in"));

        let result = simulate(&project, &HashMap::new());
        assert!(result.converged);
        assert_eq!(result.outputs[0].state, LogicState::Contended);
        assert_eq!(result.outputs[1].state, LogicState::Floating);
    }

    #[test]
    fn cmos_nor_truth_table() {
        let mut project = Project::default();
        let vdd = project.add_component("vdd", 0.0, -5.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 5.0).unwrap();
        let input_a = project.add_component("input", -5.0, -1.0).unwrap();
        let input_b = project.add_component("input", -5.0, 1.0).unwrap();
        let output_probe = project.add_component("output", 5.0, 0.0).unwrap();
        let pmos_a = project.add_component("pmos", 0.0, -3.0).unwrap();
        let pmos_b = project.add_component("pmos", 0.0, -1.0).unwrap();
        let nmos_a = project.add_component("nmos", -1.0, 2.0).unwrap();
        let nmos_b = project.add_component("nmos", 1.0, 2.0).unwrap();
        connect(&mut project, (vdd, "out"), (pmos_a, "source"));
        connect(&mut project, (pmos_a, "drain"), (pmos_b, "source"));
        connect(&mut project, (pmos_b, "drain"), (output_probe, "in"));
        connect(&mut project, (nmos_a, "drain"), (output_probe, "in"));
        connect(&mut project, (nmos_b, "drain"), (output_probe, "in"));
        connect(&mut project, (nmos_a, "source"), (gnd, "out"));
        connect(&mut project, (nmos_b, "source"), (gnd, "out"));
        connect(&mut project, (input_a, "out"), (pmos_a, "gate"));
        connect(&mut project, (input_a, "out"), (nmos_a, "gate"));
        connect(&mut project, (input_b, "out"), (pmos_b, "gate"));
        connect(&mut project, (input_b, "out"), (nmos_b, "gate"));

        let table = truth_table(&project).unwrap();
        assert_eq!(table.rows.len(), 4);
        assert_eq!(
            table
                .rows
                .iter()
                .map(|row| row.outputs[0])
                .collect::<Vec<_>>(),
            vec![
                LogicState::High,
                LogicState::Low,
                LogicState::Low,
                LogicState::Low
            ]
        );
    }

    #[test]
    fn transmission_gate_passes_data_or_floats() {
        let mut project = Project::default();
        let data = project.add_component("input", -5.0, 0.0).unwrap();
        let enable = project.add_component("input", -5.0, 2.0).unwrap();
        let enable_bar = project.add_component("input", -5.0, 4.0).unwrap();
        let output_probe = project.add_component("output", 5.0, 0.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, -1.0).unwrap();
        let pmos = project.add_component("pmos", 0.0, 1.0).unwrap();
        connect(&mut project, (data, "out"), (nmos, "source"));
        connect(&mut project, (data, "out"), (pmos, "source"));
        connect(&mut project, (nmos, "drain"), (output_probe, "in"));
        connect(&mut project, (pmos, "drain"), (output_probe, "in"));
        connect(&mut project, (enable, "out"), (nmos, "gate"));
        connect(&mut project, (enable_bar, "out"), (pmos, "gate"));

        for (data_state, expected) in [
            (LogicState::Low, LogicState::Low),
            (LogicState::High, LogicState::High),
        ] {
            let result = simulate(
                &project,
                &HashMap::from([
                    ("IN1".into(), data_state),
                    ("IN2".into(), LogicState::High),
                    ("IN3".into(), LogicState::Low),
                ]),
            );
            assert_eq!(output(&result), expected);
        }
        let off = simulate(
            &project,
            &HashMap::from([
                ("IN1".into(), LogicState::High),
                ("IN2".into(), LogicState::Low),
                ("IN3".into(), LogicState::High),
            ]),
        );
        assert_eq!(output(&off), LogicState::Floating);
    }

    #[test]
    fn waveform_samples_inputs_and_outputs_over_time() {
        let mut project = Project::default();
        let input = project.add_component("input", -2.0, 0.0).unwrap();
        let output_probe = project.add_component("output", 2.0, 0.0).unwrap();
        connect(&mut project, (input, "out"), (output_probe, "in"));
        let result = waveform(
            &project,
            WaveformConfig {
                duration_ns: 40,
                clock_period_ns: 10,
                input_change_ns: 10,
            },
        )
        .unwrap();
        assert_eq!(result.signals.len(), 2);
        assert_eq!(result.signals[0].kind, "input");
        assert_eq!(result.signals[1].kind, "output");
        assert_eq!(
            result.signals[1]
                .samples
                .iter()
                .map(|sample| sample.state)
                .collect::<Vec<_>>(),
            vec![
                LogicState::Low,
                LogicState::High,
                LogicState::Low,
                LogicState::High,
                LogicState::Low,
            ]
        );
    }
}
