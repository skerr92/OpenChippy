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
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransistorState {
    pub component_id: Uuid,
    pub name: String,
    pub state: SwitchState,
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
    pub time_ns: u32,
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
            .map(|transistor| match states[transistor.gate] {
                LogicState::High if !transistor.pmos => SwitchState::On,
                LogicState::Low if transistor.pmos => SwitchState::On,
                LogicState::High | LogicState::Low => SwitchState::Off,
                LogicState::Floating | LogicState::Unknown | LogicState::Contended => {
                    SwitchState::Unknown
                }
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

    let mut nets = (0..net_count)
        .map(|net| NamedState {
            name: net_names[net]
                .first()
                .cloned()
                .unwrap_or_else(|| format!("net-{}", net + 1)),
            state: states[net],
        })
        .collect::<Vec<_>>();
    nets.sort_by(|left, right| left.name.cmp(&right.name));

    let mut outputs = project
        .components
        .iter()
        .filter(|component| component.kind == "output")
        .map(|component| NamedState {
            name: component.name.clone(),
            state: states[terminal_nets[&(component.id, "in".into())]],
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
                        time_ns: time,
                        state,
                    });
                (name.clone(), state)
            })
            .collect::<HashMap<_, _>>();
        let result = simulate(project, &inputs);
        for output in result.outputs {
            signal_samples
                .get_mut(&output.name)
                .expect("output signal exists")
                .push(WaveformSample {
                    time_ns: time,
                    state: output.state,
                });
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
        signals.push(WaveformSignal {
            name: name.clone(),
            kind: "output",
            samples: signal_samples.remove(name).unwrap_or_default(),
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
                LogicState::Low,
                LogicState::High,
                LogicState::High,
                LogicState::Low,
                LogicState::Low,
                LogicState::High,
                LogicState::High,
                LogicState::Low
            ]
        );
    }
}
