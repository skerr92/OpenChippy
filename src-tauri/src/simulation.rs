use crate::model::{Component, Project, TerminalRef};
use crate::rtl::{PrimitiveGate, RtlEdge, RtlModule, RtlPortDirection, RtlRange};
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
    pub fanout: FanoutReport,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanoutEndpoint {
    pub component_id: Option<Uuid>,
    pub name: String,
    pub kind: String,
    pub terminal: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanoutNet {
    pub name: String,
    pub aliases: Vec<String>,
    pub drivers: Vec<FanoutEndpoint>,
    pub loads: Vec<FanoutEndpoint>,
    pub fanout: usize,
    pub high_fanout: bool,
    pub undriven: bool,
    pub multiply_driven: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanoutGroup {
    pub name: String,
    pub signals: Vec<String>,
    pub total_fanout: usize,
    pub max_fanout: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FanoutReport {
    pub high_fanout_threshold: usize,
    pub nets: Vec<FanoutNet>,
    pub groups: Vec<FanoutGroup>,
    pub high_fanout_count: usize,
    pub undriven_count: usize,
    pub multiply_driven_count: usize,
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

fn analyze_fanout(
    project: &Project,
    terminal_nets: &HashMap<TerminalKey, usize>,
    net_names: &[Vec<String>],
) -> FanoutReport {
    let high_fanout_threshold = project.high_fanout_warning_threshold;
    let mut drivers = (0..net_names.len()).map(|_| Vec::new()).collect::<Vec<_>>();
    let mut loads = (0..net_names.len()).map(|_| Vec::new()).collect::<Vec<_>>();
    let mut has_channel_driver = vec![false; net_names.len()];
    for component in &project.components {
        match component.kind.as_str() {
            "input" | "vdd" | "gnd" => {
                let net = terminal_nets[&(component.id, "out".into())];
                drivers[net].push(FanoutEndpoint {
                    component_id: Some(component.id),
                    name: component.name.clone(),
                    kind: component.kind.clone(),
                    terminal: "out".into(),
                });
            }
            "nmos" | "pmos" => {
                let gate = terminal_nets[&(component.id, "gate".into())];
                loads[gate].push(FanoutEndpoint {
                    component_id: Some(component.id),
                    name: component.name.clone(),
                    kind: component.kind.clone(),
                    terminal: "gate".into(),
                });
                for terminal in ["drain", "source"] {
                    let net = terminal_nets[&(component.id, terminal.into())];
                    has_channel_driver[net] = true;
                }
            }
            "output" => {
                let net = terminal_nets[&(component.id, "in".into())];
                loads[net].push(FanoutEndpoint {
                    component_id: Some(component.id),
                    name: component.name.clone(),
                    kind: component.kind.clone(),
                    terminal: "in".into(),
                });
            }
            _ => {}
        }
    }
    for (net, has_driver) in has_channel_driver.into_iter().enumerate() {
        if has_driver
            && !net_names[net].iter().any(|name| {
                project.components.iter().any(|component| {
                    component.name == *name && matches!(component.kind.as_str(), "vdd" | "gnd")
                })
            })
        {
            drivers[net].push(FanoutEndpoint {
                component_id: None,
                name: "CMOS channel network".into(),
                kind: "cmos".into(),
                terminal: "drain/source".into(),
            });
        }
    }
    let mut nets = (0..net_names.len())
        .map(|net| {
            let mut aliases = net_names[net].clone();
            aliases.sort();
            aliases.dedup();
            let name = aliases
                .first()
                .cloned()
                .unwrap_or_else(|| format!("net-{}", net + 1));
            drivers[net].sort_by(|left, right| {
                (&left.name, &left.terminal).cmp(&(&right.name, &right.terminal))
            });
            loads[net].sort_by(|left, right| {
                (&left.name, &left.terminal).cmp(&(&right.name, &right.terminal))
            });
            let fanout = loads[net].len();
            FanoutNet {
                name,
                aliases,
                high_fanout: fanout > high_fanout_threshold,
                undriven: drivers[net].is_empty() && fanout > 0,
                multiply_driven: drivers[net].len() > 1,
                fanout,
                drivers: std::mem::take(&mut drivers[net]),
                loads: std::mem::take(&mut loads[net]),
            }
        })
        .collect::<Vec<_>>();
    nets.sort_by(|left, right| left.name.cmp(&right.name));
    let groups = project
        .waveform_groups
        .iter()
        .map(|group| {
            let members = nets
                .iter()
                .filter(|net| {
                    group.signals.iter().any(|signal| {
                        net.name == *signal || net.aliases.iter().any(|alias| alias == signal)
                    })
                })
                .collect::<Vec<_>>();
            FanoutGroup {
                name: group.name.clone(),
                signals: group.signals.clone(),
                total_fanout: members.iter().map(|net| net.fanout).sum(),
                max_fanout: members.iter().map(|net| net.fanout).max().unwrap_or(0),
            }
        })
        .collect::<Vec<_>>();
    FanoutReport {
        high_fanout_threshold,
        high_fanout_count: nets.iter().filter(|net| net.high_fanout).count(),
        undriven_count: nets.iter().filter(|net| net.undriven).count(),
        multiply_driven_count: nets.iter().filter(|net| net.multiply_driven).count(),
        nets,
        groups,
    }
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

    let fanout = analyze_fanout(project, &terminal_nets, &net_names);
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
        fanout,
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

fn rtl_bits(name: &str, range: &Option<RtlRange>) -> Vec<String> {
    range.as_ref().map_or_else(
        || vec![name.to_string()],
        |range| {
            if range.msb >= range.lsb {
                (range.lsb..=range.msb)
                    .rev()
                    .map(|index| format!("{name}[{index}]"))
                    .collect()
            } else {
                (range.msb..=range.lsb)
                    .map(|index| format!("{name}[{index}]"))
                    .collect()
            }
        },
    )
}

#[derive(Clone, Copy)]
struct RtlValue {
    bits: u128,
    width: usize,
    known: bool,
}

fn rtl_signal_value(
    module: &RtlModule,
    states: &HashMap<String, LogicState>,
    name: &str,
) -> RtlValue {
    if name == "1'b0" || name == "0" {
        return RtlValue {
            bits: 0,
            width: 1,
            known: true,
        };
    }
    if name == "1'b1" || name == "1" {
        return RtlValue {
            bits: 1,
            width: 1,
            known: true,
        };
    }
    let range = module
        .ports
        .iter()
        .find(|port| port.name == name)
        .and_then(|port| port.range.clone())
        .or_else(|| {
            module
                .nets
                .iter()
                .find(|net| net.name == name)
                .and_then(|net| net.range.clone())
        });
    let names = rtl_bits(name, &range);
    if names.len() > 128 {
        return RtlValue {
            bits: 0,
            width: names.len(),
            known: false,
        };
    }
    let mut bits = 0;
    for (position, bit_name) in names.iter().rev().enumerate() {
        match states.get(bit_name).copied().unwrap_or(LogicState::Unknown) {
            LogicState::High => bits |= 1u128 << position,
            LogicState::Low => {}
            _ => {
                return RtlValue {
                    bits: 0,
                    width: names.len(),
                    known: false,
                }
            }
        }
    }
    RtlValue {
        bits,
        width: names.len(),
        known: true,
    }
}

fn rtl_expression(
    module: &RtlModule,
    states: &HashMap<String, LogicState>,
    source: &str,
) -> RtlValue {
    fn outer_parens(value: &str) -> bool {
        if !value.starts_with('(') || !value.ends_with(')') {
            return false;
        }
        let mut depth = 0i32;
        for (index, character) in value.char_indices() {
            if character == '(' {
                depth += 1;
            }
            if character == ')' {
                depth -= 1;
            }
            if depth == 0 && index + character.len_utf8() < value.len() {
                return false;
            }
        }
        depth == 0
    }
    let mut source = source.trim();
    while outer_parens(source) {
        source = &source[1..source.len() - 1];
    }
    if source.starts_with('{') && source.ends_with('}') {
        let content = &source[1..source.len() - 1];
        let mut brace_depth = 0i32;
        let mut paren_depth = 0i32;
        let mut replication = None;
        for (index, character) in content.char_indices() {
            if character == '(' {
                paren_depth += 1;
            }
            if character == ')' {
                paren_depth -= 1;
            }
            if character == '{' {
                if brace_depth == 0 && paren_depth == 0 && index > 0 {
                    replication = Some(index);
                    break;
                }
                brace_depth += 1;
            }
            if character == '}' {
                brace_depth -= 1;
            }
        }
        if let Some(index) = replication {
            let count = rtl_expression(module, states, &content[..index]);
            let value = rtl_expression(module, states, &content[index..]);
            let count = if count.known {
                usize::try_from(count.bits).unwrap_or(usize::MAX)
            } else {
                0
            };
            let width = value.width.saturating_mul(count);
            if count == 0 || width > 128 || !value.known {
                return RtlValue {
                    bits: 0,
                    width: width.max(1),
                    known: false,
                };
            }
            let mut bits = 0u128;
            for _ in 0..count {
                bits = (bits << value.width) | value.bits;
            }
            return RtlValue {
                bits,
                width,
                known: true,
            };
        }
        let mut pieces = Vec::new();
        let mut start = 0;
        brace_depth = 0;
        paren_depth = 0;
        for (index, character) in content.char_indices() {
            if character == '(' {
                paren_depth += 1;
            }
            if character == ')' {
                paren_depth -= 1;
            }
            if character == '{' {
                brace_depth += 1;
            }
            if character == '}' {
                brace_depth -= 1;
            }
            if character == ',' && brace_depth == 0 && paren_depth == 0 {
                pieces.push(&content[start..index]);
                start = index + 1;
            }
        }
        pieces.push(&content[start..]);
        let values = pieces
            .into_iter()
            .map(|piece| rtl_expression(module, states, piece))
            .collect::<Vec<_>>();
        let width = values.iter().map(|value| value.width).sum::<usize>();
        if values.is_empty() || width > 128 || values.iter().any(|value| !value.known) {
            return RtlValue {
                bits: 0,
                width: width.max(1),
                known: false,
            };
        }
        let bits = values
            .iter()
            .fold(0u128, |bits, value| (bits << value.width) | value.bits);
        return RtlValue {
            bits,
            width,
            known: true,
        };
    }
    let mut paren_depth = 0i32;
    let mut question = None;
    let mut ternary_depth = 0usize;
    for (index, character) in source.char_indices() {
        if character == '(' {
            paren_depth += 1;
        }
        if character == ')' {
            paren_depth -= 1;
        }
        if paren_depth != 0 {
            continue;
        }
        if character == '?' {
            if question.is_none() {
                question = Some(index);
            }
            ternary_depth += 1;
        } else if character == ':' && ternary_depth > 0 {
            ternary_depth -= 1;
            if ternary_depth == 0 {
                let condition = rtl_expression(module, states, &source[..question.unwrap()]);
                let when_true =
                    rtl_expression(module, states, &source[question.unwrap() + 1..index]);
                let when_false = rtl_expression(module, states, &source[index + 1..]);
                if !condition.known {
                    return RtlValue {
                        bits: 0,
                        width: when_true.width.max(when_false.width),
                        known: false,
                    };
                }
                return if condition.bits != 0 {
                    when_true
                } else {
                    when_false
                };
            }
        }
    }
    let mut equality_depth = 0i32;
    for (index, character) in source.char_indices() {
        if character == '(' {
            equality_depth += 1;
        }
        if character == ')' {
            equality_depth -= 1;
        }
        if equality_depth == 0 && source[index..].starts_with("==") {
            let left = rtl_expression(module, states, &source[..index]);
            let right = rtl_expression(module, states, &source[index + 2..]);
            if !left.known || !right.known {
                return RtlValue {
                    bits: 0,
                    width: 1,
                    known: false,
                };
            }
            return RtlValue {
                bits: u128::from(left.bits == right.bits),
                width: 1,
                known: true,
            };
        }
    }
    let mut depth = 0i32;
    for operators in [["|", "^", "&"], ["+", "-", ""]] {
        for (index, character) in source.char_indices().rev() {
            if character == ')' {
                depth += 1;
            }
            if character == '(' {
                depth -= 1;
            }
            if depth == 0 && operators.contains(&&source[index..index + character.len_utf8()]) {
                let left = rtl_expression(module, states, &source[..index]);
                let right = rtl_expression(module, states, &source[index + 1..]);
                let width = left.width.max(right.width) + usize::from(character == '+');
                if !left.known || !right.known || width > 128 {
                    return RtlValue {
                        bits: 0,
                        width,
                        known: false,
                    };
                }
                let mask = if width == 128 {
                    u128::MAX
                } else {
                    (1u128 << width) - 1
                };
                let bits = match character {
                    '|' => left.bits | right.bits,
                    '^' => left.bits ^ right.bits,
                    '&' => left.bits & right.bits,
                    '+' => left.bits.wrapping_add(right.bits),
                    '-' => left.bits.wrapping_sub(right.bits),
                    _ => 0,
                } & mask;
                return RtlValue {
                    bits,
                    width,
                    known: true,
                };
            }
        }
    }
    if let Some(rest) = source.strip_prefix('~') {
        let value = rtl_expression(module, states, rest);
        let mask = if value.width >= 128 {
            u128::MAX
        } else {
            (1u128 << value.width) - 1
        };
        return RtlValue {
            bits: (!value.bits) & mask,
            ..value
        };
    }
    if let Some((width, bits)) = source.split_once("'b") {
        let width = width.parse::<usize>().unwrap_or(0);
        if width == 0 || width > 128 || bits.chars().any(|bit| !matches!(bit, '0' | '1')) {
            return RtlValue {
                bits: 0,
                width: width.max(1),
                known: false,
            };
        }
        return RtlValue {
            bits: u128::from_str_radix(bits, 2).unwrap_or(0),
            width,
            known: true,
        };
    }
    if let Ok(value) = source.parse::<u128>() {
        return RtlValue {
            bits: value,
            width: (128 - value.leading_zeros() as usize).max(1),
            known: true,
        };
    }
    if let Some(parameter) = module
        .parameters
        .iter()
        .find(|parameter| parameter.name == source)
    {
        return RtlValue {
            bits: parameter.default_value.max(0) as u128,
            width: 64,
            known: parameter.default_value >= 0,
        };
    }
    rtl_signal_value(module, states, source)
}

fn assign_rtl_value(
    module: &RtlModule,
    states: &mut HashMap<String, LogicState>,
    target: &str,
    value: RtlValue,
) -> bool {
    let range = module
        .ports
        .iter()
        .find(|port| port.name == target)
        .and_then(|port| port.range.clone())
        .or_else(|| {
            module
                .nets
                .iter()
                .find(|net| net.name == target)
                .and_then(|net| net.range.clone())
        });
    let names = rtl_bits(target, &range);
    let count = names.len();
    let mut changed = false;
    for (position, name) in names.into_iter().rev().enumerate() {
        let state = if !value.known || position >= value.width {
            LogicState::Unknown
        } else if value.bits & (1u128 << position) == 0 {
            LogicState::Low
        } else {
            LogicState::High
        };
        changed |= states.insert(name, state) != Some(state);
    }
    changed || count == 0
}

fn simulate_rtl_point(
    module: &RtlModule,
    inputs: &HashMap<String, LogicState>,
) -> HashMap<String, LogicState> {
    let mut states = inputs.clone();
    for _ in 0..64 {
        let mut changed = false;
        for instance in &module.instances {
            let Some(primitive) = instance.primitive else {
                continue;
            };
            let values = instance
                .connections
                .iter()
                .skip(1)
                .map(|name| rtl_signal_value(module, &states, name))
                .collect::<Vec<_>>();
            let known = values.iter().all(|value| value.known);
            let bit = match primitive {
                PrimitiveGate::Not => {
                    known.then(|| values.first().is_some_and(|value| value.bits & 1 == 0))
                }
                PrimitiveGate::Buf => {
                    known.then(|| values.first().is_some_and(|value| value.bits & 1 != 0))
                }
                PrimitiveGate::And | PrimitiveGate::Nand => known.then(|| {
                    values.iter().all(|value| value.bits & 1 != 0)
                        ^ matches!(primitive, PrimitiveGate::Nand)
                }),
                PrimitiveGate::Or | PrimitiveGate::Nor => known.then(|| {
                    values.iter().any(|value| value.bits & 1 != 0)
                        ^ matches!(primitive, PrimitiveGate::Nor)
                }),
                PrimitiveGate::Xor | PrimitiveGate::Xnor => known.then(|| {
                    (values.iter().filter(|value| value.bits & 1 != 0).count() % 2 == 1)
                        ^ matches!(primitive, PrimitiveGate::Xnor)
                }),
            };
            if let Some(target) = instance.connections.first() {
                changed |= assign_rtl_value(
                    module,
                    &mut states,
                    target,
                    bit.map_or(
                        RtlValue {
                            bits: 0,
                            width: 1,
                            known: false,
                        },
                        |bit| RtlValue {
                            bits: u128::from(bit),
                            width: 1,
                            known: true,
                        },
                    ),
                );
            }
        }
        for assignment in &module.assignments {
            let value = rtl_expression(module, &states, &assignment.expression);
            changed |= assign_rtl_value(module, &mut states, &assignment.target, value);
        }
        if !changed {
            break;
        }
    }
    states
}

pub fn rtl_truth_table(module: &RtlModule) -> Result<TruthTableResult, String> {
    if !module.sequential_processes.is_empty() {
        return Err(
            "truth tables require combinational RTL; use Waveforms for clocked processes".into(),
        );
    }
    let input_names = module
        .ports
        .iter()
        .filter(|port| port.direction == RtlPortDirection::Input)
        .flat_map(|port| rtl_bits(&port.name, &port.range))
        .collect::<Vec<_>>();
    if input_names.len() > 8 {
        return Err("truth tables are limited to 8 digital input bits".into());
    }
    let output_names = module
        .ports
        .iter()
        .filter(|port| port.direction == RtlPortDirection::Output)
        .flat_map(|port| rtl_bits(&port.name, &port.range))
        .collect::<Vec<_>>();
    let rows = (0..(1usize << input_names.len()))
        .map(|combination| {
            let inputs = input_names
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
            let states = simulate_rtl_point(module, &inputs);
            let outputs = output_names
                .iter()
                .map(|name| states.get(name).copied().unwrap_or(LogicState::Unknown))
                .collect::<Vec<_>>();
            TruthTableRow {
                inputs: input_names.iter().map(|name| inputs[name]).collect(),
                converged: outputs.iter().all(|state| *state != LogicState::Unknown),
                outputs,
            }
        })
        .collect();
    Ok(TruthTableResult {
        input_names,
        output_names,
        rows,
    })
}

pub fn rtl_waveform(module: &RtlModule, config: WaveformConfig) -> Result<WaveformResult, String> {
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
    let input_names = module
        .ports
        .iter()
        .filter(|port| port.direction == RtlPortDirection::Input)
        .flat_map(|port| rtl_bits(&port.name, &port.range))
        .collect::<Vec<_>>();
    let output_names = module
        .ports
        .iter()
        .filter(|port| port.direction == RtlPortDirection::Output)
        .flat_map(|port| rtl_bits(&port.name, &port.range))
        .collect::<Vec<_>>();
    let is_clock = |name: &str| {
        let name = name.to_ascii_uppercase();
        name == "CLK" || name.starts_with("CLK[") || name.contains("CLOCK")
    };
    let ordinary = input_names
        .iter()
        .filter(|name| !is_clock(name))
        .collect::<Vec<_>>();
    let half_clock = config.clock_period_ns / 2;
    let mut times = BTreeSet::from([0, config.duration_ns]);
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
    let mut samples = input_names
        .iter()
        .chain(&output_names)
        .map(|name| (name.clone(), Vec::new()))
        .collect::<HashMap<_, _>>();
    let mut registers = HashMap::new();
    for process in &module.sequential_processes {
        assign_rtl_value(
            module,
            &mut registers,
            &process.target,
            RtlValue {
                bits: 0,
                width: rtl_bits(
                    &process.target,
                    &module
                        .ports
                        .iter()
                        .find(|port| port.name == process.target)
                        .and_then(|port| port.range.clone()),
                )
                .len(),
                known: false,
            },
        );
    }
    let mut previous_inputs = HashMap::new();
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
                    let index = ordinary
                        .iter()
                        .position(|candidate| *candidate == name)
                        .unwrap_or(0);
                    let shift = ordinary.len().saturating_sub(index + 1);
                    if shift < 32 && (step >> shift) & 1 != 0 {
                        LogicState::High
                    } else {
                        LogicState::Low
                    }
                };
                samples.get_mut(name).unwrap().push(WaveformSample {
                    time_ns: f64::from(time),
                    state,
                });
                (name.clone(), state)
            })
            .collect::<HashMap<_, _>>();
        let mut environment = registers.clone();
        environment.extend(inputs.clone());
        let before_edge = simulate_rtl_point(module, &environment);
        let updates = module
            .sequential_processes
            .iter()
            .filter_map(|process| {
                let previous = previous_inputs
                    .get(&process.clock)
                    .copied()
                    .unwrap_or(LogicState::Unknown);
                let current = inputs
                    .get(&process.clock)
                    .copied()
                    .unwrap_or(LogicState::Unknown);
                let triggered = match process.edge {
                    RtlEdge::Posedge => previous == LogicState::Low && current == LogicState::High,
                    RtlEdge::Negedge => previous == LogicState::High && current == LogicState::Low,
                };
                triggered.then(|| {
                    (
                        process.target.clone(),
                        rtl_expression(module, &before_edge, &process.expression),
                    )
                })
            })
            .collect::<Vec<_>>();
        for (target, value) in updates {
            assign_rtl_value(module, &mut registers, &target, value);
        }
        let mut environment = registers.clone();
        environment.extend(inputs.clone());
        let result = simulate_rtl_point(module, &environment);
        previous_inputs = inputs;
        for name in &output_names {
            let state = result.get(name).copied().unwrap_or(LogicState::Unknown);
            let lane = samples.get_mut(name).unwrap();
            if lane.last().is_none_or(|sample| sample.state != state) {
                lane.push(WaveformSample {
                    time_ns: f64::from(time),
                    state,
                });
            }
        }
    }
    let mut signals = Vec::new();
    for name in &input_names {
        signals.push(WaveformSignal {
            name: name.clone(),
            kind: "input",
            samples: samples.remove(name).unwrap_or_default(),
        });
    }
    for name in &output_names {
        signals.push(WaveformSignal {
            name: name.clone(),
            kind: "output",
            samples: samples.remove(name).unwrap_or_default(),
        });
    }
    Ok(WaveformResult {
        duration_ns: config.duration_ns,
        signals,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        rtl_truth_table, rtl_waveform, simulate, truth_table, waveform, LogicState, WaveformConfig,
    };
    use crate::model::{Project, TerminalRef, WaveformGroup, WaveformRadix};
    use crate::rtl::parse_structural_verilog;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn terminal(component_id: Uuid, terminal: &str) -> TerminalRef {
        TerminalRef {
            component_id,
            terminal: terminal.into(),
        }
    }

    #[test]
    fn imported_vector_arithmetic_drives_waveform_outputs() {
        let module = parse_structural_verilog(
            "module adder(input wire [3:0] a, input wire [3:0] b, output wire [4:0] sum); assign sum = a + b; endmodule",
        ).unwrap();
        let result = rtl_waveform(
            &module,
            WaveformConfig {
                duration_ns: 40,
                clock_period_ns: 10,
                input_change_ns: 20,
            },
        )
        .unwrap();
        assert_eq!(result.signals.len(), 13);
        let sum0 = result
            .signals
            .iter()
            .find(|signal| signal.name == "sum[0]")
            .unwrap();
        assert_eq!(sum0.samples[0].state, LogicState::Low);
        assert_eq!(sum0.samples[1].time_ns, 20.0);
        assert_eq!(sum0.samples[1].state, LogicState::High);
    }

    #[test]
    fn imported_combinational_if_else_drives_mux_waveforms() {
        let module = parse_structural_verilog(
            "module mux(input S, input A, input B, output logic Y); always_comb if (S) Y = A; else Y = B; endmodule",
        ).unwrap();
        let result = rtl_waveform(
            &module,
            WaveformConfig {
                duration_ns: 120,
                clock_period_ns: 10,
                input_change_ns: 20,
            },
        )
        .unwrap();
        let output = result
            .signals
            .iter()
            .find(|signal| signal.name == "Y")
            .unwrap();
        assert!(output
            .samples
            .iter()
            .any(|sample| sample.state == LogicState::High));
        assert!(output
            .samples
            .iter()
            .any(|sample| sample.state == LogicState::Low));
        assert!(output
            .samples
            .iter()
            .all(|sample| sample.state != LogicState::Unknown));
    }

    #[test]
    fn imported_vector_logic_generates_truth_tables() {
        let module = parse_structural_verilog(
            "module add2(input [1:0] A, input [1:0] B, output [2:0] Y); assign Y = A + B; endmodule",
        ).unwrap();
        let table = rtl_truth_table(&module).unwrap();
        assert_eq!(table.input_names, ["A[1]", "A[0]", "B[1]", "B[0]"]);
        assert_eq!(table.output_names, ["Y[2]", "Y[1]", "Y[0]"]);
        assert_eq!(table.rows.len(), 16);
        assert_eq!(
            table.rows[15].outputs,
            [LogicState::High, LogicState::High, LogicState::Low]
        );
        assert!(table.rows.iter().all(|row| row.converged));
    }

    #[test]
    fn nonblocking_register_updates_on_clock_edges_in_waveforms() {
        let module = parse_structural_verilog(
            "module dff(input logic CLK, input logic D, output logic Q); always_ff @(posedge CLK) Q <= D; endmodule",
        ).unwrap();
        assert!(rtl_truth_table(&module).unwrap_err().contains("Waveforms"));
        let result = rtl_waveform(
            &module,
            WaveformConfig {
                duration_ns: 40,
                clock_period_ns: 10,
                input_change_ns: 12,
            },
        )
        .unwrap();
        let output = result
            .signals
            .iter()
            .find(|signal| signal.name == "Q")
            .unwrap();
        assert_eq!(output.samples[0].state, LogicState::Unknown);
        assert!(output
            .samples
            .iter()
            .any(|sample| sample.time_ns == 5.0 && sample.state == LogicState::Low));
        assert!(output
            .samples
            .iter()
            .any(|sample| sample.time_ns == 15.0 && sample.state == LogicState::High));
    }

    #[test]
    fn concatenation_and_replication_drive_packed_outputs() {
        let module = parse_structural_verilog(
            "module packed #(parameter integer WIDTH=4) (input A, input B, output [1:0] Y, output [3:0] Z); assign Y={A,B}; assign Z={WIDTH{1'b1}}; endmodule",
        ).unwrap();
        let table = rtl_truth_table(&module).unwrap();
        assert_eq!(table.rows[1].inputs, [LogicState::Low, LogicState::High]);
        assert_eq!(
            table.rows[1].outputs,
            [
                LogicState::Low,
                LogicState::High,
                LogicState::High,
                LogicState::High,
                LogicState::High,
                LogicState::High
            ]
        );
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
    fn fanout_graph_reports_loads_high_fanout_and_driver_failures() {
        let result = simulate(
            &inverter_with_fanout(9, 1.0),
            &HashMap::from([("IN1".into(), LogicState::Low)]),
        );
        let output = result
            .fanout
            .nets
            .iter()
            .find(|net| net.aliases.iter().any(|alias| alias == "OUT1"))
            .unwrap();
        assert_eq!(output.drivers.len(), 1);
        assert_eq!(output.fanout, 10);
        assert!(output.high_fanout);
        assert_eq!(result.fanout.high_fanout_count, 1);

        let mut invalid = Project::default();
        let first = invalid.add_component("input", 0.0, 0.0).unwrap();
        let second = invalid.add_component("input", 0.0, 0.0).unwrap();
        let load = invalid.add_component("nmos", 0.0, 0.0).unwrap();
        let output = invalid.add_component("output", 0.0, 0.0).unwrap();
        connect(&mut invalid, (first, "out"), (second, "out"));
        connect(&mut invalid, (first, "out"), (load, "gate"));
        let report = simulate(&invalid, &HashMap::new()).fanout;
        assert_eq!(report.multiply_driven_count, 1);
        assert!(report.nets.iter().any(|net| net.multiply_driven));
        assert!(report.nets.iter().any(|net| {
            net.undriven
                && net
                    .loads
                    .iter()
                    .any(|endpoint| endpoint.component_id == Some(output))
        }));
        assert_eq!(report.undriven_count, 1);
    }

    #[test]
    fn project_fanout_threshold_controls_high_fanout_diagnostics() {
        let mut project = inverter_with_fanout(9, 1.0);
        project.set_high_fanout_warning_threshold(12).unwrap();
        let report = simulate(&project, &HashMap::new()).fanout;
        assert_eq!(report.high_fanout_threshold, 12);
        assert_eq!(report.high_fanout_count, 0);
        assert!(report.nets.iter().all(|net| !net.high_fanout));
    }

    #[test]
    fn grouped_fanout_is_stable_across_nested_hierarchy_and_save_load() {
        let mut leaf = Project::default();
        let vdd = leaf.add_component("vdd", 0.0, -4.0).unwrap();
        let gnd = leaf.add_component("gnd", 0.0, 4.0).unwrap();
        let input = leaf.add_component("input", -4.0, 0.0).unwrap();
        let output_probe = leaf.add_component("output", 4.0, 0.0).unwrap();
        let pmos = leaf.add_component("pmos", 0.0, -1.0).unwrap();
        let nmos = leaf.add_component("nmos", 0.0, 1.0).unwrap();
        connect(&mut leaf, (vdd, "out"), (pmos, "source"));
        connect(&mut leaf, (gnd, "out"), (nmos, "source"));
        connect(&mut leaf, (input, "out"), (pmos, "gate"));
        connect(&mut leaf, (input, "out"), (nmos, "gate"));
        connect(&mut leaf, (pmos, "drain"), (nmos, "drain"));
        connect(&mut leaf, (pmos, "drain"), (output_probe, "in"));
        let leaf_id = leaf.capture_block("INV".into()).unwrap();

        let mut wrapper = Project::default();
        wrapper.block_definitions = leaf.block_definitions;
        let instance = wrapper.place_block(leaf_id, 0.0, 0.0).unwrap();
        let wrapper_vdd = wrapper.add_component("vdd", 0.0, -5.0).unwrap();
        let wrapper_gnd = wrapper.add_component("gnd", 0.0, 5.0).unwrap();
        let wrapper_input = wrapper.add_component("input", -5.0, 0.0).unwrap();
        let wrapper_output = wrapper.add_component("output", 5.0, 0.0).unwrap();
        connect(&mut wrapper, (wrapper_vdd, "out"), (instance, "VDD1"));
        connect(&mut wrapper, (wrapper_gnd, "out"), (instance, "GND1"));
        connect(&mut wrapper, (wrapper_input, "out"), (instance, "IN1"));
        connect(&mut wrapper, (instance, "OUT1"), (wrapper_output, "in"));
        let wrapper_id = wrapper.capture_block("WRAPPED_INV".into()).unwrap();

        let mut top = Project::default();
        top.block_definitions = wrapper.block_definitions;
        let first = top.place_block(wrapper_id, -3.0, 0.0).unwrap();
        let second = top.place_block(wrapper_id, 3.0, 0.0).unwrap();
        let top_vdd = top.add_component("vdd", 0.0, -6.0).unwrap();
        let top_gnd = top.add_component("gnd", 0.0, 6.0).unwrap();
        let input_a = top.add_component("input", -6.0, -1.0).unwrap();
        let input_b = top.add_component("input", -6.0, 1.0).unwrap();
        let output_a = top.add_component("output", 6.0, -1.0).unwrap();
        let output_b = top.add_component("output", 6.0, 1.0).unwrap();
        top.rename_component(input_a, "A".into()).unwrap();
        top.rename_component(input_b, "B".into()).unwrap();
        top.rename_component(output_a, "Y0".into()).unwrap();
        top.rename_component(output_b, "Y1".into()).unwrap();
        for instance in [first, second] {
            connect(&mut top, (top_vdd, "out"), (instance, "VDD1"));
            connect(&mut top, (top_gnd, "out"), (instance, "GND1"));
        }
        connect(&mut top, (input_a, "out"), (first, "IN1"));
        connect(&mut top, (input_b, "out"), (second, "IN1"));
        connect(&mut top, (first, "OUT1"), (output_a, "in"));
        connect(&mut top, (second, "OUT1"), (output_b, "in"));
        top.set_waveform_groups(vec![WaveformGroup {
            id: Uuid::new_v4(),
            name: "PAIR".into(),
            signals: vec!["A".into(), "B".into(), "Y0".into(), "Y1".into()],
            radix: WaveformRadix::Binary,
            collapsed: true,
        }])
        .unwrap();

        let restored: Project =
            serde_json::from_str(&serde_json::to_string(&top).unwrap()).unwrap();
        let flattened = top.flattened().unwrap();
        let restored_flattened = restored.flattened().unwrap();
        let inputs = HashMap::from([
            ("A".into(), LogicState::Low),
            ("B".into(), LogicState::High),
        ]);
        let report = simulate(&flattened, &inputs).fanout;
        let restored_report = simulate(&restored_flattened, &inputs).fanout;
        assert_eq!(
            serde_json::to_value(&report).unwrap(),
            serde_json::to_value(&restored_report).unwrap()
        );
        let group = report
            .groups
            .iter()
            .find(|group| group.name == "PAIR")
            .unwrap();
        assert_eq!((group.total_fanout, group.max_fanout), (6, 2));

        let flattened_ids = flattened
            .components
            .iter()
            .map(|component| component.id)
            .collect::<std::collections::HashSet<_>>();
        let concrete_endpoints = report
            .nets
            .iter()
            .flat_map(|net| net.drivers.iter().chain(&net.loads))
            .filter_map(|endpoint| endpoint.component_id)
            .collect::<Vec<_>>();
        assert!(!concrete_endpoints.is_empty());
        assert!(concrete_endpoints
            .iter()
            .all(|component_id| flattened_ids.contains(component_id)));
        assert!(report
            .nets
            .iter()
            .flat_map(|net| net.drivers.iter().chain(&net.loads))
            .filter(|endpoint| endpoint.component_id.is_some())
            .all(|endpoint| top.components.iter().any(|component| {
                component.id == endpoint.component_id.unwrap()
                    || (component.kind == "block"
                        && endpoint.name.starts_with(&format!("{}·", component.name)))
            })));
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
