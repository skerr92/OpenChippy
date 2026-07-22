use crate::model::{Component, Project, TerminalRef};
use crate::physical_canvas::{device_footprint, ObstructionType, PhysicalCanvas};
use crate::physical_detailed_routing::{self, DetailedRoutingReport};
use crate::physical_drc;
use crate::physical_global_routing::{self, GlobalRoutingReport};
use crate::physical_placement::{self, PhysicalPlacementReport};
use crate::physical_planning::{self, PhysicalPlanningReport};
use crate::technology::PhysicalRuleDeck;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

pub const CURRENT_PHYSICAL_IR_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalLayoutIr {
    pub format_version: u32,
    pub source_project_name: String,
    pub technology_name: String,
    pub max_metal_layers: u16,
    pub devices: Vec<PhysicalDevice>,
    pub nets: Vec<PhysicalNet>,
    pub pins: Vec<PhysicalPin>,
    pub planning: PhysicalPlanningReport,
    pub placement: PhysicalPlacementReport,
    pub global_routing: GlobalRoutingReport,
    pub detailed_routing: DetailedRoutingReport,
    pub timing: PhysicalTimingReport,
    pub tapeout: PhysicalTapeoutReport,
    pub bounds: PhysicalBounds,
    pub shapes: Vec<PhysicalShape>,
    pub physical_blocks: Vec<PhysicalBlockImplementation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalBlockImplementation {
    pub instance_name: String,
    pub bounds: PhysicalBounds,
    pub device_ids: Vec<Uuid>,
    pub local_nets: Vec<usize>,
    pub interface_pins: Vec<PhysicalBlockPin>,
    pub shape_indices: Vec<usize>,
    pub local_drc_errors: usize,
    pub verified: bool,
    pub immutable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalBlockPin {
    pub name: String,
    pub net: usize,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalTapeoutReport {
    pub name: String,
    pub width_um: f64,
    pub height_um: f64,
    pub edge_margin_um: f64,
    pub usable_width_um: f64,
    pub usable_height_um: f64,
    pub geometry_width_um: f64,
    pub geometry_height_um: f64,
    pub area_utilization: f64,
    pub fits: bool,
    pub shapes_outside_floorplan: usize,
    pub shapes_outside_tapeout: usize,
}

fn physical_tapeout(
    shapes: &[PhysicalShape],
    floorplan: &PhysicalBounds,
    project: &Project,
) -> PhysicalTapeoutReport {
    let window = &project.technology.tapeout_window;
    let usable_width = window.width_um - 2.0 * window.edge_margin_um;
    let usable_height = window.height_um - 2.0 * window.edge_margin_um;
    let usable_left = -usable_width / 2.0;
    let usable_right = usable_width / 2.0;
    let usable_top = -usable_height / 2.0;
    let usable_bottom = usable_height / 2.0;
    let outside = |shape: &PhysicalShape, left: f64, top: f64, right: f64, bottom: f64| {
        shape.x - shape.width / 2.0 < left
            || shape.x + shape.width / 2.0 > right
            || shape.y - shape.height / 2.0 < top
            || shape.y + shape.height / 2.0 > bottom
    };
    let shapes_outside_floorplan = shapes
        .iter()
        .filter(|shape| {
            outside(
                shape,
                floorplan.min_x,
                floorplan.min_y,
                floorplan.max_x,
                floorplan.max_y,
            )
        })
        .count();
    let shapes_outside_tapeout = shapes
        .iter()
        .filter(|shape| outside(shape, usable_left, usable_top, usable_right, usable_bottom))
        .count();
    let (geometry_left, geometry_top, geometry_right, geometry_bottom) = shapes.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |(left, top, right, bottom), shape| {
            (
                left.min(shape.x - shape.width / 2.0),
                top.min(shape.y - shape.height / 2.0),
                right.max(shape.x + shape.width / 2.0),
                bottom.max(shape.y + shape.height / 2.0),
            )
        },
    );
    let geometry_width = if shapes.is_empty() {
        0.0
    } else {
        geometry_right - geometry_left
    };
    let geometry_height = if shapes.is_empty() {
        0.0
    } else {
        geometry_bottom - geometry_top
    };
    PhysicalTapeoutReport {
        name: window.name.clone(),
        width_um: window.width_um,
        height_um: window.height_um,
        edge_margin_um: window.edge_margin_um,
        usable_width_um: usable_width,
        usable_height_um: usable_height,
        geometry_width_um: geometry_width,
        geometry_height_um: geometry_height,
        area_utilization: geometry_width * geometry_height / (usable_width * usable_height),
        fits: shapes_outside_tapeout == 0,
        shapes_outside_floorplan,
        shapes_outside_tapeout,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalTimingReport {
    pub nets: Vec<PhysicalNetTiming>,
    pub paths: Vec<PhysicalTimingPath>,
    pub candidates: Vec<PhysicalCandidateTiming>,
    pub selected_candidate: Option<usize>,
    pub critical_path: Option<usize>,
    pub timing_target_ns: Option<f64>,
    pub worst_slack_ns: Option<f64>,
    pub critical_net: Option<usize>,
    pub critical_net_name: Option<String>,
    pub estimated_worst_delay_ns: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalCandidateTiming {
    pub candidate: usize,
    pub floorplan_candidate: usize,
    pub placement_candidate: usize,
    pub strategy: crate::physical_planning::FloorplanStrategy,
    pub timing_driven: bool,
    pub area_um2: f64,
    pub total_wire_length_um: f64,
    pub total_via_count: usize,
    pub routing_overflow: usize,
    pub detail_conflicts: usize,
    pub estimated_worst_delay_ns: f64,
    pub slack_ns: Option<f64>,
    pub meets_timing: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalTimingPath {
    pub input_pin: String,
    pub output_pin: String,
    pub nets: Vec<usize>,
    pub net_names: Vec<String>,
    pub device_ids: Vec<Uuid>,
    pub device_names: Vec<String>,
    pub estimated_delay_ns: f64,
    pub required_time_ns: Option<f64>,
    pub slack_ns: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalNetTiming {
    pub net: usize,
    pub name: String,
    pub fanout: usize,
    pub wire_length_um: f64,
    pub via_count: usize,
    pub routed_capacitance_ff: f64,
    pub device_capacitance_ff: f64,
    pub total_capacitance_ff: f64,
    pub estimated_delay_ns: f64,
}

fn physical_timing(
    nets: &[PhysicalNet],
    devices: &[PhysicalDevice],
    pins: &[PhysicalPin],
    detailed: &DetailedRoutingReport,
    project: &Project,
) -> PhysicalTimingReport {
    let routes = detailed
        .routes
        .iter()
        .map(|route| (route.net, route))
        .collect::<HashMap<_, _>>();
    let mut metrics = nets
        .iter()
        .map(|net| {
            let route = routes.get(&net.id).copied();
            let wire_length_um = route.map_or(0.0, |route| route.wire_length_um);
            let via_count = route.map_or(0, |route| route.via_count);
            let routed_capacitance_ff = route.map_or(0.0, |route| {
                let wire = route
                    .layer_wire_lengths_um
                    .iter()
                    .map(|(layer, length)| {
                        let coefficient = project
                            .technology
                            .physical_parasitics
                            .layer_capacitance_ff_per_um
                            .get(layer)
                            .copied()
                            .unwrap_or(
                                project
                                    .technology
                                    .physical_parasitics
                                    .wire_capacitance_ff_per_um,
                            );
                        length * coefficient
                    })
                    .sum::<f64>();
                let vias = route
                    .via_counts
                    .iter()
                    .map(|(via, count)| {
                        let coefficient = project
                            .technology
                            .physical_parasitics
                            .via_capacitance_overrides_ff
                            .get(via)
                            .copied()
                            .unwrap_or(project.technology.physical_parasitics.via_capacitance_ff);
                        *count as f64 * coefficient
                    })
                    .sum::<f64>();
                wire + vias
            });
            let mut device_capacitance_ff = 0.0;
            let mut fanout = 0;
            let mut driver_resistance_ohms = f64::INFINITY;
            for device in devices {
                let process = match device.kind {
                    DeviceKind::Nmos => &project.technology.nmos,
                    DeviceKind::Pmos => &project.technology.pmos,
                };
                if device.gate_net == net.id {
                    fanout += 1;
                    device_capacitance_ff += process.gate_capacitance_ff_per_um * device.width_um;
                }
                if device.drain_net == net.id || device.source_net == net.id {
                    device_capacitance_ff +=
                        process.diffusion_capacitance_ff_per_um * device.width_um;
                    let resistance = process.nominal_on_resistance_ohms
                        * (device.length_um / process.reference_length_um)
                        * (process.reference_width_um / device.width_um);
                    driver_resistance_ohms = driver_resistance_ohms.min(resistance);
                }
            }
            let total_capacitance_ff = routed_capacitance_ff + device_capacitance_ff;
            let estimated_delay_ns = if driver_resistance_ohms.is_finite() {
                0.69 * driver_resistance_ohms * total_capacitance_ff * 1e-6
            } else {
                0.0
            };
            PhysicalNetTiming {
                net: net.id,
                name: net.name.clone(),
                fanout,
                wire_length_um,
                via_count,
                routed_capacitance_ff,
                device_capacitance_ff,
                total_capacitance_ff,
                estimated_delay_ns,
            }
        })
        .collect::<Vec<_>>();
    metrics.sort_by_key(|metric| metric.net);
    let critical = metrics
        .iter()
        .filter(|metric| !matches!(nets[metric.net].role, NetRole::Power | NetRole::Ground))
        .max_by(|left, right| left.estimated_delay_ns.total_cmp(&right.estimated_delay_ns));
    let mut edge_devices = BTreeMap::<(usize, usize), Vec<(Uuid, String)>>::new();
    for device in devices {
        for target in [device.drain_net, device.source_net] {
            if target == device.gate_net
                || matches!(nets[target].role, NetRole::Power | NetRole::Ground)
            {
                continue;
            }
            edge_devices
                .entry((device.gate_net, target))
                .or_default()
                .push((device.component_id, device.name.clone()));
        }
    }
    for edge in edge_devices.values_mut() {
        edge.sort_by(|left, right| (&left.1, left.0).cmp(&(&right.1, right.0)));
        edge.dedup_by_key(|device| device.0);
    }
    let mut adjacency = BTreeMap::<usize, Vec<(usize, Vec<(Uuid, String)>)>>::new();
    for ((from, to), edge) in edge_devices {
        adjacency.entry(from).or_default().push((to, edge));
    }
    let outputs = pins
        .iter()
        .filter(|pin| pin.role == NetRole::Output)
        .map(|pin| (pin.net, pin.name.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut paths = Vec::new();
    for input in pins.iter().filter(|pin| pin.role == NetRole::Input) {
        let mut stack = vec![(
            input.net,
            vec![input.net],
            Vec::<Uuid>::new(),
            Vec::<String>::new(),
            HashSet::from([input.net]),
        )];
        while let Some((net, path_nets, device_ids, device_names, visited)) = stack.pop() {
            if let Some(output_pin) = outputs.get(&net) {
                let estimated_delay_ns = path_nets
                    .iter()
                    .skip(1)
                    .map(|net| metrics[*net].estimated_delay_ns)
                    .sum();
                paths.push(PhysicalTimingPath {
                    input_pin: input.name.clone(),
                    output_pin: output_pin.clone(),
                    net_names: path_nets
                        .iter()
                        .map(|net| nets[*net].name.clone())
                        .collect(),
                    estimated_delay_ns,
                    required_time_ns: project.timing_target_ns,
                    slack_ns: project
                        .timing_target_ns
                        .map(|target| target - estimated_delay_ns),
                    nets: path_nets.clone(),
                    device_ids: device_ids.clone(),
                    device_names: device_names.clone(),
                });
            }
            if path_nets.len() >= nets.len() || paths.len() >= 4096 {
                continue;
            }
            let Some(edges) = adjacency.get(&net) else {
                continue;
            };
            for (next, edge) in edges.iter().rev() {
                if visited.contains(next) {
                    continue;
                }
                let mut next_nets = path_nets.clone();
                next_nets.push(*next);
                let mut next_ids = device_ids.clone();
                next_ids.extend(edge.iter().map(|device| device.0));
                let mut next_names = device_names.clone();
                next_names.extend(edge.iter().map(|device| device.1.clone()));
                let mut next_visited = visited.clone();
                next_visited.insert(*next);
                stack.push((*next, next_nets, next_ids, next_names, next_visited));
            }
        }
    }
    paths.sort_by(|left, right| {
        (&left.input_pin, &left.output_pin, &left.nets).cmp(&(
            &right.input_pin,
            &right.output_pin,
            &right.nets,
        ))
    });
    let critical_path = paths
        .iter()
        .enumerate()
        .max_by(|(left_index, left), (right_index, right)| {
            left.estimated_delay_ns
                .total_cmp(&right.estimated_delay_ns)
                .then_with(|| right_index.cmp(left_index))
        })
        .map(|(index, _)| index);
    let estimated_worst_delay_ns = critical_path
        .map(|index| paths[index].estimated_delay_ns)
        .unwrap_or_else(|| critical.map_or(0.0, |metric| metric.estimated_delay_ns));
    PhysicalTimingReport {
        critical_path,
        timing_target_ns: project.timing_target_ns,
        worst_slack_ns: critical_path.and_then(|index| paths[index].slack_ns),
        paths,
        candidates: Vec::new(),
        selected_candidate: None,
        critical_net: critical.map(|metric| metric.net),
        critical_net_name: critical.map(|metric| metric.name.clone()),
        estimated_worst_delay_ns,
        nets: metrics,
    }
}

struct CandidateEvaluation {
    candidate: usize,
    placement: PhysicalPlacementReport,
    global: GlobalRoutingReport,
    detailed: DetailedRoutingReport,
    timing: PhysicalTimingReport,
    summary: PhysicalCandidateTiming,
}

fn candidate_is_better(left: &CandidateEvaluation, right: &CandidateEvaluation) -> bool {
    if left.global.total_overflow != right.global.total_overflow {
        return left.global.total_overflow < right.global.total_overflow;
    }
    if left.detailed.conflict_count != right.detailed.conflict_count {
        return left.detailed.conflict_count < right.detailed.conflict_count;
    }
    if let (Some(left_met), Some(right_met)) =
        (left.summary.meets_timing, right.summary.meets_timing)
    {
        if left_met != right_met {
            return left_met;
        }
        if !left_met
            && left
                .timing
                .estimated_worst_delay_ns
                .total_cmp(&right.timing.estimated_worst_delay_ns)
                != std::cmp::Ordering::Equal
        {
            return left.timing.estimated_worst_delay_ns < right.timing.estimated_worst_delay_ns;
        }
    }
    if left.summary.area_um2.total_cmp(&right.summary.area_um2) != std::cmp::Ordering::Equal {
        return left.summary.area_um2 < right.summary.area_um2;
    }
    if left
        .detailed
        .total_wire_length_um
        .total_cmp(&right.detailed.total_wire_length_um)
        != std::cmp::Ordering::Equal
    {
        return left.detailed.total_wire_length_um < right.detailed.total_wire_length_um;
    }
    if left.detailed.total_via_count != right.detailed.total_via_count {
        return left.detailed.total_via_count < right.detailed.total_via_count;
    }
    let left_score = left.placement.candidates[left.placement.selected_candidate].score;
    let right_score = right.placement.candidates[right.placement.selected_candidate].score;
    left_score < right_score || (left_score == right_score && left.candidate < right.candidate)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDevice {
    pub component_id: Uuid,
    pub name: String,
    /// Largest reusable instance that owns this device after hierarchy flattening.
    pub physical_group: Option<String>,
    pub kind: DeviceKind,
    pub gate_net: usize,
    pub drain_net: usize,
    pub source_net: usize,
    pub width_um: f64,
    pub length_um: f64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Nmos,
    Pmos,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalNet {
    pub id: usize,
    pub name: String,
    pub role: NetRole,
    pub terminals: Vec<PhysicalTerminal>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetRole {
    Power,
    Ground,
    Input,
    Output,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalTerminal {
    pub component_id: Uuid,
    pub component_name: String,
    pub terminal: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalPin {
    pub component_id: Uuid,
    pub name: String,
    pub role: NetRole,
    pub net: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalShape {
    pub layer: PhysicalLayer,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub component_id: Option<Uuid>,
    pub net: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PhysicalLayer {
    Substrate,
    Nwell,
    Ndiff,
    Pdiff,
    Poly,
    Metal(u16),
    Contact,
    Via(u16),
}

impl Serialize for PhysicalLayer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let name = match self {
            Self::Substrate => "substrate".into(),
            Self::Nwell => "nwell".into(),
            Self::Ndiff => "ndiff".into(),
            Self::Pdiff => "pdiff".into(),
            Self::Poly => "poly".into(),
            Self::Metal(index) => format!("metal{index}"),
            Self::Contact => "contact".into(),
            Self::Via(lower) => format!("via{lower}{}", lower + 1),
        };
        serializer.serialize_str(&name)
    }
}

impl<'de> Deserialize<'de> for PhysicalLayer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        match name.as_str() {
            "substrate" => Ok(Self::Substrate),
            "nwell" => Ok(Self::Nwell),
            "ndiff" => Ok(Self::Ndiff),
            "pdiff" => Ok(Self::Pdiff),
            "poly" => Ok(Self::Poly),
            "contact" => Ok(Self::Contact),
            _ if name.starts_with("metal") => name[5..]
                .parse::<u16>()
                .map(Self::Metal)
                .map_err(serde::de::Error::custom),
            _ if name.starts_with("via") => {
                let digits = &name[3..];
                (1..digits.len())
                    .find_map(|split| {
                        let lower = digits[..split].parse::<u16>().ok()?;
                        (digits[split..].parse::<u16>().ok()? == lower + 1)
                            .then_some(Self::Via(lower))
                    })
                    .ok_or_else(|| {
                        serde::de::Error::custom("physical via layer must join adjacent metals")
                    })
            }
            _ => Err(serde::de::Error::custom(format!(
                "unknown physical layer {name}"
            ))),
        }
    }
}

type TerminalKey = (Uuid, String);

fn terminals(component: &Component) -> &'static [&'static str] {
    match component.kind.as_str() {
        "nmos" | "pmos" => &["gate", "drain", "source"],
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

fn role_for(components: &[&Component]) -> NetRole {
    if components.iter().any(|component| component.kind == "vdd") {
        NetRole::Power
    } else if components.iter().any(|component| component.kind == "gnd") {
        NetRole::Ground
    } else if components.iter().any(|component| component.kind == "input") {
        NetRole::Input
    } else if components
        .iter()
        .any(|component| component.kind == "output")
    {
        NetRole::Output
    } else {
        NetRole::Internal
    }
}

fn name_for(role: NetRole, components: &[&Component], internal_index: usize) -> String {
    match role {
        NetRole::Power => "VDD".into(),
        NetRole::Ground => "GND".into(),
        NetRole::Input => components
            .iter()
            .filter(|component| component.kind == "input")
            .map(|component| component.name.clone())
            .min()
            .unwrap_or_else(|| "INPUT".into()),
        NetRole::Output => components
            .iter()
            .filter(|component| component.kind == "output")
            .map(|component| component.name.clone())
            .min()
            .unwrap_or_else(|| "OUTPUT".into()),
        NetRole::Internal => components
            .iter()
            .filter(|component| component.kind == "net_label")
            .map(|component| component.name.clone())
            .min()
            .unwrap_or_else(|| format!("N${internal_index}")),
    }
}

fn line_shape(
    start: (f64, f64),
    end: (f64, f64),
    layer: PhysicalLayer,
    net: Option<usize>,
) -> PhysicalShape {
    let horizontal = (end.0 - start.0).abs() >= (end.1 - start.1).abs();
    PhysicalShape {
        layer,
        x: (start.0 + end.0) / 2.0,
        y: (start.1 + end.1) / 2.0,
        width: if horizontal {
            (end.0 - start.0).abs() + 0.2
        } else {
            0.2
        },
        height: if horizontal {
            0.2
        } else {
            (end.1 - start.1).abs() + 0.2
        },
        component_id: None,
        net,
    }
}

#[derive(Clone, Copy)]
struct RouteAnchor {
    point: (f64, f64),
    layer: PhysicalLayer,
    access_bounds: Option<(f64, f64)>,
}

const ROUTING_LANDING_SIZE: f64 = 0.28;
const ROUTING_VIA_SIZE: f64 = 0.24;
const ROUTING_CLEARANCE: f64 = 0.04;
const ROUTING_SNAP_GUARD: f64 = 0.02;
const TRACK_SPACING: f64 = ROUTING_LANDING_SIZE + ROUTING_CLEARANCE + ROUTING_SNAP_GUARD;

fn add_via_stack(
    shapes: &mut Vec<PhysicalShape>,
    point: (f64, f64),
    from_layer: u16,
    to_layer: u16,
    net: usize,
) {
    let low = from_layer.min(to_layer);
    let high = from_layer.max(to_layer);
    for layer in low..=high {
        shapes.push(PhysicalShape {
            layer: PhysicalLayer::Metal(layer),
            x: point.0,
            y: point.1,
            width: ROUTING_LANDING_SIZE,
            height: ROUTING_LANDING_SIZE,
            component_id: None,
            net: Some(net),
        });
    }
    for lower in low..high {
        shapes.push(PhysicalShape {
            layer: PhysicalLayer::Via(lower),
            x: point.0,
            y: point.1,
            width: ROUTING_VIA_SIZE,
            height: ROUTING_VIA_SIZE,
            component_id: None,
            net: Some(net),
        });
    }
}

fn route_shape_conflicts(
    existing: &[PhysicalShape],
    candidate: &[PhysicalShape],
    net: usize,
) -> usize {
    let mut conflicting_nets = std::collections::HashSet::new();
    for shape in candidate {
        let left = shape.x - shape.width / 2.0;
        let top = shape.y - shape.height / 2.0;
        let right = shape.x + shape.width / 2.0;
        let bottom = shape.y + shape.height / 2.0;
        let clearance = if matches!(shape.layer, PhysicalLayer::Via(_)) {
            0.08 + ROUTING_SNAP_GUARD
        } else {
            ROUTING_CLEARANCE + ROUTING_SNAP_GUARD
        };
        for other in existing {
            if other.layer != shape.layer || other.net == Some(net) || other.net.is_none() {
                continue;
            }
            let other_left = other.x - other.width / 2.0;
            let other_top = other.y - other.height / 2.0;
            let other_right = other.x + other.width / 2.0;
            let other_bottom = other.y + other.height / 2.0;
            if left < other_right + clearance
                && right + clearance > other_left
                && top < other_bottom + clearance
                && bottom + clearance > other_top
            {
                conflicting_nets.insert(other.net.expect("filtered routed shape"));
            }
        }
    }
    conflicting_nets.len()
}

fn commit_preview_routing(
    shapes: Vec<PhysicalShape>,
    rules: &PhysicalRuleDeck,
) -> (Vec<PhysicalShape>, usize) {
    let mut canvas = PhysicalCanvas::new(rules);
    let mut conflicts = std::collections::HashSet::new();
    let mut accepted = vec![None; shapes.len()];
    for (shape_index, shape) in shapes.iter().enumerate() {
        let obstruction = match shape.layer {
            PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => ObstructionType::Diffusion,
            PhysicalLayer::Poly => ObstructionType::Poly,
            PhysicalLayer::Contact => ObstructionType::Contact,
            PhysicalLayer::Nwell | PhysicalLayer::Substrate => ObstructionType::Device,
            PhysicalLayer::Metal(_) | PhysicalLayer::Via(_) => continue,
        };
        if canvas
            .commit(shape, format!("preview-fixed-{shape_index}"), obstruction)
            .is_ok()
        {
            accepted[shape_index] = Some(shape.clone());
        }
    }
    for (shape_index, shape) in shapes.iter().enumerate() {
        if !matches!(shape.layer, PhysicalLayer::Metal(_)) {
            continue;
        }
        let net = shape.net.unwrap_or(usize::MAX);
        if let Err(collisions) = canvas.commit_routing_geometry(
            std::slice::from_ref(shape),
            format!("preview-shape-{shape_index}"),
        ) {
            for collision in collisions {
                let other_net = collision.net.unwrap_or(net);
                conflicts.insert((shape.layer, net.min(other_net), net.max(other_net)));
            }
        } else {
            accepted[shape_index] = Some(shape.clone());
        }
    }
    for (shape_index, shape) in shapes.iter().enumerate() {
        let PhysicalLayer::Via(lower) = shape.layer else {
            continue;
        };
        let cut = rules
            .via_overrides
            .get(&format!("via{lower}{}", lower + 1))
            .unwrap_or(&rules.via);
        let enclosed = [PhysicalLayer::Metal(lower), PhysicalLayer::Metal(lower + 1)]
            .into_iter()
            .all(|required_layer| {
                accepted.iter().flatten().any(|metal| {
                    if metal.layer != required_layer || metal.net != shape.net {
                        return false;
                    }
                    let metal_left = metal.x - metal.width / 2.0;
                    let metal_top = metal.y - metal.height / 2.0;
                    let metal_right = metal.x + metal.width / 2.0;
                    let metal_bottom = metal.y + metal.height / 2.0;
                    let via_left = shape.x - shape.width / 2.0;
                    let via_top = shape.y - shape.height / 2.0;
                    let via_right = shape.x + shape.width / 2.0;
                    let via_bottom = shape.y + shape.height / 2.0;
                    metal_left <= via_left - cut.enclosure_um + 1e-9
                        && metal_top <= via_top - cut.enclosure_um + 1e-9
                        && metal_right >= via_right + cut.enclosure_um - 1e-9
                        && metal_bottom >= via_bottom + cut.enclosure_um - 1e-9
                })
            });
        let net = shape.net.unwrap_or(usize::MAX);
        if !enclosed {
            conflicts.insert((shape.layer, net, net));
            continue;
        }
        if let Err(collisions) = canvas.commit_routing_geometry(
            std::slice::from_ref(shape),
            format!("preview-via-{shape_index}"),
        ) {
            for collision in collisions {
                let other_net = collision.net.unwrap_or(net);
                conflicts.insert((shape.layer, net.min(other_net), net.max(other_net)));
            }
        } else {
            accepted[shape_index] = Some(shape.clone());
        }
    }
    (accepted.into_iter().flatten().collect(), conflicts.len())
}

fn compact_preview(
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    max_metal_layers: u16,
    rules: &PhysicalRuleDeck,
    planning: &PhysicalPlanningReport,
    placement: &PhysicalPlacementReport,
) -> (PhysicalBounds, Vec<PhysicalShape>) {
    let signal_net_count = nets
        .iter()
        .filter(|net| {
            matches!(
                net.role,
                NetRole::Input | NetRole::Output | NetRole::Internal
            )
        })
        .count();
    let first_horizontal_layer = max_metal_layers.max(2);
    let horizontal_layer_count = usize::from(max_metal_layers - first_horizontal_layer + 1);
    // Tracks keep distinct Y coordinates even when their horizontal metals differ:
    // a via stack crosses every intermediate metal and would otherwise land on
    // another net sharing the same coordinate.
    let selected = &planning.candidates[planning.selected_candidate];
    let selected_placement = &placement.candidates[placement.selected_candidate];
    let hierarchical = selected_placement.block_regions.len() >= 2;
    let region_by_group = selected_placement
        .block_regions
        .iter()
        .map(|region| (region.name.as_str(), (region.min_x, region.max_x)))
        .collect::<HashMap<_, _>>();
    let placed_by_id = selected_placement
        .devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let cell_width = selected.width_um;
    let cell_height = selected.height_um;
    let vdd_y = -cell_height / 2.0 + 0.5;
    let gnd_y = cell_height / 2.0 - 0.5;
    let bounds = PhysicalBounds {
        min_x: -cell_width / 2.0,
        min_y: -cell_height / 2.0,
        max_x: cell_width / 2.0,
        max_y: cell_height / 2.0,
    };
    let center_channel = (signal_net_count.max(1) as f64 * TRACK_SPACING + 0.9).max(1.4);
    const DEVICE_BANK_EDGE_MARGIN: f64 = 1.25;
    let pmos_last_y = selected_placement
        .devices
        .iter()
        .filter(|device| device.kind == DeviceKind::Pmos)
        .map(|device| device.y)
        .fold(bounds.min_y + DEVICE_BANK_EDGE_MARGIN, f64::max);
    let max_pdiffusion_height = devices
        .iter()
        .filter(|device| device.kind == DeviceKind::Pmos)
        .map(|device| 0.55 + device.width_um.min(4.0) * 0.12)
        .fold(0.55, f64::max);
    let well_top = bounds.min_y + 0.05;
    let well_bottom = (pmos_last_y
        + max_pdiffusion_height / 2.0
        + rules.well_enclosure_um
        + rules.manufacturing_grid_um)
        .min(bounds.max_y - 0.05);
    let mut shapes = vec![
        PhysicalShape {
            layer: PhysicalLayer::Substrate,
            x: 0.0,
            y: 0.0,
            width: cell_width,
            height: cell_height,
            component_id: None,
            net: None,
        },
        PhysicalShape {
            layer: PhysicalLayer::Nwell,
            x: 0.0,
            y: (well_top + well_bottom) / 2.0,
            width: cell_width - 0.1,
            height: well_bottom - well_top,
            component_id: None,
            net: None,
        },
    ];
    let power_net = nets
        .iter()
        .find(|net| net.role == NetRole::Power)
        .map(|net| net.id);
    let ground_net = nets
        .iter()
        .find(|net| net.role == NetRole::Ground)
        .map(|net| net.id);
    if hierarchical {
        for region in &selected_placement.block_regions {
            let left = region.min_x + 0.12;
            let right = region.max_x - 0.12;
            if left < right {
                shapes.push(line_shape(
                    (left, vdd_y),
                    (right, vdd_y),
                    PhysicalLayer::Metal(1),
                    power_net,
                ));
                shapes.push(line_shape(
                    (left, gnd_y),
                    (right, gnd_y),
                    PhysicalLayer::Metal(1),
                    ground_net,
                ));
            }
        }
    } else {
        shapes.push(line_shape(
            (bounds.min_x + 0.25, vdd_y),
            (bounds.max_x - 0.25, vdd_y),
            PhysicalLayer::Metal(1),
            power_net,
        ));
        shapes.push(line_shape(
            (bounds.min_x + 0.25, gnd_y),
            (bounds.max_x - 0.25, gnd_y),
            PhysicalLayer::Metal(1),
            ground_net,
        ));
    }

    let mut anchors: HashMap<usize, Vec<RouteAnchor>> = HashMap::new();
    for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
        let row = devices
            .iter()
            .filter(|device| device.kind == kind)
            .collect::<Vec<_>>();
        for device in row {
            let placed = placed_by_id[&device.component_id];
            let access_bounds = device
                .physical_group
                .as_deref()
                .and_then(|group| region_by_group.get(group).copied());
            let x = placed.x;
            let y = placed.y;
            shapes.extend(
                device_footprint(device, x, y, rules)
                    .into_iter()
                    .map(|(shape, _)| shape),
            );
            for (terminal_x, net) in [(x - 0.58, device.drain_net), (x + 0.58, device.source_net)] {
                anchors.entry(net).or_default().push(RouteAnchor {
                    point: (terminal_x, y),
                    layer: PhysicalLayer::Metal(1),
                    access_bounds,
                });
            }
            anchors
                .entry(device.gate_net)
                .or_default()
                .push(RouteAnchor {
                    point: (x, y),
                    layer: PhysicalLayer::Metal(1),
                    access_bounds,
                });
        }
    }

    let signal_nets = nets
        .iter()
        .filter(|net| {
            net.role == NetRole::Input
                || net.role == NetRole::Output
                || net.role == NetRole::Internal
        })
        .map(|net| net.id)
        .collect::<Vec<_>>();
    let signal_layer = signal_nets
        .iter()
        .enumerate()
        .map(|(index, net)| {
            (
                *net,
                first_horizontal_layer + (index % horizontal_layer_count) as u16,
            )
        })
        .collect::<HashMap<_, _>>();
    let signal_track = signal_nets
        .iter()
        .enumerate()
        .map(|(index, net)| {
            let center = signal_nets.len().saturating_sub(1) as f64 / 2.0;
            (*net, (index as f64 - center) * TRACK_SPACING)
        })
        .collect::<HashMap<_, _>>();

    let signal_pins = pins
        .iter()
        .filter(|pin| pin.role == NetRole::Input || pin.role == NetRole::Output)
        .collect::<Vec<_>>();
    let mut input_index = 0;
    let mut output_index = 0;
    for pin in signal_pins {
        let x = if pin.role == NetRole::Input {
            let x = bounds.min_x + 0.35 + input_index as f64 * 0.7;
            input_index += 1;
            x
        } else {
            let x = bounds.max_x - 0.35 - output_index as f64 * 0.7;
            output_index += 1;
            x
        };
        let y = signal_track[&pin.net];
        let layer = PhysicalLayer::Metal(signal_layer[&pin.net]);
        anchors.entry(pin.net).or_default().push(RouteAnchor {
            point: (x, y),
            layer,
            access_bounds: None,
        });
        shapes.push(PhysicalShape {
            layer,
            x,
            y,
            width: 0.55,
            height: ROUTING_LANDING_SIZE,
            component_id: Some(pin.component_id),
            net: Some(pin.net),
        });
    }

    let device_group = devices
        .iter()
        .filter_map(|device| {
            device
                .physical_group
                .as_deref()
                .map(|group| (device.component_id, group))
        })
        .collect::<HashMap<_, _>>();
    let net_group_count = nets
        .iter()
        .map(|net| {
            let groups = net
                .terminals
                .iter()
                .filter_map(|terminal| device_group.get(&terminal.component_id).copied())
                .collect::<HashSet<_>>();
            (net.id, groups.len())
        })
        .collect::<HashMap<_, _>>();
    let mut routed_nets = nets.iter().collect::<Vec<_>>();
    if hierarchical {
        routed_nets.sort_by_key(|net| match net.role {
            NetRole::Internal if net_group_count[&net.id] == 1 => 0,
            NetRole::Internal if net_group_count[&net.id] > 1 => 1,
            NetRole::Input | NetRole::Output | NetRole::Internal => 2,
            NetRole::Power | NetRole::Ground => 3,
        });
    }
    for net in routed_nets {
        let Some(points) = anchors.get(&net.id) else {
            continue;
        };
        let track_y = match net.role {
            NetRole::Power => vdd_y,
            NetRole::Ground => gnd_y,
            NetRole::Input | NetRole::Output | NetRole::Internal => signal_track[&net.id],
        };
        let min_x = points
            .iter()
            .map(|anchor| anchor.point.0)
            .fold(f64::INFINITY, f64::min);
        let max_x = points
            .iter()
            .map(|anchor| anchor.point.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let signal = net.role != NetRole::Power && net.role != NetRole::Ground;
        let route_layer = if signal {
            PhysicalLayer::Metal(signal_layer[&net.id])
        } else {
            PhysicalLayer::Metal(1)
        };
        if !hierarchical || signal {
            shapes.push(line_shape(
                (min_x, track_y),
                (max_x, track_y),
                route_layer,
                Some(net.id),
            ));
        }
        for anchor in points {
            let preferred_drop = if !signal {
                2
            } else if anchor.point.1 > center_channel / 2.0 {
                max_metal_layers.saturating_sub(1).max(1)
            } else {
                max_metal_layers.saturating_sub(2).max(1)
            };
            let mut candidate_layers = if max_metal_layers < 3 {
                vec![1]
            } else {
                (2..max_metal_layers).collect::<Vec<_>>()
            };
            candidate_layers.sort_by_key(|layer| layer.abs_diff(preferred_drop));
            // Search the complete synthesized width. The first-zero early exit
            // keeps ordinary cases cheap while dense cells can reach a legal
            // column instead of accepting a colliding nearby candidate.
            let (access_min_x, access_max_x) =
                if hierarchical && (!signal || net_group_count[&net.id] == 1) {
                    anchor.access_bounds.unwrap_or((bounds.min_x, bounds.max_x))
                } else {
                    (bounds.min_x, bounds.max_x)
                };
            let max_jog_steps =
                (((access_max_x - access_min_x) / TRACK_SPACING).ceil() as usize).max(1);
            let mut drop_columns = vec![(0usize, anchor.point.0)];
            for step in 1..=max_jog_steps {
                for direction in [-1.0, 1.0] {
                    let x = anchor.point.0 + direction * step as f64 * TRACK_SPACING;
                    if x >= access_min_x + ROUTING_LANDING_SIZE
                        && x <= access_max_x - ROUTING_LANDING_SIZE
                    {
                        drop_columns.push((step, x));
                    }
                }
            }
            let mut best_candidate = None;
            'access_search: for drop_layer_index in candidate_layers {
                for (jog_steps, drop_x) in &drop_columns {
                    let mut candidate = Vec::new();
                    let drop_layer = if anchor.layer == PhysicalLayer::Metal(1) {
                        PhysicalLayer::Metal(drop_layer_index)
                    } else {
                        anchor.layer
                    };
                    if anchor.layer == PhysicalLayer::Metal(1) {
                        if (anchor.point.0 - *drop_x).abs() > f64::EPSILON {
                            candidate.push(line_shape(
                                anchor.point,
                                (*drop_x, anchor.point.1),
                                drop_layer,
                                Some(net.id),
                            ));
                        }
                    }
                    if (anchor.point.1 - track_y).abs() > f64::EPSILON {
                        candidate.push(line_shape(
                            (*drop_x, anchor.point.1),
                            (*drop_x, track_y),
                            drop_layer,
                            Some(net.id),
                        ));
                    }
                    if signal && anchor.layer == PhysicalLayer::Metal(1) {
                        let target_layer = signal_layer[&net.id];
                        add_via_stack(&mut candidate, anchor.point, 1, drop_layer_index, net.id);
                        add_via_stack(
                            &mut candidate,
                            (*drop_x, track_y),
                            drop_layer_index,
                            target_layer,
                            net.id,
                        );
                    } else if !signal && drop_layer_index > 1 {
                        add_via_stack(&mut candidate, anchor.point, 1, drop_layer_index, net.id);
                        add_via_stack(
                            &mut candidate,
                            (*drop_x, track_y),
                            1,
                            drop_layer_index,
                            net.id,
                        );
                    }
                    let conflicts = route_shape_conflicts(&shapes, &candidate, net.id);
                    let routed = (
                        conflicts,
                        drop_layer_index.abs_diff(preferred_drop),
                        *jog_steps,
                        candidate,
                    );
                    let replace = best_candidate.as_ref().is_none_or(
                        |(best_conflicts, best_preference, best_jog, _)| {
                            (routed.0, routed.1, routed.2)
                                < (*best_conflicts, *best_preference, *best_jog)
                        },
                    );
                    if replace {
                        best_candidate = Some(routed);
                    }
                    if conflicts == 0 {
                        break 'access_search;
                    }
                }
            }
            if let Some((_, _, _, candidate)) = best_candidate {
                shapes.extend(candidate);
            }
        }
    }
    if hierarchical && max_metal_layers >= 2 {
        for (y, net) in [(vdd_y, power_net), (gnd_y, ground_net)] {
            let Some(net) = net else { continue };
            shapes.push(line_shape(
                (bounds.min_x + 0.25, y),
                (bounds.max_x - 0.25, y),
                PhysicalLayer::Metal(2),
                Some(net),
            ));
            for region in &selected_placement.block_regions {
                let x = (region.min_x + region.max_x) / 2.0;
                add_via_stack(&mut shapes, (x, y), 1, 2, net);
            }
        }
    }
    (bounds, shapes)
}

fn snap_to_grid(value: f64, grid: f64) -> f64 {
    (value / grid).round() * grid
}

fn snap_shape_to_grid(shape: &mut PhysicalShape, grid: f64) {
    let left = snap_to_grid(shape.x - shape.width / 2.0, grid);
    let top = snap_to_grid(shape.y - shape.height / 2.0, grid);
    let right = snap_to_grid(shape.x + shape.width / 2.0, grid);
    let bottom = snap_to_grid(shape.y + shape.height / 2.0, grid);
    shape.x = (left + right) / 2.0;
    shape.y = (top + bottom) / 2.0;
    shape.width = (right - left).max(grid);
    shape.height = (bottom - top).max(grid);
}

fn deduplicate_exact_vias(shapes: &mut Vec<PhysicalShape>, grid: f64) {
    let mut seen = std::collections::HashSet::new();
    shapes.retain(|shape| {
        let PhysicalLayer::Via(layer) = shape.layer else {
            return true;
        };
        seen.insert((
            layer,
            shape.net,
            (shape.x / grid).round() as i64,
            (shape.y / grid).round() as i64,
            (shape.width / grid).round() as i64,
            (shape.height / grid).round() as i64,
        ))
    });
}

fn physical_blocks(
    ir: &PhysicalLayoutIr,
    drc: &physical_drc::PhysicalDrcReport,
) -> Vec<PhysicalBlockImplementation> {
    let selected = &ir.placement.candidates[ir.placement.selected_candidate];
    let group_by_device = ir
        .devices
        .iter()
        .filter_map(|device| {
            device
                .physical_group
                .as_ref()
                .map(|group| (device.component_id, group.as_str()))
        })
        .collect::<HashMap<_, _>>();
    selected
        .block_regions
        .iter()
        .map(|region| {
            let mut device_ids = group_by_device
                .iter()
                .filter_map(|(id, group)| (*group == region.name).then_some(*id))
                .collect::<Vec<_>>();
            device_ids.sort_unstable();
            let device_set = device_ids.iter().copied().collect::<HashSet<_>>();
            let touched = ir
                .devices
                .iter()
                .filter(|device| device_set.contains(&device.component_id))
                .flat_map(|device| [device.gate_net, device.drain_net, device.source_net])
                .collect::<HashSet<_>>();
            let mut local_nets = touched
                .iter()
                .copied()
                .filter(|net| {
                    let physical_net = &ir.nets[*net];
                    physical_net.role == NetRole::Internal
                        && physical_net.terminals.iter().all(|terminal| {
                            group_by_device.get(&terminal.component_id).copied()
                                == Some(region.name.as_str())
                        })
                })
                .collect::<Vec<_>>();
            local_nets.sort_unstable();
            let local_set = local_nets.iter().copied().collect::<HashSet<_>>();
            let mut interface_nets = touched.difference(&local_set).copied().collect::<Vec<_>>();
            interface_nets.sort_unstable();
            let interface_pins = interface_nets
                .iter()
                .enumerate()
                .map(|(index, net)| {
                    let physical_net = &ir.nets[*net];
                    let fraction = (index + 1) as f64 / (interface_nets.len() + 1) as f64;
                    let (x, y) = match physical_net.role {
                        NetRole::Power => (
                            region.min_x + fraction * (region.max_x - region.min_x),
                            region.min_y,
                        ),
                        NetRole::Ground => (
                            region.min_x + fraction * (region.max_x - region.min_x),
                            region.max_y,
                        ),
                        NetRole::Output => (
                            region.max_x,
                            region.min_y + fraction * (region.max_y - region.min_y),
                        ),
                        NetRole::Input | NetRole::Internal => (
                            region.min_x,
                            region.min_y + fraction * (region.max_y - region.min_y),
                        ),
                    };
                    PhysicalBlockPin {
                        name: physical_net.name.clone(),
                        net: *net,
                        x,
                        y,
                    }
                })
                .collect::<Vec<_>>();
            let shape_indices = ir
                .shapes
                .iter()
                .enumerate()
                .filter_map(|(index, shape)| {
                    (shape
                        .component_id
                        .is_some_and(|id| device_set.contains(&id))
                        || shape.net.is_some_and(|net| local_set.contains(&net)))
                    .then_some(index)
                })
                .collect::<Vec<_>>();
            let shape_set = shape_indices.iter().copied().collect::<HashSet<_>>();
            let local_drc_errors = drc
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    !diagnostic.shape_indices.is_empty()
                        && diagnostic
                            .shape_indices
                            .iter()
                            .all(|index| shape_set.contains(index))
                })
                .count();
            PhysicalBlockImplementation {
                instance_name: region.name.clone(),
                bounds: PhysicalBounds {
                    min_x: region.min_x,
                    min_y: region.min_y,
                    max_x: region.max_x,
                    max_y: region.max_y,
                },
                device_ids,
                local_nets,
                interface_pins,
                shape_indices,
                local_drc_errors,
                verified: local_drc_errors == 0,
                immutable: region.immutable,
            }
        })
        .collect()
}

fn top_level_routing_nets(nets: &[PhysicalNet], devices: &[PhysicalDevice]) -> Vec<PhysicalNet> {
    let group_by_device = devices
        .iter()
        .filter_map(|device| {
            device
                .physical_group
                .as_deref()
                .map(|group| (device.component_id, group))
        })
        .collect::<HashMap<_, _>>();
    nets.iter()
        .filter(|net| {
            if net.role != NetRole::Internal {
                return true;
            }
            let groups = net
                .terminals
                .iter()
                .filter_map(|terminal| group_by_device.get(&terminal.component_id).copied())
                .collect::<HashSet<_>>();
            let all_grouped = net
                .terminals
                .iter()
                .all(|terminal| group_by_device.contains_key(&terminal.component_id));
            !(all_grouped && groups.len() == 1)
        })
        .cloned()
        .collect()
}

pub fn normalize(project: &Project) -> Result<PhysicalLayoutIr, String> {
    let physical_components = project
        .components
        .iter()
        .filter(|component| {
            matches!(
                component.kind.as_str(),
                "nmos" | "pmos" | "vdd" | "gnd" | "input" | "output" | "junction" | "net_label"
            )
        })
        .collect::<Vec<_>>();
    let component_by_id = physical_components
        .iter()
        .map(|component| (component.id, *component))
        .collect::<HashMap<_, _>>();

    let mut sets = DisjointSet::default();
    let mut terminal_indices = HashMap::new();
    for component in &physical_components {
        for terminal in terminals(component) {
            terminal_indices.insert((component.id, (*terminal).into()), sets.add());
        }
    }
    for wire in &project.wires {
        let Some(to) = &wire.to else {
            continue;
        };
        let Some(from_index) = terminal_indices.get(&terminal_key(&wire.from)) else {
            continue;
        };
        let Some(to_index) = terminal_indices.get(&terminal_key(to)) else {
            continue;
        };
        sets.union(*from_index, *to_index);
    }

    let mut first_label = HashMap::new();
    for component in physical_components
        .iter()
        .filter(|component| component.kind == "net_label")
    {
        let index = terminal_indices[&(component.id, "node".into())];
        if let Some(previous) = first_label.insert(component.name.as_str(), index) {
            sets.union(previous, index);
        }
    }

    let mut root_terminals: HashMap<usize, Vec<PhysicalTerminal>> = HashMap::new();
    for ((component_id, terminal), index) in &terminal_indices {
        let component = component_by_id[component_id];
        root_terminals
            .entry(sets.find(*index))
            .or_default()
            .push(PhysicalTerminal {
                component_id: *component_id,
                component_name: component.name.clone(),
                terminal: terminal.clone(),
            });
    }
    for terminals in root_terminals.values_mut() {
        terminals.sort_by(|left, right| {
            (&left.component_name, &left.terminal).cmp(&(&right.component_name, &right.terminal))
        });
    }

    let mut ordered_roots = root_terminals
        .keys()
        .map(|root| {
            let signature = root_terminals[root]
                .iter()
                .map(|terminal| format!("{}:{}", terminal.component_name, terminal.terminal))
                .collect::<Vec<_>>()
                .join("|");
            (*root, signature)
        })
        .collect::<Vec<_>>();
    ordered_roots.sort_by(|left, right| left.1.cmp(&right.1));
    let root_to_net = ordered_roots
        .iter()
        .enumerate()
        .map(|(net, (root, _))| (*root, net))
        .collect::<HashMap<_, _>>();

    let mut terminal_nets = HashMap::new();
    for (terminal, index) in &terminal_indices {
        terminal_nets.insert(terminal.clone(), root_to_net[&sets.find(*index)]);
    }

    let mut nets = Vec::new();
    let mut internal_index = 1;
    for (id, (root, _)) in ordered_roots.iter().enumerate() {
        let terminals = root_terminals[root].clone();
        let components = terminals
            .iter()
            .filter_map(|terminal| component_by_id.get(&terminal.component_id).copied())
            .collect::<Vec<_>>();
        let role = role_for(&components);
        let name = name_for(role, &components, internal_index);
        if role == NetRole::Internal {
            internal_index += 1;
        }
        nets.push(PhysicalNet {
            id,
            name,
            role,
            terminals,
        });
    }

    let mut devices = physical_components
        .iter()
        .filter(|component| component.kind == "nmos" || component.kind == "pmos")
        .map(|component| {
            let characteristics = project.device_characteristics(component.id)?;
            Ok(PhysicalDevice {
                component_id: component.id,
                name: component.name.clone(),
                physical_group: component
                    .name
                    .split_once('·')
                    .map(|(instance, _)| instance.to_string()),
                kind: if component.kind == "pmos" {
                    DeviceKind::Pmos
                } else {
                    DeviceKind::Nmos
                },
                gate_net: terminal_nets[&(component.id, "gate".into())],
                drain_net: terminal_nets[&(component.id, "drain".into())],
                source_net: terminal_nets[&(component.id, "source".into())],
                width_um: characteristics.width_um,
                length_um: characteristics.length_um,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    devices.sort_by(|left, right| (&left.kind, &left.name).cmp(&(&right.kind, &right.name)));
    let top_level_nets = top_level_routing_nets(&nets, &devices);

    let mut pins = physical_components
        .iter()
        .filter_map(|component| {
            let (terminal, role) = match component.kind.as_str() {
                "vdd" => ("out", NetRole::Power),
                "gnd" => ("out", NetRole::Ground),
                "input" => ("out", NetRole::Input),
                "output" => ("in", NetRole::Output),
                _ => return None,
            };
            Some(PhysicalPin {
                component_id: component.id,
                name: component.name.clone(),
                role,
                net: terminal_nets[&(component.id, terminal.into())],
            })
        })
        .collect::<Vec<_>>();
    pins.sort_by(|left, right| (&left.role, &left.name).cmp(&(&right.role, &right.name)));
    let mut planning = physical_planning::plan(&devices, &nets, &pins, &project.technology);
    let has_feasible_candidate = planning
        .candidates
        .iter()
        .any(|candidate| candidate.feasible);
    let mut best: Option<CandidateEvaluation> = None;
    let mut candidate_summaries = Vec::new();
    for candidate_index in 0..planning.candidates.len() {
        // A deliberately constrained rule deck can mark every floorplan as
        // over-utilized. Still route those candidates so negotiation can
        // report bounded overflow instead of making normalization fail.
        if has_feasible_candidate && !planning.candidates[candidate_index].feasible {
            continue;
        }
        let mut candidate_plan = planning.clone();
        candidate_plan.selected_candidate = candidate_index;
        let candidate_placement =
            physical_placement::place(&devices, &nets, &project.technology, &candidate_plan);
        let top_level_net_ids = top_level_nets
            .iter()
            .map(|net| net.id)
            .collect::<HashSet<_>>();
        let mut routing_plan = candidate_plan.clone();
        routing_plan
            .nets
            .retain(|net| top_level_net_ids.contains(&net.net));
        let candidate_global = physical_global_routing::route(
            &top_level_nets,
            &pins,
            &candidate_placement,
            &routing_plan,
        );
        let candidate_detailed = physical_detailed_routing::route(
            &devices,
            &nets,
            &pins,
            &candidate_global,
            &candidate_plan,
            &project.technology,
            &candidate_placement,
        );
        let candidate_timing =
            physical_timing(&nets, &devices, &pins, &candidate_detailed, project);
        let floorplan = &candidate_plan.candidates[candidate_index];
        let summary = PhysicalCandidateTiming {
            candidate: candidate_summaries.len(),
            floorplan_candidate: candidate_index,
            placement_candidate: candidate_placement.selected_candidate,
            strategy: floorplan.strategy,
            timing_driven: false,
            area_um2: floorplan.width_um * floorplan.height_um,
            total_wire_length_um: candidate_detailed.total_wire_length_um,
            total_via_count: candidate_detailed.total_via_count,
            routing_overflow: candidate_global.total_overflow,
            detail_conflicts: candidate_detailed.conflict_count,
            estimated_worst_delay_ns: candidate_timing.estimated_worst_delay_ns,
            slack_ns: candidate_timing.worst_slack_ns,
            meets_timing: candidate_timing.worst_slack_ns.map(|slack| slack >= 0.0),
        };
        let mut evaluation = CandidateEvaluation {
            candidate: candidate_index,
            placement: candidate_placement,
            global: candidate_global,
            detailed: candidate_detailed,
            timing: candidate_timing,
            summary,
        };
        candidate_summaries.push(evaluation.summary.clone());
        if project.timing_target_ns.is_some() {
            if let Some(path_index) = evaluation.timing.critical_path {
                let critical_path = &evaluation.timing.paths[path_index];
                let critical_nets = critical_path.nets.iter().copied().collect::<HashSet<_>>();
                let mut timing_plan = candidate_plan.clone();
                for net in &mut timing_plan.nets {
                    if net.class != crate::physical_planning::PhysicalNetClass::Power
                        && critical_nets.contains(&net.net)
                    {
                        net.priority = 10;
                        net.estimated_demand *= 2.0;
                    }
                }
                timing_plan.nets.sort_by_key(|net| {
                    (net.priority, std::cmp::Reverse(net.terminal_count), net.net)
                });
                let mut timing_placement =
                    physical_placement::place(&devices, &nets, &project.technology, &timing_plan);
                physical_placement::add_timing_candidate(
                    &mut timing_placement,
                    &devices,
                    &nets,
                    &timing_plan,
                    &project.technology,
                    &critical_path.device_ids,
                );
                for placement_index in 0..timing_placement.candidates.len() {
                    if !timing_placement.candidates[placement_index].legal {
                        continue;
                    }
                    let mut routed_placement = timing_placement.clone();
                    routed_placement.selected_candidate = placement_index;
                    let timing_global = physical_global_routing::route(
                        &top_level_nets,
                        &pins,
                        &routed_placement,
                        &{
                            let mut routing_plan = timing_plan.clone();
                            routing_plan
                                .nets
                                .retain(|net| top_level_net_ids.contains(&net.net));
                            routing_plan
                        },
                    );
                    let timing_detailed = physical_detailed_routing::route(
                        &devices,
                        &nets,
                        &pins,
                        &timing_global,
                        &timing_plan,
                        &project.technology,
                        &routed_placement,
                    );
                    let timing_report =
                        physical_timing(&nets, &devices, &pins, &timing_detailed, project);
                    let timing_summary = PhysicalCandidateTiming {
                        candidate: candidate_summaries.len(),
                        floorplan_candidate: candidate_index,
                        placement_candidate: placement_index,
                        strategy: floorplan.strategy,
                        timing_driven: true,
                        area_um2: floorplan.width_um * floorplan.height_um,
                        total_wire_length_um: timing_detailed.total_wire_length_um,
                        total_via_count: timing_detailed.total_via_count,
                        routing_overflow: timing_global.total_overflow,
                        detail_conflicts: timing_detailed.conflict_count,
                        estimated_worst_delay_ns: timing_report.estimated_worst_delay_ns,
                        slack_ns: timing_report.worst_slack_ns,
                        meets_timing: timing_report.worst_slack_ns.map(|slack| slack >= 0.0),
                    };
                    let timing_evaluation = CandidateEvaluation {
                        candidate: candidate_index,
                        placement: routed_placement,
                        global: timing_global,
                        detailed: timing_detailed,
                        timing: timing_report,
                        summary: timing_summary,
                    };
                    candidate_summaries.push(timing_evaluation.summary.clone());
                    if candidate_is_better(&timing_evaluation, &evaluation) {
                        evaluation = timing_evaluation;
                    }
                }
            }
        }
        let better = best
            .as_ref()
            .is_none_or(|current| candidate_is_better(&evaluation, current));
        if better {
            best = Some(evaluation);
        }
    }
    let selected =
        best.ok_or_else(|| "Physical planning produced no feasible candidate.".to_string())?;
    let selected_candidate = selected.candidate;
    let selected_timing_candidate = selected.summary.candidate;
    let placement = selected.placement;
    let global_routing = selected.global;
    let detailed_routing = selected.detailed;
    planning.selected_candidate = selected_candidate;
    let grid = project.technology.physical_rules.manufacturing_grid_um;
    let mut route_orders = vec![nets.clone()];
    let mut reversed = nets.clone();
    reversed.reverse();
    route_orders.push(reversed);
    let mut high_fanout_first = nets.clone();
    high_fanout_first.sort_by_key(|net| (std::cmp::Reverse(net.terminals.len()), net.id));
    route_orders.push(high_fanout_first);
    let mut preview_candidates = Vec::new();
    for ordered_nets in route_orders {
        let (bounds, mut shapes) = compact_preview(
            &devices,
            &ordered_nets,
            &pins,
            project.technology.max_metal_layers,
            &project.technology.physical_rules,
            &planning,
            &placement,
        );
        for shape in &mut shapes {
            snap_shape_to_grid(shape, grid);
        }
        deduplicate_exact_vias(&mut shapes, grid);
        let (shapes, rejected_conflicts) =
            commit_preview_routing(shapes, &project.technology.physical_rules);
        preview_candidates.push((rejected_conflicts, bounds, shapes));
    }
    let (_, mut bounds, shapes) = preview_candidates
        .into_iter()
        .min_by_key(|(conflicts, _, _)| *conflicts)
        .expect("at least one physical preview candidate");
    bounds.min_x = snap_to_grid(bounds.min_x, grid);
    bounds.min_y = snap_to_grid(bounds.min_y, grid);
    bounds.max_x = snap_to_grid(bounds.max_x, grid);
    bounds.max_y = snap_to_grid(bounds.max_y, grid);
    candidate_summaries.sort_by_key(|candidate| candidate.candidate);
    let mut timing = physical_timing(&nets, &devices, &pins, &detailed_routing, project);
    timing.candidates = candidate_summaries;
    timing.selected_candidate = Some(selected_timing_candidate);
    let tapeout = physical_tapeout(&shapes, &bounds, project);

    let mut ir = PhysicalLayoutIr {
        format_version: CURRENT_PHYSICAL_IR_VERSION,
        source_project_name: project.name.clone(),
        technology_name: project.technology.name.clone(),
        max_metal_layers: project.technology.max_metal_layers,
        devices,
        nets,
        pins,
        planning,
        placement,
        global_routing,
        detailed_routing,
        timing,
        tapeout,
        bounds,
        shapes,
        physical_blocks: Vec::new(),
    };
    let block_drc = physical_drc::validate(&ir, &project.technology);
    ir.physical_blocks = physical_blocks(&ir, &block_drc);
    Ok(ir)
}

pub fn normalize_project(project: &Project) -> Result<PhysicalLayoutIr, String> {
    let implementation = if project
        .components
        .iter()
        .any(|component| component.block_definition_id.is_some())
    {
        project.flattened()?
    } else {
        project.clone()
    };
    normalize(&implementation)
}

#[cfg(test)]
mod tests {
    use super::{
        normalize, DeviceKind, NetRole, PhysicalLayer, ROUTING_CLEARANCE, ROUTING_LANDING_SIZE,
    };
    use crate::model::{Project, TerminalRef};
    use crate::physical_canvas::PhysicalCanvas;
    use crate::{physical_drc, technology::Technology};
    use std::collections::HashSet;
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

    fn inverter() -> Project {
        let mut project = Project::default();
        project.rename("Inverter".into()).unwrap();
        let vdd = project.add_component("vdd", 10.0, -20.0).unwrap();
        let gnd = project.add_component("gnd", 10.0, 20.0).unwrap();
        let input = project.add_component("input", -20.0, 0.0).unwrap();
        let output = project.add_component("output", 20.0, 0.0).unwrap();
        let pmos = project.add_component("pmos", 0.0, -5.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 5.0).unwrap();
        connect(&mut project, (vdd, "out"), (pmos, "source"));
        connect(&mut project, (gnd, "out"), (nmos, "source"));
        connect(&mut project, (input, "out"), (pmos, "gate"));
        connect(&mut project, (input, "out"), (nmos, "gate"));
        connect(&mut project, (pmos, "drain"), (nmos, "drain"));
        connect(&mut project, (pmos, "drain"), (output, "in"));
        project
    }

    fn two_stage_inverter() -> Project {
        let mut project = Project::default();
        project.rename("Two stage inverter".into()).unwrap();
        let vdd = project.add_component("vdd", 0.0, -20.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 20.0).unwrap();
        let input = project.add_component("input", -20.0, 0.0).unwrap();
        let output = project.add_component("output", 20.0, 0.0).unwrap();
        let p1 = project.add_component("pmos", -5.0, -5.0).unwrap();
        let n1 = project.add_component("nmos", -5.0, 5.0).unwrap();
        let p2 = project.add_component("pmos", 5.0, -5.0).unwrap();
        let n2 = project.add_component("nmos", 5.0, 5.0).unwrap();
        for pmos in [p1, p2] {
            connect(&mut project, (vdd, "out"), (pmos, "source"));
        }
        for nmos in [n1, n2] {
            connect(&mut project, (gnd, "out"), (nmos, "source"));
        }
        connect(&mut project, (input, "out"), (p1, "gate"));
        connect(&mut project, (input, "out"), (n1, "gate"));
        connect(&mut project, (p1, "drain"), (n1, "drain"));
        connect(&mut project, (p1, "drain"), (p2, "gate"));
        connect(&mut project, (p1, "drain"), (n2, "gate"));
        connect(&mut project, (p2, "drain"), (n2, "drain"));
        connect(&mut project, (p2, "drain"), (output, "in"));
        project
    }

    fn timing_acceptance_fixture() -> Project {
        let mut project = Project::default();
        project.rename("Timing acceptance".into()).unwrap();
        let vdd = project.add_component("vdd", 0.0, -20.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 20.0).unwrap();

        // Independent loads make a compact baseline non-trivial while the
        // four-stage path below remains the unique timing-critical path.
        for _ in 0..12 {
            let input = project.add_component("input", 0.0, 0.0).unwrap();
            let output = project.add_component("output", 0.0, 0.0).unwrap();
            let pmos = project.add_component("pmos", 0.0, 0.0).unwrap();
            let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
            connect(&mut project, (vdd, "out"), (pmos, "source"));
            connect(&mut project, (gnd, "out"), (nmos, "source"));
            connect(&mut project, (input, "out"), (pmos, "gate"));
            connect(&mut project, (input, "out"), (nmos, "gate"));
            connect(&mut project, (pmos, "drain"), (nmos, "drain"));
            connect(&mut project, (pmos, "drain"), (output, "in"));
        }

        let input = project.add_component("input", 0.0, 0.0).unwrap();
        let output = project.add_component("output", 0.0, 0.0).unwrap();
        let mut stages = Vec::new();
        for _ in 0..4 {
            let pmos = project.add_component("pmos", 0.0, 0.0).unwrap();
            let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
            connect(&mut project, (vdd, "out"), (pmos, "source"));
            connect(&mut project, (gnd, "out"), (nmos, "source"));
            connect(&mut project, (pmos, "drain"), (nmos, "drain"));
            stages.push((pmos, nmos));
        }
        connect(&mut project, (input, "out"), (stages[0].0, "gate"));
        connect(&mut project, (input, "out"), (stages[0].1, "gate"));
        for pair in stages.windows(2) {
            connect(&mut project, (pair[0].0, "drain"), (pair[1].0, "gate"));
            connect(&mut project, (pair[0].0, "drain"), (pair[1].1, "gate"));
        }
        connect(
            &mut project,
            (stages.last().unwrap().0, "drain"),
            (output, "in"),
        );
        project
    }

    fn hierarchical_inverter() -> Project {
        let mut leaf = inverter();
        let definition = leaf.capture_block("INV".into()).unwrap();
        let mut parent = Project::default();
        parent.block_definitions = leaf.block_definitions;
        let vdd = parent.add_component("vdd", 0.0, -10.0).unwrap();
        let gnd = parent.add_component("gnd", 0.0, 10.0).unwrap();
        let input = parent.add_component("input", -10.0, 0.0).unwrap();
        let output = parent.add_component("output", 10.0, 0.0).unwrap();
        let instance = parent.place_block(definition, 0.0, 0.0).unwrap();
        connect(&mut parent, (vdd, "out"), (instance, "VDD1"));
        connect(&mut parent, (gnd, "out"), (instance, "GND1"));
        connect(&mut parent, (input, "out"), (instance, "IN1"));
        connect(&mut parent, (instance, "OUT1"), (output, "in"));
        parent
    }

    fn hierarchical_inverter_pair() -> Project {
        let mut leaf = inverter();
        let definition = leaf.capture_block("INV".into()).unwrap();
        let mut parent = Project::default();
        parent.block_definitions = leaf.block_definitions;
        let vdd = parent.add_component("vdd", 0.0, -10.0).unwrap();
        let gnd = parent.add_component("gnd", 0.0, 10.0).unwrap();
        let input = parent.add_component("input", -10.0, 0.0).unwrap();
        let output = parent.add_component("output", 10.0, 0.0).unwrap();
        let first = parent.place_block(definition, -5.0, 0.0).unwrap();
        let second = parent.place_block(definition, 5.0, 0.0).unwrap();
        for instance in [first, second] {
            connect(&mut parent, (vdd, "out"), (instance, "VDD1"));
            connect(&mut parent, (gnd, "out"), (instance, "GND1"));
        }
        connect(&mut parent, (input, "out"), (first, "IN1"));
        connect(&mut parent, (first, "OUT1"), (second, "IN1"));
        connect(&mut parent, (second, "OUT1"), (output, "in"));
        parent
    }

    #[test]
    fn generated_inverter_passes_educational_physical_drc() {
        let project = inverter();
        let ir = normalize(&project).unwrap();
        assert!(ir.tapeout.fits);
        assert_eq!(ir.tapeout.shapes_outside_tapeout, 0);
        assert_eq!(
            (ir.tapeout.width_um, ir.tapeout.height_um),
            (2_920.0, 3_520.0)
        );
        let report = physical_drc::validate(&ir, &Technology::default());
        assert_eq!(
            report.error_count, 0,
            "unexpected physical DRC diagnostics: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn tapeout_report_distinguishes_floorplan_escape_from_window_escape() {
        let project = inverter();
        let normal = normalize(&project).unwrap();
        assert!(normal.tapeout.fits);

        let mut tiny = project;
        tiny.technology.tapeout_window.width_um = 1.0;
        tiny.technology.tapeout_window.height_um = 1.0;
        let report = normalize(&tiny).unwrap().tapeout;
        assert!(!report.fits);
        assert!(report.shapes_outside_tapeout > 0);
        assert!(report.geometry_width_um > report.usable_width_um);
        assert!(report.geometry_height_um > report.usable_height_um);
    }

    #[test]
    fn physical_timing_exposes_routed_parasitics_and_a_deterministic_critical_net() {
        let project = inverter();
        let ir = normalize(&project).unwrap();
        assert_eq!(ir.timing.nets.len(), ir.nets.len());
        assert!(ir.timing.critical_net.is_some());
        assert!(ir.timing.critical_net_name.is_some());
        assert_eq!(ir.timing.paths.len(), 1);
        assert_eq!(ir.timing.paths[0].input_pin, "IN1");
        assert_eq!(ir.timing.paths[0].output_pin, "OUT1");
        assert_eq!(ir.timing.paths[0].nets.len(), 2);
        assert_eq!(ir.timing.paths[0].device_names.len(), 2);
        assert_eq!(ir.timing.critical_path, Some(0));
        assert!(ir.timing.estimated_worst_delay_ns.is_finite());
        assert!(ir.timing.estimated_worst_delay_ns >= 0.0);
        assert!(ir
            .timing
            .nets
            .windows(2)
            .all(|pair| pair[0].net < pair[1].net));
        assert!(ir.timing.nets.iter().any(|metric| {
            metric.wire_length_um > 0.0
                && metric.routed_capacitance_ff > 0.0
                && metric.total_capacitance_ff >= metric.routed_capacitance_ff
        }));
        assert_eq!(ir, normalize(&project).unwrap());

        let mut higher_capacitance = project.clone();
        higher_capacitance
            .technology
            .physical_parasitics
            .wire_capacitance_ff_per_um *= 2.0;
        higher_capacitance
            .technology
            .physical_parasitics
            .layer_capacitance_ff_per_um
            .clear();
        let higher = normalize(&higher_capacitance).unwrap();
        assert!(higher
            .timing
            .nets
            .iter()
            .zip(&ir.timing.nets)
            .any(|(high, base)| { high.routed_capacitance_ff > base.routed_capacitance_ff }));
    }

    #[test]
    fn timing_path_graph_accumulates_cascaded_stage_delays() {
        let project = two_stage_inverter();
        let ir = normalize(&project).unwrap();
        assert_eq!(ir.timing.paths.len(), 1);
        let path = &ir.timing.paths[0];
        assert_eq!(
            (path.input_pin.as_str(), path.output_pin.as_str()),
            ("IN1", "OUT1")
        );
        assert_eq!(path.nets.len(), 3);
        assert_eq!(path.device_ids.len(), 4);
        let expected = path
            .nets
            .iter()
            .skip(1)
            .map(|net| ir.timing.nets[*net].estimated_delay_ns)
            .sum::<f64>();
        assert!((path.estimated_delay_ns - expected).abs() < 1e-12);
        assert_eq!(ir.timing.critical_path, Some(0));
        assert_eq!(ir.timing.timing_target_ns, None);
        assert_eq!(ir.timing.worst_slack_ns, None);
        assert_eq!(ir, normalize(&project).unwrap());

        let mut constrained = project.clone();
        constrained
            .set_timing_target(Some(path.estimated_delay_ns + 0.1))
            .unwrap();
        let met = normalize(&constrained).unwrap();
        assert!(met.timing.worst_slack_ns.unwrap() > 0.0);
        assert_eq!(
            met.timing.paths[0].required_time_ns,
            constrained.timing_target_ns
        );
        constrained
            .set_timing_target(Some(path.estimated_delay_ns / 2.0))
            .unwrap();
        assert!(
            normalize(&constrained)
                .unwrap()
                .timing
                .worst_slack_ns
                .unwrap()
                < 0.0
        );
    }

    #[test]
    fn routed_floorplan_candidates_publish_deterministic_timing_scores() {
        let ir = normalize(&nand()).unwrap();
        assert_eq!(ir.timing.candidates.len(), 3);
        assert!(ir
            .timing
            .candidates
            .windows(2)
            .all(|pair| pair[0].candidate < pair[1].candidate));
        assert!(ir.timing.candidates.iter().all(|candidate| {
            candidate.estimated_worst_delay_ns.is_finite()
                && candidate.area_um2 > 0.0
                && candidate.total_wire_length_um >= 0.0
                && candidate.slack_ns.is_none()
                && candidate.meets_timing.is_none()
        }));
        let selected = ir
            .timing
            .candidates
            .iter()
            .find(|candidate| Some(candidate.candidate) == ir.timing.selected_candidate)
            .unwrap();
        assert_eq!(selected.floorplan_candidate, ir.planning.selected_candidate);
        assert_eq!(
            selected.placement_candidate,
            ir.placement.selected_candidate
        );
        assert_eq!(selected.routing_overflow, ir.global_routing.total_overflow);
        assert_eq!(
            selected.detail_conflicts,
            ir.detailed_routing.conflict_count
        );
        assert_eq!(
            selected.total_via_count,
            ir.detailed_routing.total_via_count
        );
        assert!(
            (selected.total_wire_length_um - ir.detailed_routing.total_wire_length_um).abs() < 1e-9
        );
    }

    #[test]
    fn violated_timing_target_preserves_legality_before_delay() {
        let mut project = timing_acceptance_fixture();
        project.set_timing_target(Some(1e-12)).unwrap();
        let ir = normalize(&project).unwrap();
        assert_no_cross_net_metal_overlaps(&ir);
        let metal_spacing = physical_drc::validate(&ir, &project.technology)
            .diagnostics
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.rule_id == "GEOMETRY.MIN_SPACING"
                    && diagnostic.layer.starts_with("metal")
            })
            .collect::<Vec<_>>();
        assert!(
            metal_spacing.is_empty(),
            "timing-driven preview has same-layer spacing conflicts: {metal_spacing:?}"
        );
        assert!(
            ir.timing
                .candidates
                .iter()
                .any(|candidate| candidate.timing_driven),
            "a constrained fixture should retain at least one legal timing-driven candidate: {:?}",
            ir.timing.candidates
        );
        let selected = ir
            .timing
            .candidates
            .iter()
            .find(|candidate| Some(candidate.candidate) == ir.timing.selected_candidate)
            .unwrap();
        let best_baseline = ir
            .timing
            .candidates
            .iter()
            .filter(|candidate| !candidate.timing_driven)
            .min_by(|left, right| {
                (left.routing_overflow, left.detail_conflicts)
                    .cmp(&(right.routing_overflow, right.detail_conflicts))
                    .then_with(|| {
                        left.estimated_worst_delay_ns
                            .total_cmp(&right.estimated_worst_delay_ns)
                    })
            })
            .unwrap();
        assert!(
            (selected.routing_overflow, selected.detail_conflicts)
                <= (
                    best_baseline.routing_overflow,
                    best_baseline.detail_conflicts
                )
        );
        assert_eq!(selected.meets_timing, Some(false));
        assert!(ir.timing.candidates.iter().all(|candidate| {
            candidate.routing_overflow > selected.routing_overflow
                || (candidate.routing_overflow == selected.routing_overflow
                    && candidate.detail_conflicts > selected.detail_conflicts)
                || (candidate.routing_overflow == selected.routing_overflow
                    && candidate.detail_conflicts == selected.detail_conflicts
                    && selected.estimated_worst_delay_ns
                        <= candidate.estimated_worst_delay_ns + 1e-12)
        }));
    }

    #[test]
    fn relaxed_and_hierarchical_timing_acceptance_is_compact_and_deterministic() {
        let relaxed = timing_acceptance_fixture();
        let first = normalize(&relaxed).unwrap();
        assert_eq!(first.timing.candidates.len(), 3);
        assert!(first
            .timing
            .candidates
            .iter()
            .all(|candidate| !candidate.timing_driven));
        assert_eq!(first, normalize(&relaxed).unwrap());

        let mut hierarchical = hierarchical_inverter();
        hierarchical.set_timing_target(Some(0.2)).unwrap();
        let restored: Project =
            serde_json::from_str(&serde_json::to_string(&hierarchical).unwrap()).unwrap();
        let hierarchical_ir = normalize(&hierarchical.flattened().unwrap()).unwrap();
        assert_eq!(hierarchical_ir.devices.len(), 2);
        assert!(hierarchical_ir
            .timing
            .candidates
            .iter()
            .any(|candidate| candidate.timing_driven));
        assert_eq!(
            hierarchical_ir,
            normalize(&restored.flattened().unwrap()).unwrap()
        );
    }

    #[test]
    fn physical_plan_exposes_resources_priorities_and_bounded_candidates() {
        let ir = normalize(&nand()).unwrap();
        assert_eq!(ir.planning.routing_layers.len(), 5);
        assert_eq!(ir.planning.candidates.len(), 3);
        assert!(ir
            .planning
            .nets
            .windows(2)
            .all(|pair| pair[0].priority <= pair[1].priority));
        assert!(ir
            .planning
            .nets
            .iter()
            .take(2)
            .all(|net| net.class == crate::physical_planning::PhysicalNetClass::Power));
        let selected = &ir.planning.candidates[ir.planning.selected_candidate];
        assert!(selected.feasible);
        assert!(selected.device_density <= ir.planning.target_device_density + 1e-9);
        assert!(
            selected.estimated_routing_utilization <= ir.planning.target_routing_utilization + 1e-9
        );
        assert!(selected.growth_passes <= ir.planning.max_floorplan_growth_passes);
        assert_eq!(
            ir.planning.routing_bins.len(),
            ir.planning.bin_columns * ir.planning.bin_rows
        );
        assert!(ir.planning.routing_bins.iter().all(|bin| {
            bin.layer_capacities.len() == ir.planning.routing_layers.len()
                && bin
                    .layer_capacities
                    .iter()
                    .any(|capacity| capacity.horizontal_tracks > 0 || capacity.vertical_tracks > 0)
        }));
        assert!((ir.bounds.max_x - ir.bounds.min_x - selected.width_um).abs() < 1e-9);
        assert!((ir.bounds.max_y - ir.bounds.min_y - selected.height_um).abs() < 1e-9);
    }

    #[test]
    fn global_router_prioritizes_nets_and_emits_adjacent_capacity_guides() {
        let ir = normalize(&nand()).unwrap();
        assert!(ir
            .global_routing
            .routes
            .windows(2)
            .all(|pair| pair[0].priority <= pair[1].priority));
        assert!(
            ir.global_routing.iterations.len() <= usize::from(ir.global_routing.max_iterations) + 1
        );
        for route in &ir.global_routing.routes {
            for segment in &route.segments {
                let from = &ir.planning.routing_bins[segment.from_bin];
                let to = &ir.planning.routing_bins[segment.to_bin];
                assert_eq!(
                    from.column.abs_diff(to.column) + from.row.abs_diff(to.row),
                    1
                );
                assert!((1..=ir.max_metal_layers).contains(&segment.layer));
            }
        }
    }

    #[test]
    fn global_router_bounds_negotiation_when_capacity_cannot_converge() {
        let mut project = nand();
        for layer in project
            .technology
            .physical_planning
            .routing_layers
            .values_mut()
        {
            layer.capacity_adjustment = 0.01;
        }
        project
            .technology
            .physical_planning
            .global_route_max_iterations = 8;
        project
            .technology
            .physical_planning
            .global_route_stall_iterations = 2;
        let ir = normalize(&project).unwrap();
        assert!(!ir.global_routing.converged);
        assert!(ir.global_routing.total_overflow > 0);
        assert!(ir.global_routing.iterations.len() <= 9);
        assert!(ir.global_routing.iterations.len() >= 2);
        assert!(ir
            .global_routing
            .iterations
            .iter()
            .skip(1)
            .any(|iteration| iteration.rerouted_nets > 0));
    }

    #[test]
    fn detailed_router_emits_grid_polygons_and_accounts_for_pin_access() {
        let technology = Technology::default();
        let grid = technology.physical_rules.manufacturing_grid_um;
        let ir = normalize(&nand()).unwrap();
        assert!(!ir.detailed_routing.routes.is_empty());
        assert!(
            ir.detailed_routing.converged,
            "NAND detailed route did not converge: {:?}",
            ir.detailed_routing.iterations
        );
        assert!(
            ir.detailed_routing.iterations.len()
                <= usize::from(ir.detailed_routing.max_iterations) + 1
        );
        assert_eq!(
            ir.detailed_routing.blocked_pin_count,
            ir.detailed_routing
                .routes
                .iter()
                .map(|route| route.blocked_pin_access_points)
                .sum::<usize>()
        );
        let selected_placement = &ir.placement.candidates[ir.placement.selected_candidate];
        assert_eq!(
            ir.detailed_routing.seeded_device_shape_count,
            selected_placement.reserved_device_shapes
        );
        assert_eq!(
            ir.detailed_routing.track_retry_count,
            ir.detailed_routing
                .routes
                .iter()
                .map(|route| route.track_retry_count)
                .sum::<usize>()
        );
        assert_eq!(
            ir.detailed_routing.layer_escalation_count,
            ir.detailed_routing
                .routes
                .iter()
                .map(|route| route.layer_escalation_count)
                .sum::<usize>()
        );
        let detailed_shapes = ir
            .detailed_routing
            .routes
            .iter()
            .flat_map(|route| route.polygons.iter())
            .collect::<Vec<_>>();
        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        for route in &ir.detailed_routing.routes {
            assert!(route.pin_access_points > 0);
            for (shape_index, polygon) in route.polygons.iter().enumerate() {
                assert_eq!(polygon.net, Some(route.net));
                if let PhysicalLayer::Via(lower) = polygon.layer {
                    let enclosure = technology.physical_rules.via.enclosure_um;
                    for layer in [PhysicalLayer::Metal(lower), PhysicalLayer::Metal(lower + 1)] {
                        assert!(
                            detailed_shapes.iter().any(|metal| {
                                metal.layer == layer
                                    && metal.net == polygon.net
                                    && metal.x - metal.width / 2.0
                                        <= polygon.x - polygon.width / 2.0 - enclosure + 1e-9
                                    && metal.x + metal.width / 2.0
                                        >= polygon.x + polygon.width / 2.0 + enclosure - 1e-9
                                    && metal.y - metal.height / 2.0
                                        <= polygon.y - polygon.height / 2.0 - enclosure + 1e-9
                                    && metal.y + metal.height / 2.0
                                        >= polygon.y + polygon.height / 2.0 + enclosure - 1e-9
                            }),
                            "accepted via lacks its atomic landing on {:?}",
                            layer
                        );
                    }
                }
                canvas
                    .commit_routing_geometry(
                        std::slice::from_ref(polygon),
                        format!("accepted-net-{}-shape-{shape_index}", route.net),
                    )
                    .expect("detailed IR contains only canvas-accepted routing geometry");
                for edge in [
                    polygon.x - polygon.width / 2.0,
                    polygon.x + polygon.width / 2.0,
                    polygon.y - polygon.height / 2.0,
                    polygon.y + polygon.height / 2.0,
                ] {
                    assert!((edge / grid - (edge / grid).round()).abs() < 1e-7);
                }
            }
        }
        assert!(
            (ir.detailed_routing.total_wire_length_um
                - ir.detailed_routing
                    .routes
                    .iter()
                    .map(|route| route.wire_length_um)
                    .sum::<f64>())
            .abs()
                < 1e-7
        );
    }

    #[test]
    fn placement_candidates_are_legal_scored_and_consumed_by_geometry() {
        let ir = normalize(&nand()).unwrap();
        assert_eq!(ir.placement.candidates.len(), 3);
        assert!(ir
            .placement
            .candidates
            .iter()
            .all(|candidate| candidate.legal));
        let selected = &ir.placement.candidates[ir.placement.selected_candidate];
        assert!(ir
            .placement
            .candidates
            .iter()
            .all(|candidate| selected.score <= candidate.score + 1e-9));
        let mut occupied = HashSet::new();
        let grid = Technology::default().physical_rules.manufacturing_grid_um;
        for placed in &selected.devices {
            assert!(
                (placed.x / ir.planning.placement_site_width_um
                    - (placed.x / ir.planning.placement_site_width_um).round())
                .abs()
                    < 1e-9
            );
            assert!(occupied.insert((placed.kind, placed.row, placed.site)));
            let diffusion = ir
                .shapes
                .iter()
                .find(|shape| {
                    shape.component_id == Some(placed.component_id)
                        && matches!(shape.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff)
                })
                .unwrap();
            assert!((diffusion.x - placed.x).abs() <= grid / 2.0 + 1e-9);
            assert!((diffusion.y - placed.y).abs() <= grid / 2.0 + 1e-9);
        }
    }

    fn nand() -> Project {
        let mut project = Project::default();
        let vdd = project.add_component("vdd", 0.0, -5.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 5.0).unwrap();
        let input_a = project.add_component("input", -5.0, -1.0).unwrap();
        let input_b = project.add_component("input", -5.0, 1.0).unwrap();
        let output = project.add_component("output", 5.0, 0.0).unwrap();
        let pmos_a = project.add_component("pmos", -1.0, -2.0).unwrap();
        let pmos_b = project.add_component("pmos", 1.0, -2.0).unwrap();
        let nmos_a = project.add_component("nmos", 0.0, 1.0).unwrap();
        let nmos_b = project.add_component("nmos", 0.0, 3.0).unwrap();
        connect(&mut project, (vdd, "out"), (pmos_a, "source"));
        connect(&mut project, (vdd, "out"), (pmos_b, "source"));
        connect(&mut project, (pmos_a, "drain"), (output, "in"));
        connect(&mut project, (pmos_b, "drain"), (output, "in"));
        connect(&mut project, (nmos_a, "drain"), (output, "in"));
        connect(&mut project, (nmos_a, "source"), (nmos_b, "drain"));
        connect(&mut project, (nmos_b, "source"), (gnd, "out"));
        connect(&mut project, (input_a, "out"), (pmos_a, "gate"));
        connect(&mut project, (input_a, "out"), (nmos_a, "gate"));
        connect(&mut project, (input_b, "out"), (pmos_b, "gate"));
        connect(&mut project, (input_b, "out"), (nmos_b, "gate"));
        project
    }

    #[test]
    fn generated_nand_passes_educational_physical_drc() {
        let project = nand();
        let ir = normalize(&project).unwrap();
        let report = physical_drc::validate(&ir, &Technology::default());
        assert_eq!(
            report.error_count, 0,
            "unexpected physical DRC diagnostics: {:?}",
            report.diagnostics
        );
    }

    fn assert_no_cross_net_metal_overlaps(ir: &super::PhysicalLayoutIr) {
        for layer_index in 1..=ir.max_metal_layers {
            let layer = PhysicalLayer::Metal(layer_index);
            let shapes = ir
                .shapes
                .iter()
                .filter(|shape| shape.layer == layer && shape.net.is_some())
                .collect::<Vec<_>>();
            for left_index in 0..shapes.len() {
                for right in shapes.iter().skip(left_index + 1) {
                    let left = shapes[left_index];
                    if left.net == right.net {
                        continue;
                    }
                    let overlap_x = (left.x + left.width / 2.0).min(right.x + right.width / 2.0)
                        - (left.x - left.width / 2.0).max(right.x - right.width / 2.0);
                    let overlap_y = (left.y + left.height / 2.0).min(right.y + right.height / 2.0)
                        - (left.y - left.height / 2.0).max(right.y - right.height / 2.0);
                    assert!(
                        overlap_x <= 1e-6 || overlap_y <= 1e-6,
                        "{layer:?} shapes for nets {:?} ({},{},{},{}) and {:?} ({},{},{},{}) overlap",
                        left.net,
                        left.x,
                        left.y,
                        left.width,
                        left.height,
                        right.net,
                        right.x,
                        right.y,
                        right.width,
                        right.height
                    );
                }
            }
        }
    }

    #[test]
    fn inverter_normalizes_to_devices_nets_and_pins() {
        let ir = normalize(&inverter()).unwrap();
        assert_eq!(ir.devices.len(), 2);
        assert_eq!(ir.nets.len(), 4);
        assert_eq!(ir.pins.len(), 4);
        assert_eq!(ir.devices[0].kind, DeviceKind::Nmos);
        assert!(ir.nets.iter().any(|net| net.role == NetRole::Power));
        assert!(ir.nets.iter().any(|net| net.role == NetRole::Ground));
        assert!(ir.nets.iter().any(|net| net.role == NetRole::Input));
        assert!(ir.nets.iter().any(|net| net.role == NetRole::Output));
    }

    #[test]
    fn schematic_movement_does_not_change_normalized_connectivity() {
        let project = inverter();
        let mut moved = project.clone();
        for (index, component) in moved.components.iter_mut().enumerate() {
            component.position.x = index as f64 * 113.0;
            component.position.y = index as f64 * -79.0;
            component.rotation = std::f64::consts::FRAC_PI_2;
        }
        assert_eq!(normalize(&project).unwrap(), normalize(&moved).unwrap());
    }

    #[test]
    fn wire_order_and_direction_do_not_change_normalized_connectivity() {
        let project = inverter();
        let mut redrawn = project.clone();
        redrawn.wires.reverse();
        for wire in &mut redrawn.wires {
            let to = wire.to.take().unwrap();
            let from = std::mem::replace(&mut wire.from, to);
            wire.to = Some(from);
        }
        assert_eq!(normalize(&project).unwrap(), normalize(&redrawn).unwrap());
    }

    #[test]
    fn nand_uses_directional_metals_without_same_layer_cross_net_overlaps() {
        let ir = normalize(&nand()).unwrap();
        assert_eq!(ir.max_metal_layers, 5);
        let routed_signal_nets = ir
            .shapes
            .iter()
            .filter_map(|shape| match (shape.layer, shape.net) {
                (PhysicalLayer::Metal(layer @ 2..), Some(net)) => Some((net, layer)),
                _ => None,
            })
            .collect::<HashSet<_>>();
        assert!(!routed_signal_nets.is_empty());
        assert!(ir
            .shapes
            .iter()
            .any(|shape| shape.layer == PhysicalLayer::Metal(1)));
        for (net, metal_layer) in routed_signal_nets {
            for lower_layer in 1..metal_layer {
                assert!(ir.shapes.iter().any(|shape| {
                    shape.layer == PhysicalLayer::Via(lower_layer) && shape.net == Some(net)
                }));
            }
        }
        assert_no_cross_net_metal_overlaps(&ir);

        let mut track_positions = ir
            .shapes
            .iter()
            .filter(|shape| {
                shape.layer == PhysicalLayer::Metal(ir.max_metal_layers)
                    && shape.net.is_some()
                    && shape.height <= 0.21
            })
            .map(|shape| shape.y)
            .collect::<Vec<_>>();
        track_positions.sort_by(f64::total_cmp);
        track_positions.dedup_by(|left, right| (*left - *right).abs() < 1e-6);
        let gaps = track_positions
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect::<Vec<_>>();
        assert!(!gaps.is_empty());
        assert!(gaps
            .iter()
            .all(|gap| *gap + 1e-6 >= ROUTING_LANDING_SIZE + ROUTING_CLEARANCE));
        assert!(
            gaps.iter().any(|gap| *gap < 0.34),
            "routing lanes should use landing-size clearance rather than the old 0.42 pitch"
        );
    }

    #[test]
    fn routing_does_not_exceed_the_selected_technology_layer_limit() {
        let mut project = nand();
        project.technology.max_metal_layers = 2;
        let ir = normalize(&project).unwrap();
        assert_eq!(ir.max_metal_layers, 2);
        assert!(ir.shapes.iter().all(|shape| match shape.layer {
            PhysicalLayer::Metal(layer) => layer <= 2,
            PhysicalLayer::Via(lower_layer) => lower_layer < 2,
            _ => true,
        }));
    }

    #[test]
    fn large_device_arrays_fold_into_a_compact_footprint() {
        let mut project = Project::default();
        let input = project.add_component("input", 0.0, 0.0).unwrap();
        let output = project.add_component("output", 0.0, 0.0).unwrap();
        let vdd = project.add_component("vdd", 0.0, 0.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 0.0).unwrap();
        for _ in 0..64 {
            let pmos = project.add_component("pmos", 0.0, 0.0).unwrap();
            let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
            connect(&mut project, (input, "out"), (pmos, "gate"));
            connect(&mut project, (input, "out"), (nmos, "gate"));
            connect(&mut project, (vdd, "out"), (pmos, "source"));
            connect(&mut project, (gnd, "out"), (nmos, "source"));
            connect(&mut project, (output, "in"), (pmos, "drain"));
            connect(&mut project, (output, "in"), (nmos, "drain"));
        }
        let ir = normalize(&project).unwrap();
        let width = ir.bounds.max_x - ir.bounds.min_x;
        let height = ir.bounds.max_y - ir.bounds.min_y;
        let aspect_ratio = width.max(height) / width.min(height);

        assert_eq!(ir.devices.len(), 128);
        assert!(
            ir.detailed_routing.track_retry_count > 0
                || ir.detailed_routing.layer_escalation_count > 0,
            "dense routing should exercise indexed track or layer alternatives"
        );
        assert!(
            aspect_ratio < 3.5,
            "large arrays should remain folded instead of becoming a device line, got {width} × {height}"
        );
        let distinct_device_rows = ir
            .shapes
            .iter()
            .filter(|shape| {
                shape.layer == PhysicalLayer::Pdiff || shape.layer == PhysicalLayer::Ndiff
            })
            .map(|shape| (shape.y * 100.0).round() as i64)
            .collect::<HashSet<_>>();
        assert!(distinct_device_rows.len() > 2);
        assert_no_cross_net_metal_overlaps(&ir);
        let physical_report = physical_drc::validate(&ir, &project.technology);
        let metal_spacing = physical_report
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.rule_id == "GEOMETRY.MIN_SPACING"
                    && diagnostic.layer.starts_with("metal")
            })
            .collect::<Vec<_>>();
        assert!(
            metal_spacing.is_empty(),
            "large layout has metal spacing conflicts: {:?}",
            metal_spacing
        );
        let device_spacing = physical_report
            .diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.rule_id == "GEOMETRY.MIN_SPACING"
                    && matches!(
                        diagnostic.layer.as_str(),
                        "ndiff" | "pdiff" | "poly" | "contact"
                    )
            })
            .collect::<Vec<_>>();
        assert!(
            device_spacing.is_empty(),
            "reserved device footprints reached physical IR with spacing conflicts: {:?}",
            device_spacing
        );
        let well = ir
            .shapes
            .iter()
            .find(|shape| shape.layer == PhysicalLayer::Nwell)
            .unwrap();
        let enclosure = project.technology.physical_rules.well_enclosure_um;
        for shape in ir.shapes.iter().filter(|shape| {
            matches!(
                shape.layer,
                PhysicalLayer::Pdiff
                    | PhysicalLayer::Ndiff
                    | PhysicalLayer::Poly
                    | PhysicalLayer::Contact
            )
        }) {
            assert!(
                shape.x - shape.width / 2.0 >= ir.bounds.min_x - 1e-9
                    && shape.x + shape.width / 2.0 <= ir.bounds.max_x + 1e-9
                    && shape.y - shape.height / 2.0 >= ir.bounds.min_y - 1e-9
                    && shape.y + shape.height / 2.0 <= ir.bounds.max_y + 1e-9,
                "{:?} escaped planned bounds: {:?}",
                shape.layer,
                shape
            );
            if shape.layer == PhysicalLayer::Pdiff {
                assert!(
                    well.x - well.width / 2.0 <= shape.x - shape.width / 2.0 - enclosure + 1e-9
                        && well.x + well.width / 2.0
                            >= shape.x + shape.width / 2.0 + enclosure - 1e-9
                        && well.y - well.height / 2.0
                            <= shape.y - shape.height / 2.0 - enclosure + 1e-9
                        && well.y + well.height / 2.0
                            >= shape.y + shape.height / 2.0 + enclosure - 1e-9,
                    "P diffusion escaped N-well enclosure: {:?}, well: {:?}",
                    shape,
                    well
                );
            }
        }
    }

    #[test]
    fn many_independent_nets_coalesce_spacing_fragments_by_conflicting_net_pair() {
        let mut project = Project::default();
        let vdd = project.add_component("vdd", 0.0, 0.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 0.0).unwrap();
        for _ in 0..32 {
            let input = project.add_component("input", 0.0, 0.0).unwrap();
            let output = project.add_component("output", 0.0, 0.0).unwrap();
            let pmos = project.add_component("pmos", 0.0, 0.0).unwrap();
            let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
            connect(&mut project, (input, "out"), (pmos, "gate"));
            connect(&mut project, (input, "out"), (nmos, "gate"));
            connect(&mut project, (vdd, "out"), (pmos, "source"));
            connect(&mut project, (gnd, "out"), (nmos, "source"));
            connect(&mut project, (output, "in"), (pmos, "drain"));
            connect(&mut project, (output, "in"), (nmos, "drain"));
        }
        let ir = normalize(&project).unwrap();
        let report = physical_drc::validate(&ir, &project.technology);
        let mut counts = std::collections::BTreeMap::new();
        for diagnostic in &report.diagnostics {
            *counts
                .entry((diagnostic.rule_id.as_str(), diagnostic.layer.as_str()))
                .or_insert(0usize) += 1;
        }
        assert!(
            report.error_count < 100,
            "pairwise fragments should coalesce into actionable conflicts: {counts:?}"
        );
        assert_no_cross_net_metal_overlaps(&ir);
        assert!(report.diagnostics.iter().all(|diagnostic| {
            diagnostic.rule_id != "GEOMETRY.MIN_SPACING" || !diagnostic.layer.starts_with("metal")
        }));
        let selected_score = (
            ir.global_routing.total_overflow,
            ir.detailed_routing.conflict_count,
        );
        for selected in 0..ir.planning.candidates.len() {
            let mut planning = ir.planning.clone();
            planning.selected_candidate = selected;
            let placement = crate::physical_placement::place(
                &ir.devices,
                &ir.nets,
                &project.technology,
                &planning,
            );
            let global =
                crate::physical_global_routing::route(&ir.nets, &ir.pins, &placement, &planning);
            let detail = crate::physical_detailed_routing::route(
                &ir.devices,
                &ir.nets,
                &ir.pins,
                &global,
                &planning,
                &project.technology,
                &placement,
            );
            assert!(
                selected_score <= (global.total_overflow, detail.conflict_count),
                "selected physical candidate must minimize routing failure before shape preference"
            );
        }
    }

    #[test]
    fn hierarchy_candidate_freezes_reusable_instances_into_regions() {
        let project = hierarchical_inverter_pair();
        let flattened = project.flattened().unwrap();
        let ir = normalize(&flattened).unwrap();
        assert!(ir
            .devices
            .iter()
            .all(|device| device.physical_group.is_some()));
        let selected = &ir.placement.candidates[ir.placement.selected_candidate];
        assert_eq!(
            selected.strategy,
            crate::physical_placement::PlacementStrategy::Hierarchy
        );
        assert_eq!(selected.block_regions.len(), 2);
        assert!(selected.block_regions.iter().all(|region| region.immutable));
        assert!(selected
            .block_regions
            .iter()
            .all(|region| region.device_count == 2));
        assert_eq!(ir.physical_blocks.len(), 2);
        assert!(ir.physical_blocks.iter().all(|block| {
            block.immutable
                && block.verified
                && block.device_ids.len() == 2
                && !block.interface_pins.is_empty()
                && !block.shape_indices.is_empty()
        }));
        let private_nets = ir
            .physical_blocks
            .iter()
            .flat_map(|block| block.local_nets.iter().copied())
            .collect::<HashSet<_>>();
        assert!(ir
            .global_routing
            .routes
            .iter()
            .all(|route| !private_nets.contains(&route.net)));
        let power = ir
            .nets
            .iter()
            .find(|net| net.role == NetRole::Power)
            .unwrap()
            .id;
        assert!(
            ir.shapes
                .iter()
                .filter(|shape| shape.net == Some(power) && shape.layer == PhysicalLayer::Metal(1))
                .count()
                >= 2,
            "each reusable block should retain a local M1 power rail"
        );
        assert!(ir
            .shapes
            .iter()
            .any(|shape| { shape.net == Some(power) && shape.layer == PhysicalLayer::Metal(2) }));
    }

    #[test]
    fn net_labels_merge_logically_identical_nets() {
        let mut project = Project::default();
        let first = project.add_component("net_label", 0.0, 0.0).unwrap();
        let second = project.add_component("net_label", 20.0, 20.0).unwrap();
        project.rename_component(first, "shared".into()).unwrap();
        project
            .rename_component(second, "shared".into())
            .unwrap_err();
        // The editor prevents duplicate component names, so emulate a valid
        // loaded project whose equal labels intentionally identify one net.
        project
            .components
            .iter_mut()
            .find(|component| component.id == second)
            .unwrap()
            .name = "shared".into();
        let ir = normalize(&project).unwrap();
        assert_eq!(ir.nets.len(), 1);
        assert_eq!(ir.nets[0].name, "shared");
    }
}
