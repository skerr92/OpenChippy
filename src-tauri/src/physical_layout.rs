use crate::model::{Component, Project, TerminalRef};
use crate::physical_canvas::{
    device_diffusion_landing_size, device_footprint_with_gate_access, device_gate_access_with_side,
    device_terminal_offset, ObstructionType, PhysicalCanvas,
};
use crate::physical_detailed_routing::{self, DetailedRoutingReport};
use crate::physical_drc;
use crate::physical_global_routing::{self, GlobalRoutingReport};
use crate::physical_placement::{self, PhysicalPlacementReport, PlacedDevice};
use crate::physical_planning::{self, PhysicalPlanningReport};
use crate::standard_cells::{self, LibrarySummary};
use crate::technology::{DensityFillLayerRule, PhysicalRuleDeck, RoutingDirection, Technology};
use rayon::prelude::*;
use serde::{Deserialize, Serialize, Serializer};
use std::cmp::{Ordering, Reverse};
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use uuid::Uuid;

// Version 40 makes standard-cell identity a hard sharing boundary. Same-net
// terminals in separate instances may meet only through routed conductors;
// shared diffusion/contact and poly straps are synthesized solely inside one
// explicit leaf cell. Version 39's independent route anchors remain active.
pub const CURRENT_PHYSICAL_IR_VERSION: u32 = 45;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalLayoutIr {
    pub format_version: u32,
    pub source_project_name: String,
    pub technology_name: String,
    pub technology_fingerprint: String,
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
    #[serde(default)]
    pub row_topology: Vec<PhysicalRowTopology>,
    #[serde(default)]
    pub route_quality: PhysicalRouteQualityReport,
    pub orphan_routing_shapes_removed: usize,
    pub physical_blocks: Vec<PhysicalBlockImplementation>,
    #[serde(default)]
    pub standard_cell_library: LibrarySummary,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalRouteQualityReport {
    pub routed_shape_area_um2: f64,
    pub routing_bbox_area_um2: f64,
    pub max_device_envelope_excursion_um: f64,
    pub max_terminal_envelope_excursion_um: f64,
    pub largest_terminal_excursions: Vec<PhysicalRouteExcursion>,
    pub terminal_free_component_count: usize,
    pub terminal_free_shape_count: usize,
    pub terminal_free_components: Vec<PhysicalRouteQualityComponent>,
    pub exact_duplicate_via_count: usize,
    pub unlanded_via_count: usize,
    pub unjustified_route_endpoint_count: usize,
    pub unjustified_route_endpoints: Vec<PhysicalRouteEndpoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalRouteEndpoint {
    pub shape_index: usize,
    pub net: usize,
    pub layer: String,
    pub purpose: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalRouteQualityComponent {
    pub net: usize,
    pub shape_indices: Vec<usize>,
    pub layers: Vec<String>,
    pub purposes: Vec<String>,
    pub bounds: PhysicalBounds,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalRouteExcursion {
    pub shape_index: usize,
    pub net: Option<usize>,
    pub layer: String,
    pub purpose: String,
    pub excursion_um: f64,
    pub bounds: PhysicalBounds,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalBlockImplementation {
    pub instance_name: String,
    #[serde(default)]
    pub cell_name: String,
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
    /// Lowest reusable instance that can be implemented as a canonical standard cell.
    #[serde(default)]
    pub standard_cell_group: Option<String>,
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
    #[serde(default)]
    pub purpose: PhysicalShapePurpose,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalRowTopology {
    pub kind: DeviceKind,
    pub y: f64,
    pub ordered_devices: Vec<Uuid>,
    pub islands: Vec<PhysicalActiveIsland>,
    #[serde(default)]
    pub gate_straps: Vec<PhysicalGateStrap>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalActiveIsland {
    pub layer: PhysicalLayer,
    pub device_ids: Vec<Uuid>,
    /// Drain-to-source net sequence after row orientation. Adjacent devices in
    /// one island share the repeated boundary net.
    pub terminal_nets: Vec<usize>,
    #[serde(default)]
    pub accesses: Vec<PhysicalTerminalAccess>,
    pub bounds: PhysicalBounds,
    /// Non-overlapping, edge-abutting rectangles whose union is the finalized
    /// active polygon. A single rectangle is retained for equal-width rows;
    /// mixed-width rows use a rectilinear decomposition so each gate keeps its
    /// process-derived channel width.
    #[serde(default)]
    pub geometry: Vec<PhysicalBounds>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalTerminalAccess {
    pub net: usize,
    pub device_ids: Vec<Uuid>,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub shared_contact: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalGateStrap {
    pub net: usize,
    pub device_ids: Vec<Uuid>,
    pub bounds: PhysicalBounds,
    /// Rectilinear conductor rectangles whose union is the finalized shared
    /// gate connection. This is the physical proof of `device_ids`; `bounds`
    /// alone must never be interpreted as connectivity.
    #[serde(default)]
    pub geometry: Vec<PhysicalBounds>,
}

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalShapePurpose {
    /// Compatibility value for Physical IR written before shape provenance.
    #[default]
    Unknown,
    Fabric,
    Active,
    Gate,
    GateAccess,
    Contact,
    DeviceLanding,
    Pin,
    /// A process-wide VDD/GND distribution conductor. This is an external
    /// electrical obligation and must survive terminal-tree pruning.
    PowerRail,
    Route,
    RouteFill,
    /// Process-owned, electrically inert density geometry. This purpose must
    /// never participate in device extraction, routing, or net connectivity.
    DummyFill,
    ViaLanding,
    Via,
    Tap,
}

impl PhysicalShapePurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Fabric => "fabric",
            Self::Active => "active",
            Self::Gate => "gate",
            Self::GateAccess => "gate_access",
            Self::Contact => "contact",
            Self::DeviceLanding => "device_landing",
            Self::Pin => "pin",
            Self::PowerRail => "power_rail",
            Self::Route => "route",
            Self::RouteFill => "route_fill",
            Self::DummyFill => "dummy_fill",
            Self::ViaLanding => "via_landing",
            Self::Via => "via",
            Self::Tap => "tap",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PhysicalLayer {
    Substrate,
    Pwell,
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
            Self::Pwell => "pwell".into(),
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
            "pwell" => Ok(Self::Pwell),
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
    rules: &PhysicalRuleDeck,
) -> PhysicalShape {
    let horizontal = (end.0 - start.0).abs() >= (end.1 - start.1).abs();
    let rule = match layer {
        PhysicalLayer::Metal(index) => rules
            .layer_overrides
            .get(&format!("metal{index}"))
            .unwrap_or(&rules.metal),
        _ => &rules.metal,
    };
    let width = rule.min_width_um;
    let minimum_length = (rule.min_area_um2 / width).max(width) + rules.manufacturing_grid_um * 2.0;
    let routed_length = if horizontal {
        (end.0 - start.0).abs() + width
    } else {
        (end.1 - start.1).abs() + width
    }
    .max(minimum_length);
    PhysicalShape {
        layer,
        x: (start.0 + end.0) / 2.0,
        y: (start.1 + end.1) / 2.0,
        width: if horizontal { routed_length } else { width },
        height: if horizontal { width } else { routed_length },
        component_id: None,
        net,
        purpose: PhysicalShapePurpose::Route,
    }
}

fn power_rail_shape(
    start: (f64, f64),
    end: (f64, f64),
    layer: PhysicalLayer,
    net: Option<usize>,
    rules: &PhysicalRuleDeck,
) -> PhysicalShape {
    let mut shape = line_shape(start, end, layer, net, rules);
    shape.purpose = PhysicalShapePurpose::PowerRail;
    shape
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RouteAnchor {
    point: (f64, f64),
    layer: PhysicalLayer,
    access_bounds: Option<(f64, f64)>,
    component_id: Option<Uuid>,
}

const ROUTING_LANDING_SIZE: f64 = 0.28;
const ROUTING_CLEARANCE: f64 = 0.04;
const ROUTING_SNAP_GUARD: f64 = 0.02;
const LOCAL_ROUTE_MARKER_PREFIX: u128 = 0x0c41_4d31_4d32_0000_0000_0000_0000_0000;
const ACCESS_ROUTE_MARKER_PREFIX: u128 = 0x0acc_e551_0000_0000_0000_0000_0000_0000;
const REPAIR_ANCHOR_PAIR_LIMIT: usize = 32;
const REPAIR_PATH_LIMIT: usize = 192;

pub(crate) fn well_stripe_geometry(
    floorplan_height: f64,
    row_height: f64,
    rules: &PhysicalRuleDeck,
) -> (usize, f64) {
    let minimum = (row_height + 2.0 * rules.well_enclosure_um)
        .max(rules.well.min_width_um)
        .max(rules.manufacturing_grid_um);
    let mut count = (floorplan_height / minimum).floor() as usize;
    count = count.max(2);
    if count % 2 == 1 {
        count = count.saturating_sub(1).max(2);
    }
    (count, floorplan_height / count as f64)
}

fn local_route_marker(net: usize) -> Uuid {
    Uuid::from_u128(LOCAL_ROUTE_MARKER_PREFIX | net as u128)
}

fn access_route_marker(net: usize, component_id: Option<Uuid>, ordinal: usize) -> Uuid {
    let component_salt = component_id
        .map(|id| (id.as_u128() as u32) ^ (ordinal as u32).rotate_left(13))
        .unwrap_or(ordinal as u32);
    Uuid::from_u128(ACCESS_ROUTE_MARKER_PREFIX | ((net as u128) << 32) | component_salt as u128)
}

fn is_local_route_shape(shape: &PhysicalShape) -> bool {
    shape.component_id.is_some_and(|component_id| {
        component_id.as_u128() & 0xffff_ffff_ffff_0000_0000_0000_0000_0000
            == LOCAL_ROUTE_MARKER_PREFIX
    })
}

fn is_access_route_shape(shape: &PhysicalShape) -> bool {
    shape.component_id.is_some_and(|component_id| {
        component_id.as_u128() & 0xffff_ffff_0000_0000_0000_0000_0000_0000
            == ACCESS_ROUTE_MARKER_PREFIX
    })
}

fn is_internal_route_shape(shape: &PhysicalShape) -> bool {
    is_local_route_shape(shape) || is_access_route_shape(shape)
}

fn is_generated_route_shape(shape: &PhysicalShape) -> bool {
    matches!(
        shape.purpose,
        PhysicalShapePurpose::Route
            | PhysicalShapePurpose::RouteFill
            | PhysicalShapePurpose::ViaLanding
            | PhysicalShapePurpose::Via
    ) || is_local_route_shape(shape)
        || is_access_route_shape(shape)
}

fn metal_rule(rules: &PhysicalRuleDeck, layer: u16) -> &crate::technology::LayerRule {
    rules
        .layer_overrides
        .get(&format!("metal{layer}"))
        .unwrap_or(&rules.metal)
}

fn minimum_metal_landing(rules: &PhysicalRuleDeck, layer: u16, adjacent_cut: u16) -> f64 {
    let metal = metal_rule(rules, layer);
    let cut = rules
        .via_overrides
        .get(&format!("via{adjacent_cut}{}", adjacent_cut + 1))
        .unwrap_or(&rules.via);
    (cut.size_um + 2.0 * cut.enclosure_um)
        .max(metal.min_width_um)
        .max(metal.min_area_um2.sqrt())
}

fn add_via_stack(
    shapes: &mut Vec<PhysicalShape>,
    point: (f64, f64),
    from_layer: u16,
    to_layer: u16,
    net: usize,
    rules: &PhysicalRuleDeck,
) {
    let low = from_layer.min(to_layer);
    let high = from_layer.max(to_layer);
    for layer in low..=high {
        let adjacent_cut = if layer == low {
            low
        } else {
            layer.saturating_sub(1)
        };
        let landing = minimum_metal_landing(rules, layer, adjacent_cut);
        shapes.push(PhysicalShape {
            layer: PhysicalLayer::Metal(layer),
            x: point.0,
            y: point.1,
            width: landing,
            height: landing,
            component_id: None,
            net: Some(net),
            purpose: PhysicalShapePurpose::ViaLanding,
        });
    }
    for lower in low..high {
        let cut = rules
            .via_overrides
            .get(&format!("via{lower}{}", lower + 1))
            .unwrap_or(&rules.via);
        shapes.push(PhysicalShape {
            layer: PhysicalLayer::Via(lower),
            x: point.0,
            y: point.1,
            width: cut.size_um,
            height: cut.size_um,
            component_id: None,
            net: Some(net),
            purpose: PhysicalShapePurpose::Via,
        });
    }
}

#[cfg(test)]
fn guard_via_enclosures_for_grid(shapes: &mut [PhysicalShape], rules: &PhysicalRuleDeck) {
    let grid = rules.manufacturing_grid_um;
    let vias = shapes
        .iter()
        .filter_map(|shape| match shape.layer {
            PhysicalLayer::Via(lower) => Some((lower, shape.x, shape.y, shape.net)),
            _ => None,
        })
        .collect::<Vec<_>>();
    for (lower, x, y, net) in vias {
        let cut = rules
            .via_overrides
            .get(&format!("via{lower}{}", lower + 1))
            .unwrap_or(&rules.via);
        // Experimental via-stack rectangles are edge-snapped independently.
        // Preserve one grid quantum on each side so a half-grid center shift
        // cannot consume the process enclosure.
        let guarded = cut.size_um + 2.0 * cut.enclosure_um + 2.0 * grid;
        for layer in [lower, lower + 1] {
            if let Some(landing) = shapes.iter_mut().find(|shape| {
                shape.layer == PhysicalLayer::Metal(layer)
                    && shape.purpose == PhysicalShapePurpose::ViaLanding
                    && shape.net == net
                    && (shape.x - x).abs() <= grid / 2.0
                    && (shape.y - y).abs() <= grid / 2.0
            }) {
                landing.width = landing.width.max(guarded);
                landing.height = landing.height.max(guarded);
            }
        }
    }
}

fn route_shape_conflicts(
    existing: &[PhysicalShape],
    candidate: &[PhysicalShape],
    net: usize,
    rules: &PhysicalRuleDeck,
) -> usize {
    let mut conflicting_nets = std::collections::HashSet::new();
    for shape in candidate {
        let left = shape.x - shape.width / 2.0;
        let top = shape.y - shape.height / 2.0;
        let right = shape.x + shape.width / 2.0;
        let bottom = shape.y + shape.height / 2.0;
        let clearance = match shape.layer {
            PhysicalLayer::Via(lower) => {
                rules
                    .via_overrides
                    .get(&format!("via{lower}{}", lower + 1))
                    .unwrap_or(&rules.via)
                    .min_spacing_um
            }
            PhysicalLayer::Metal(layer) => metal_rule(rules, layer).min_spacing_um,
            _ => ROUTING_CLEARANCE,
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

fn local_route_candidates(
    first: &RouteAnchor,
    second: &RouteAnchor,
    net: usize,
    rules: &PhysicalRuleDeck,
    max_metal_layers: u16,
) -> Vec<Vec<PhysicalShape>> {
    let landing = minimum_metal_landing(rules, 1, 1);
    let landing_shape = |point: (f64, f64)| PhysicalShape {
        layer: PhysicalLayer::Metal(1),
        x: point.0,
        y: point.1,
        width: landing,
        height: landing,
        component_id: None,
        net: Some(net),
        purpose: PhysicalShapePurpose::ViaLanding,
    };
    let mut candidates = Vec::new();
    for bend in [
        (second.point.0, first.point.1),
        (first.point.0, second.point.1),
    ] {
        let mut route = vec![landing_shape(first.point), landing_shape(second.point)];
        route.push(line_shape(
            first.point,
            bend,
            PhysicalLayer::Metal(1),
            Some(net),
            rules,
        ));
        route.push(line_shape(
            bend,
            second.point,
            PhysicalLayer::Metal(1),
            Some(net),
            rules,
        ));
        candidates.push(route);
    }
    if max_metal_layers >= 2 {
        let grid = rules.manufacturing_grid_um.max(1e-9);
        let spacing = minimum_metal_landing(rules, 2, 1)
            + metal_rule(rules, 2).min_spacing_um
            + ROUTING_SNAP_GUARD;
        let midpoint = snap_to_grid((first.point.1 + second.point.1) / 2.0, grid);
        let mut tracks = vec![first.point.1, second.point.1, midpoint];
        for step in 1..=4 {
            tracks.push(snap_to_grid(midpoint - step as f64 * spacing, grid));
            tracks.push(snap_to_grid(midpoint + step as f64 * spacing, grid));
        }
        tracks.sort_by(|left, right| {
            (left - midpoint)
                .abs()
                .total_cmp(&(right - midpoint).abs())
                .then_with(|| left.total_cmp(right))
        });
        tracks.dedup_by(|left, right| (*left - *right).abs() <= grid);
        for track_y in tracks {
            let first_via = (first.point.0, track_y);
            let second_via = (second.point.0, track_y);
            let mut route = vec![landing_shape(first.point), landing_shape(second.point)];
            route.push(line_shape(
                first.point,
                first_via,
                PhysicalLayer::Metal(1),
                Some(net),
                rules,
            ));
            route.push(line_shape(
                second.point,
                second_via,
                PhysicalLayer::Metal(1),
                Some(net),
                rules,
            ));
            add_via_stack(&mut route, first_via, 1, 2, net, rules);
            add_via_stack(&mut route, second_via, 1, 2, net, rules);
            route.push(line_shape(
                first_via,
                second_via,
                PhysicalLayer::Metal(2),
                Some(net),
                rules,
            ));
            candidates.push(route);
        }
    }
    let marker = local_route_marker(net);
    for candidate in &mut candidates {
        for shape in candidate {
            shape.component_id = Some(marker);
        }
    }
    candidates
}

// Retained only as a geometry regression fixture. Production routing must not
// collapse distinct logical terminal anchors merely because they share a net.
#[cfg(test)]
fn shared_signal_row_accesses(
    points: &[RouteAnchor],
    net: usize,
    max_metal_layers: u16,
    existing: &[PhysicalShape],
    rules: &PhysicalRuleDeck,
    standard_cell_by_device: &HashMap<Uuid, &str>,
) -> (Vec<RouteAnchor>, Vec<PhysicalShape>) {
    if max_metal_layers < 3 {
        return (points.to_vec(), Vec::new());
    }
    let grid = rules.manufacturing_grid_um;
    // Share access only inside one leaf standard-cell occurrence. A same-net
    // terminal in another leaf belongs to global routing; merging those
    // terminals here creates a chip-spanning pseudo-local bus and defeats
    // topology-aware placement.
    let mut row_members = BTreeMap::<(i64, &str), Vec<usize>>::new();
    for (index, anchor) in points.iter().enumerate() {
        if let (Some(component_id), PhysicalLayer::Metal(1)) = (anchor.component_id, anchor.layer) {
            let Some(group) = standard_cell_by_device.get(&component_id).copied() else {
                continue;
            };
            row_members
                .entry(((anchor.point.1 / grid).round() as i64, group))
                .or_default()
                .push(index);
        }
    }
    let mut consumed = HashSet::new();
    let mut shared_anchors = Vec::new();
    let mut shared_geometry = Vec::new();
    for indices in row_members.values_mut() {
        indices.sort_by(|left, right| {
            points[*left]
                .point
                .0
                .total_cmp(&points[*right].point.0)
                .then_with(|| left.cmp(right))
        });
        indices.dedup_by(|left, right| {
            (points[*left].point.0 - points[*right].point.0).abs() <= grid / 2.0
        });
        if indices.len() < 2 {
            continue;
        }
        let min_x = points[*indices.first().expect("shared access row is nonempty")]
            .point
            .0;
        let max_x = points[*indices.last().expect("shared access row is nonempty")]
            .point
            .0;
        if max_x - min_x <= grid {
            continue;
        }
        let y = points[indices[0]].point.1;
        let median_x = points[indices[indices.len() / 2]].point.0;
        let mut accepted = None;
        // M1/M2 remain local device and power resources. Prefer the lowest
        // legal signal layer so the shared access tree stays near its row.
        for layer in 3..=max_metal_layers {
            let mut candidate = vec![line_shape(
                (min_x, y),
                (max_x, y),
                PhysicalLayer::Metal(layer),
                Some(net),
                rules,
            )];
            for index in indices.iter().copied() {
                add_via_stack(&mut candidate, points[index].point, 1, layer, net, rules);
            }
            guard_via_enclosures_for_grid(&mut candidate, rules);
            for shape in &mut candidate {
                snap_shape_to_grid(shape, grid);
            }
            deduplicate_exact_vias(&mut candidate, grid);
            let mut local = PhysicalCanvas::new(rules);
            if route_shape_conflicts(existing, &candidate, net, rules) == 0
                && local
                    .commit_routing_geometry(&candidate, format!("shared-row-access-{net}-{layer}"))
                    .is_ok()
            {
                accepted = Some((layer, candidate));
                break;
            }
        }
        let Some((layer, candidate)) = accepted else {
            continue;
        };
        consumed.extend(indices.iter().copied());
        shared_geometry.extend(candidate);
        shared_anchors.push(RouteAnchor {
            point: (median_x, y),
            layer: PhysicalLayer::Metal(layer),
            access_bounds: points[indices[0]].access_bounds,
            component_id: None,
        });
    }
    let mut result = points
        .iter()
        .enumerate()
        .filter_map(|(index, anchor)| (!consumed.contains(&index)).then_some(*anchor))
        .collect::<Vec<_>>();
    result.extend(shared_anchors);
    result.sort_by(|left, right| {
        left.point
            .1
            .total_cmp(&right.point.1)
            .then_with(|| left.point.0.total_cmp(&right.point.0))
            .then_with(|| format!("{:?}", left.layer).cmp(&format!("{:?}", right.layer)))
    });
    (result, shared_geometry)
}

fn commit_preview_routing(
    shapes: Vec<PhysicalShape>,
    rules: &PhysicalRuleDeck,
) -> (Vec<PhysicalShape>, usize) {
    let mut canvas = PhysicalCanvas::new(rules);
    let mut conflicts = std::collections::HashSet::new();
    let mut accepted = vec![None; shapes.len()];
    let mut topology_poly_by_net = BTreeMap::<usize, Vec<usize>>::new();
    let mut topology_active_indices = Vec::new();
    for (shape_index, shape) in shapes.iter().enumerate() {
        if matches!(shape.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff)
            && shape.purpose == PhysicalShapePurpose::Active
            && shape.component_id.is_none()
        {
            topology_active_indices.push(shape_index);
            continue;
        }
        if shape.layer == PhysicalLayer::Poly
            && shape.purpose == PhysicalShapePurpose::GateAccess
            && shape.component_id.is_none()
        {
            if let Some(net) = shape.net {
                topology_poly_by_net
                    .entry(net)
                    .or_default()
                    .push(shape_index);
            }
            continue;
        }
        let obstruction = match shape.layer {
            PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => ObstructionType::Diffusion,
            PhysicalLayer::Poly => ObstructionType::Poly,
            PhysicalLayer::Contact => ObstructionType::Contact,
            PhysicalLayer::Pwell | PhysicalLayer::Nwell | PhysicalLayer::Substrate => {
                ObstructionType::Device
            }
            PhysicalLayer::Metal(_) | PhysicalLayer::Via(_) => continue,
        };
        if canvas
            .commit(shape, format!("preview-fixed-{shape_index}"), obstruction)
            .is_ok()
        {
            accepted[shape_index] = Some(shape.clone());
        }
    }
    let mut unseen = topology_active_indices
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut active_group = 0usize;
    while let Some(seed) = unseen.iter().copied().min() {
        unseen.remove(&seed);
        let mut pending = vec![seed];
        let mut indices = vec![seed];
        while let Some(current) = pending.pop() {
            let current_shape = &shapes[current];
            let neighbors = unseen
                .iter()
                .copied()
                .filter(|candidate_index| {
                    let other = &shapes[*candidate_index];
                    current_shape.layer == other.layer
                        && current_shape.x - current_shape.width / 2.0
                            <= other.x + other.width / 2.0 + 1e-9
                        && current_shape.x + current_shape.width / 2.0 + 1e-9
                            >= other.x - other.width / 2.0
                        && current_shape.y - current_shape.height / 2.0
                            <= other.y + other.height / 2.0 + 1e-9
                        && current_shape.y + current_shape.height / 2.0 + 1e-9
                            >= other.y - other.height / 2.0
                })
                .collect::<Vec<_>>();
            for neighbor in neighbors {
                unseen.remove(&neighbor);
                pending.push(neighbor);
                indices.push(neighbor);
            }
        }
        let candidate = indices
            .iter()
            .map(|index| shapes[*index].clone())
            .collect::<Vec<_>>();
        match canvas.commit_topology_active_geometry(
            &candidate,
            format!("preview-topology-active-island:{active_group}"),
        ) {
            Ok(_) => {
                for index in indices {
                    accepted[index] = Some(shapes[index].clone());
                }
            }
            Err(collisions) => {
                for collision in collisions {
                    conflicts.insert((
                        shapes[seed].layer,
                        usize::MAX,
                        collision.net.unwrap_or(usize::MAX),
                    ));
                }
            }
        }
        active_group += 1;
    }
    // Shared-poly topology is one conductor, not a sequence of independent
    // rectangles. Sequential fixed-shape admission rejected trunks/branches
    // against their own same-net gate accesses, then left row_topology claiming
    // straps that were absent from the emitted IR. Admit every net's complete
    // tree transactionally with the same collective rule used by placement.
    for (net, indices) in topology_poly_by_net {
        let candidate = indices
            .iter()
            .map(|index| shapes[*index].clone())
            .collect::<Vec<_>>();
        match canvas
            .commit_topology_poly_geometry(&candidate, format!("preview-topology-poly-net:{net}"))
        {
            Ok(_) => {
                for index in indices {
                    accepted[index] = Some(shapes[index].clone());
                }
            }
            Err(collisions) => {
                for collision in collisions {
                    let other_net = collision.net.unwrap_or(net);
                    conflicts.insert((PhysicalLayer::Poly, net.min(other_net), net.max(other_net)));
                }
            }
        }
    }
    // Power/device access is mandatory infrastructure, not an optional route
    // refinement. Admit each complete landing/via/drop bundle before ordinary
    // trunks can consume its access corridor. A bundle either lands in full or
    // leaves no residue for cleanup to misinterpret as connectivity.
    let mut access_groups = BTreeMap::<Uuid, Vec<usize>>::new();
    for (index, shape) in shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| is_access_route_shape(shape))
    {
        access_groups
            .entry(shape.component_id.expect("access route marker"))
            .or_default()
            .push(index);
    }
    for (marker, indices) in access_groups {
        let candidate = indices
            .iter()
            .map(|index| shapes[*index].clone())
            .collect::<Vec<_>>();
        if canvas
            .commit_routing_geometry(&candidate, format!("preview-access-route-{marker}"))
            .is_ok()
        {
            for index in indices {
                let mut shape = shapes[index].clone();
                shape.component_id = None;
                accepted[index] = Some(shape);
            }
        }
    }
    let mut metal_indices = shapes
        .iter()
        .enumerate()
        .filter_map(|(index, shape)| {
            (matches!(shape.layer, PhysicalLayer::Metal(_)) && !is_internal_route_shape(shape))
                .then_some(index)
        })
        .collect::<Vec<_>>();
    // Device diffusion contacts and gate-access points are unusable without
    // their local Metal 1 landing. Commit both before longer routes so a
    // rejected trunk cannot consume the access area and strand a transistor.
    metal_indices.sort_by_key(|index| {
        let metal = &shapes[*index];
        let lands_device_access = shapes.iter().any(|candidate| {
            matches!(
                candidate.layer,
                PhysicalLayer::Contact | PhysicalLayer::Poly
            ) && candidate.net == metal.net
                && match candidate.layer {
                    PhysicalLayer::Contact => {
                        contains_shape(metal, candidate, rules.contact.enclosure_um)
                    }
                    PhysicalLayer::Poly => shapes_touch(metal, candidate, 1e-9),
                    _ => false,
                }
        });
        !lands_device_access
    });
    for shape_index in metal_indices {
        let shape = &shapes[shape_index];
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
        if is_internal_route_shape(shape) {
            continue;
        }
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

fn admit_local_route_refinements(
    shapes: &mut Vec<PhysicalShape>,
    refinements: Vec<PhysicalShape>,
    rules: &PhysicalRuleDeck,
) {
    let mut canvas = PhysicalCanvas::new(rules);
    for (index, shape) in shapes.iter().enumerate() {
        let obstruction = match shape.layer {
            PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => ObstructionType::Diffusion,
            PhysicalLayer::Poly => ObstructionType::Poly,
            PhysicalLayer::Contact => ObstructionType::Contact,
            PhysicalLayer::Pwell | PhysicalLayer::Nwell | PhysicalLayer::Substrate => {
                ObstructionType::Device
            }
            PhysicalLayer::Metal(_) => ObstructionType::Metal,
            PhysicalLayer::Via(_) => ObstructionType::Via,
        };
        canvas.index_unchecked(shape, format!("finalized-preview-{index}"), obstruction);
    }
    let mut groups = BTreeMap::<Uuid, Vec<PhysicalShape>>::new();
    for shape in refinements {
        groups
            .entry(shape.component_id.expect("local refinement marker"))
            .or_default()
            .push(shape);
    }
    for (marker, candidate) in groups {
        if canvas
            .commit_routing_geometry(&candidate, format!("local-refinement-{marker}"))
            .is_ok()
        {
            shapes.extend(candidate.into_iter().map(|mut shape| {
                shape.component_id = None;
                shape
            }));
        }
    }
}

fn contains_shape(outer: &PhysicalShape, inner: &PhysicalShape, enclosure: f64) -> bool {
    outer.x - outer.width / 2.0 <= inner.x - inner.width / 2.0 - enclosure + 1e-9
        && outer.y - outer.height / 2.0 <= inner.y - inner.height / 2.0 - enclosure + 1e-9
        && outer.x + outer.width / 2.0 >= inner.x + inner.width / 2.0 + enclosure - 1e-9
        && outer.y + outer.height / 2.0 >= inner.y + inner.height / 2.0 + enclosure - 1e-9
}

fn add_shared_diffusion_regions(
    shapes: &mut Vec<PhysicalShape>,
    devices: &[PhysicalDevice],
    placed_by_id: &HashMap<Uuid, &PlacedDevice>,
    rules: &PhysicalRuleDeck,
) -> Vec<PhysicalRowTopology> {
    synthesize_row_topology(shapes, devices, placed_by_id, rules)
}

fn rectilinear_active_geometry(
    run: &[Uuid],
    active_by_id: &HashMap<Uuid, PhysicalShape>,
    grid: f64,
) -> Vec<PhysicalBounds> {
    let mut source = run
        .iter()
        .filter_map(|device_id| active_by_id.get(device_id).cloned())
        .collect::<Vec<_>>();
    for pair in run.windows(2) {
        let (Some(left), Some(right)) = (active_by_id.get(&pair[0]), active_by_id.get(&pair[1]))
        else {
            continue;
        };
        let left_edge = left.x + left.width / 2.0;
        let right_edge = right.x - right.width / 2.0;
        if right_edge > left_edge + grid / 2.0 {
            let top = (left.y - left.height / 2.0).max(right.y - right.height / 2.0);
            let bottom = (left.y + left.height / 2.0).min(right.y + right.height / 2.0);
            if bottom > top + grid / 2.0 {
                source.push(PhysicalShape {
                    layer: left.layer,
                    x: (left_edge + right_edge) / 2.0,
                    y: (top + bottom) / 2.0,
                    width: right_edge - left_edge,
                    height: bottom - top,
                    component_id: None,
                    net: None,
                    purpose: PhysicalShapePurpose::Active,
                });
            }
        }
    }
    let mut x_edges = source
        .iter()
        .flat_map(|shape| {
            [
                snap_to_grid(shape.x - shape.width / 2.0, grid),
                snap_to_grid(shape.x + shape.width / 2.0, grid),
            ]
        })
        .collect::<Vec<_>>();
    x_edges.sort_by(|left, right| left.total_cmp(right));
    x_edges.dedup_by(|left, right| (*left - *right).abs() <= grid / 2.0);

    let mut segments = Vec::<PhysicalBounds>::new();
    for edges in x_edges.windows(2) {
        let left = edges[0];
        let right = edges[1];
        if right <= left + grid / 2.0 {
            continue;
        }
        let midpoint = (left + right) / 2.0;
        let mut vertical = source
            .iter()
            .filter(|shape| {
                midpoint > shape.x - shape.width / 2.0 - grid / 2.0
                    && midpoint < shape.x + shape.width / 2.0 + grid / 2.0
            })
            .map(|shape| {
                (
                    snap_to_grid(shape.y - shape.height / 2.0, grid),
                    snap_to_grid(shape.y + shape.height / 2.0, grid),
                )
            })
            .collect::<Vec<_>>();
        if vertical.is_empty() {
            continue;
        }
        vertical.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        });
        let mut top = vertical[0].0;
        let mut bottom = vertical[0].1;
        for (next_top, next_bottom) in vertical.into_iter().skip(1) {
            if next_top <= bottom + grid / 2.0 {
                bottom = bottom.max(next_bottom);
            } else {
                segments.push(PhysicalBounds {
                    min_x: left,
                    min_y: top,
                    max_x: right,
                    max_y: bottom,
                });
                top = next_top;
                bottom = next_bottom;
            }
        }
        segments.push(PhysicalBounds {
            min_x: left,
            min_y: top,
            max_x: right,
            max_y: bottom,
        });
    }

    let mut merged = Vec::<PhysicalBounds>::new();
    for segment in segments {
        if let Some(previous) = merged.last_mut() {
            if (previous.max_x - segment.min_x).abs() <= grid / 2.0
                && (previous.min_y - segment.min_y).abs() <= grid / 2.0
                && (previous.max_y - segment.max_y).abs() <= grid / 2.0
            {
                previous.max_x = segment.max_x;
                continue;
            }
        }
        merged.push(segment);
    }
    merged
}

fn synthesize_row_topology(
    shapes: &mut Vec<PhysicalShape>,
    devices: &[PhysicalDevice],
    placed_by_id: &HashMap<Uuid, &PlacedDevice>,
    rules: &PhysicalRuleDeck,
) -> Vec<PhysicalRowTopology> {
    let grid = rules.manufacturing_grid_um;
    let by_id = devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let active_by_id = devices
        .iter()
        .filter_map(|device| {
            let placed = placed_by_id.get(&device.component_id)?;
            device_footprint_with_gate_access(device, placed.x, placed.y, rules, false)
                .into_iter()
                .map(|(shape, _)| shape)
                .find(|shape| shape.purpose == PhysicalShapePurpose::Active)
                .map(|shape| (device.component_id, shape))
        })
        .collect::<HashMap<_, _>>();
    let mut rows = BTreeMap::<(DeviceKind, i64), Vec<Uuid>>::new();
    for device in devices {
        let placed = placed_by_id[&device.component_id];
        rows.entry((device.kind, (placed.y / grid).round() as i64))
            .or_default()
            .push(device.component_id);
    }
    let snapshot = shapes.clone();
    let mut topology = Vec::new();
    let mut regions = Vec::new();
    for ((kind, row_index), row) in &mut rows {
        row.sort_by(|left, right| {
            placed_by_id[left]
                .x
                .total_cmp(&placed_by_id[right].x)
                .then_with(|| left.cmp(right))
        });
        let layer = if *kind == DeviceKind::Pmos {
            PhysicalLayer::Pdiff
        } else {
            PhysicalLayer::Ndiff
        };
        let mut runs = Vec::<Vec<Uuid>>::new();
        for device_id in row.iter().copied() {
            let Some(run) = runs.last_mut() else {
                runs.push(vec![device_id]);
                continue;
            };
            let left_id = *run.last().expect("nonempty active run");
            let left_device = by_id[&left_id];
            let right_device = by_id[&device_id];
            let Some(left) = active_by_id.get(&left_id) else {
                runs.push(vec![device_id]);
                continue;
            };
            let Some(right) = active_by_id.get(&device_id) else {
                runs.push(vec![device_id]);
                continue;
            };
            let left_edge = left.x + left.width / 2.0;
            let right_edge = right.x - right.width / 2.0;
            let gap = right_edge - left_edge;
            let bridge = PhysicalShape {
                layer,
                x: (left_edge + right_edge) / 2.0,
                y: (left.y + right.y) / 2.0,
                width: gap.max(grid),
                height: left.height.min(right.height),
                component_id: None,
                net: None,
                purpose: PhysicalShapePurpose::Active,
            };
            let blocked = gap > grid / 2.0
                && snapshot.iter().any(|shape| {
                    if !matches!(
                        shape.layer,
                        PhysicalLayer::Ndiff | PhysicalLayer::Pdiff | PhysicalLayer::Poly
                    ) || shape.component_id == Some(left_id)
                        || shape.component_id == Some(device_id)
                        || (shape.purpose == PhysicalShapePurpose::Active
                            && shape.component_id.is_none())
                    {
                        return false;
                    }
                    let spacing = if shape.layer == layer {
                        rules.diffusion.min_spacing_um
                    } else if shape.layer == PhysicalLayer::Poly {
                        // A foreign poly crossing would manufacture an
                        // unintended transistor inside the proposed island.
                        0.0
                    } else {
                        return false;
                    };
                    rectangles_within(&bridge, shape, spacing)
                });
            let coincident_shared_access = left_device.source_net == right_device.drain_net
                && snapshot.iter().any(|left_contact| {
                    left_contact.component_id == Some(left_id)
                        && left_contact.layer == PhysicalLayer::Contact
                        && left_contact.net == Some(left_device.source_net)
                        && snapshot.iter().any(|right_contact| {
                            right_contact.component_id == Some(device_id)
                                && right_contact.layer == PhysicalLayer::Contact
                                && right_contact.net == Some(right_device.drain_net)
                                && (left_contact.x - right_contact.x).abs() <= grid / 2.0
                                && (left_contact.y - right_contact.y).abs() <= grid / 2.0
                        })
                });
            let coincident_terminal_geometry = left_device.source_net == right_device.drain_net
                && ((placed_by_id[&left_id].x + device_terminal_offset(left_device, rules))
                    - (placed_by_id[&device_id].x - device_terminal_offset(right_device, rules)))
                .abs()
                    <= grid / 2.0;
            let same_cell = left_device.standard_cell_group.is_some()
                && left_device.standard_cell_group == right_device.standard_cell_group;
            if same_cell
                && left_device.source_net == right_device.drain_net
                && (gap >= -grid / 2.0 || coincident_shared_access || coincident_terminal_geometry)
                && !blocked
            {
                run.push(device_id);
            } else {
                runs.push(vec![device_id]);
            }
        }

        let mut islands = Vec::new();
        for run in runs {
            let first_id = run[0];
            let last_id = *run.last().expect("nonempty active run");
            let Some(first) = active_by_id.get(&first_id) else {
                continue;
            };
            let Some(last) = active_by_id.get(&last_id) else {
                continue;
            };
            let geometry = rectilinear_active_geometry(&run, &active_by_id, grid);
            let left = geometry
                .iter()
                .map(|bounds| bounds.min_x)
                .fold(first.x - first.width / 2.0, f64::min);
            let right = geometry
                .iter()
                .map(|bounds| bounds.max_x)
                .fold(last.x + last.width / 2.0, f64::max);
            let top = geometry
                .iter()
                .map(|bounds| bounds.min_y)
                .fold(f64::INFINITY, f64::min);
            let bottom = geometry
                .iter()
                .map(|bounds| bounds.max_y)
                .fold(f64::NEG_INFINITY, f64::max);
            let mut terminal_nets = vec![by_id[&first_id].drain_net];
            terminal_nets.extend(run.iter().map(|id| by_id[id].source_net));
            let mut accesses = vec![PhysicalTerminalAccess {
                net: by_id[&first_id].drain_net,
                device_ids: vec![first_id],
                x: placed_by_id[&first_id].x - device_terminal_offset(by_id[&first_id], rules),
                y: placed_by_id[&first_id].y,
                shared_contact: false,
            }];
            for (index, device_id) in run.iter().copied().enumerate() {
                let shared_with = run.get(index + 1).copied();
                accesses.push(PhysicalTerminalAccess {
                    net: by_id[&device_id].source_net,
                    device_ids: shared_with
                        .map_or_else(|| vec![device_id], |right_id| vec![device_id, right_id]),
                    x: shared_with.map_or_else(
                        || {
                            placed_by_id[&device_id].x
                                + device_terminal_offset(by_id[&device_id], rules)
                        },
                        |right_id| {
                            let left_x = placed_by_id[&device_id].x
                                + device_terminal_offset(by_id[&device_id], rules);
                            let right_x = placed_by_id[&right_id].x
                                - device_terminal_offset(by_id[&right_id], rules);
                            (left_x + right_x) / 2.0
                        },
                    ),
                    y: placed_by_id[&device_id].y,
                    shared_contact: false,
                });
            }
            let bounds = PhysicalBounds {
                min_x: left,
                min_y: top,
                max_x: right,
                max_y: bottom,
            };
            if run.len() > 1 {
                regions.extend(geometry.iter().map(|bounds| PhysicalShape {
                    layer,
                    x: (bounds.min_x + bounds.max_x) / 2.0,
                    y: (bounds.min_y + bounds.max_y) / 2.0,
                    width: bounds.max_x - bounds.min_x,
                    height: bounds.max_y - bounds.min_y,
                    component_id: None,
                    net: None,
                    purpose: PhysicalShapePurpose::Active,
                }));
            }
            islands.push(PhysicalActiveIsland {
                layer,
                device_ids: run,
                terminal_nets,
                accesses,
                bounds,
                geometry,
            });
        }
        let mut gate_runs = Vec::<Vec<Uuid>>::new();
        for device_id in row.iter().copied() {
            let Some(_) = gate_contact_point(&snapshot, by_id[&device_id]) else {
                gate_runs.push(vec![device_id]);
                continue;
            };
            let Some(run) = gate_runs.last_mut() else {
                gate_runs.push(vec![device_id]);
                continue;
            };
            let left_id = *run.last().expect("nonempty gate run");
            let left_device = by_id[&left_id];
            let Some(_) = gate_contact_point(&snapshot, left_device) else {
                gate_runs.push(vec![device_id]);
                continue;
            };
            let right_device = by_id[&device_id];
            let same_cell = left_device.standard_cell_group.is_some()
                && left_device.standard_cell_group == right_device.standard_cell_group;
            if same_cell && left_device.gate_net == right_device.gate_net {
                run.push(device_id);
            } else {
                gate_runs.push(vec![device_id]);
            }
        }
        let mut gate_straps = Vec::new();
        for run in gate_runs.into_iter().filter(|run| run.len() > 1) {
            let first_device = by_id[&run[0]];
            let contacts = run
                .iter()
                .filter_map(|device_id| {
                    gate_contact_point(&snapshot, by_id[device_id]).map(|point| (*device_id, point))
                })
                .collect::<Vec<_>>();
            if contacts.len() != run.len() {
                continue;
            }
            let row_y = *row_index as f64 * grid;
            let trunk_y = contacts
                .iter()
                .map(|(_, (_, y))| *y)
                .max_by(|left, right| {
                    (left - row_y)
                        .abs()
                        .total_cmp(&(right - row_y).abs())
                        .then_with(|| left.total_cmp(right))
                })
                .expect("nonempty gate-contact run");
            let min_x = contacts
                .iter()
                .map(|(_, (x, _))| *x)
                .fold(f64::INFINITY, f64::min);
            let max_x = contacts
                .iter()
                .map(|(_, (x, _))| *x)
                .fold(f64::NEG_INFINITY, f64::max);
            let mut candidates = vec![PhysicalShape {
                layer: PhysicalLayer::Poly,
                x: (min_x + max_x) / 2.0,
                y: trunk_y,
                width: max_x - min_x + rules.poly.min_width_um,
                height: rules.poly.min_width_um,
                component_id: None,
                net: Some(first_device.gate_net),
                purpose: PhysicalShapePurpose::GateAccess,
            }];
            for (_, (x, y)) in &contacts {
                if (y - trunk_y).abs() <= grid / 2.0 {
                    continue;
                }
                candidates.push(PhysicalShape {
                    layer: PhysicalLayer::Poly,
                    x: *x,
                    y: (y + trunk_y) / 2.0,
                    width: rules.poly.min_width_um,
                    height: (y - trunk_y).abs() + rules.poly.min_width_um,
                    component_id: None,
                    net: Some(first_device.gate_net),
                    purpose: PhysicalShapePurpose::GateAccess,
                });
            }
            for candidate in &mut candidates {
                snap_shape_to_grid(candidate, grid);
            }
            let run_ids = run.iter().copied().collect::<HashSet<_>>();
            let blocked =
                poly_candidates_blocked(&candidates, &snapshot, &run_ids, &active_by_id, rules);
            if blocked {
                continue;
            }
            let min_y = candidates
                .iter()
                .map(|shape| shape.y - shape.height / 2.0)
                .fold(f64::INFINITY, f64::min);
            let max_y = candidates
                .iter()
                .map(|shape| shape.y + shape.height / 2.0)
                .fold(f64::NEG_INFINITY, f64::max);
            let bounds = PhysicalBounds {
                min_x,
                min_y,
                max_x,
                max_y,
            };
            let geometry = candidates
                .iter()
                .map(|shape| PhysicalBounds {
                    min_x: shape.x - shape.width / 2.0,
                    min_y: shape.y - shape.height / 2.0,
                    max_x: shape.x + shape.width / 2.0,
                    max_y: shape.y + shape.height / 2.0,
                })
                .collect();
            regions.extend(candidates);
            gate_straps.push(PhysicalGateStrap {
                net: first_device.gate_net,
                device_ids: run,
                bounds,
                geometry,
            });
        }
        topology.push(PhysicalRowTopology {
            kind: *kind,
            y: *row_index as f64 * grid,
            ordered_devices: row.clone(),
            islands,
            gate_straps,
        });
    }
    let existing_gate_groups = topology
        .iter()
        .flat_map(|row| &row.gate_straps)
        .map(|strap| strap.device_ids.iter().copied().collect::<HashSet<_>>())
        .collect::<Vec<_>>();
    let mut cross_row_groups = BTreeMap::<(String, usize), Vec<Uuid>>::new();
    for device in devices {
        let Some(group) = device.standard_cell_group.as_ref() else {
            continue;
        };
        cross_row_groups
            .entry((group.clone(), device.gate_net))
            .or_default()
            .push(device.component_id);
    }
    for ((_, net), mut device_ids) in cross_row_groups {
        device_ids.sort();
        device_ids.dedup();
        if device_ids.len() < 2
            || existing_gate_groups.iter().any(|existing| {
                device_ids
                    .iter()
                    .all(|device_id| existing.contains(device_id))
            })
        {
            continue;
        }
        let kinds = device_ids
            .iter()
            .filter_map(|device_id| by_id.get(device_id).map(|device| device.kind))
            .collect::<HashSet<_>>();
        if kinds.len() < 2 {
            continue;
        }
        let contacts = device_ids
            .iter()
            .filter_map(|device_id| {
                gate_contact_point(&snapshot, by_id[device_id]).map(|point| (*device_id, point))
            })
            .collect::<Vec<_>>();
        if contacts.len() != device_ids.len() {
            continue;
        }
        let min_x = contacts
            .iter()
            .map(|(_, (x, _))| *x)
            .fold(f64::INFINITY, f64::min);
        let max_x = contacts
            .iter()
            .map(|(_, (x, _))| *x)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = contacts
            .iter()
            .map(|(_, (_, y))| *y)
            .fold(f64::INFINITY, f64::min);
        let max_y = contacts
            .iter()
            .map(|(_, (_, y))| *y)
            .fold(f64::NEG_INFINITY, f64::max);
        let mut trunk_ys = contacts.iter().map(|(_, (_, y))| *y).collect::<Vec<_>>();
        trunk_ys.sort_by(f64::total_cmp);
        trunk_ys.dedup_by(|left, right| (*left - *right).abs() <= grid / 2.0);
        let mut trunk_xs = contacts.iter().map(|(_, (x, _))| *x).collect::<Vec<_>>();
        trunk_xs.sort_by(f64::total_cmp);
        trunk_xs.dedup_by(|left, right| (*left - *right).abs() <= grid / 2.0);
        let mut alternatives = Vec::<Vec<PhysicalShape>>::new();
        for trunk_y in trunk_ys {
            let mut candidate = vec![PhysicalShape {
                layer: PhysicalLayer::Poly,
                x: (min_x + max_x) / 2.0,
                y: trunk_y,
                width: max_x - min_x + rules.poly.min_width_um,
                height: rules.poly.min_width_um,
                component_id: None,
                net: Some(net),
                purpose: PhysicalShapePurpose::GateAccess,
            }];
            candidate.extend(contacts.iter().filter_map(|(_, (x, y))| {
                ((y - trunk_y).abs() > grid / 2.0).then_some(PhysicalShape {
                    layer: PhysicalLayer::Poly,
                    x: *x,
                    y: (y + trunk_y) / 2.0,
                    width: rules.poly.min_width_um,
                    height: (y - trunk_y).abs() + rules.poly.min_width_um,
                    component_id: None,
                    net: Some(net),
                    purpose: PhysicalShapePurpose::GateAccess,
                })
            }));
            alternatives.push(candidate);
        }
        for trunk_x in trunk_xs {
            let mut candidate = vec![PhysicalShape {
                layer: PhysicalLayer::Poly,
                x: trunk_x,
                y: (min_y + max_y) / 2.0,
                width: rules.poly.min_width_um,
                height: max_y - min_y + rules.poly.min_width_um,
                component_id: None,
                net: Some(net),
                purpose: PhysicalShapePurpose::GateAccess,
            }];
            candidate.extend(contacts.iter().filter_map(|(_, (x, y))| {
                ((x - trunk_x).abs() > grid / 2.0).then_some(PhysicalShape {
                    layer: PhysicalLayer::Poly,
                    x: (x + trunk_x) / 2.0,
                    y: *y,
                    width: (x - trunk_x).abs() + rules.poly.min_width_um,
                    height: rules.poly.min_width_um,
                    component_id: None,
                    net: Some(net),
                    purpose: PhysicalShapePurpose::GateAccess,
                })
            }));
            alternatives.push(candidate);
        }
        alternatives.sort_by(|left, right| {
            let score = |shapes: &[PhysicalShape]| {
                shapes
                    .iter()
                    .map(|shape| shape.width * shape.height)
                    .sum::<f64>()
            };
            score(left)
                .total_cmp(&score(right))
                .then_with(|| left.len().cmp(&right.len()))
        });
        let mut selected = None;
        for candidates in &mut alternatives {
            for candidate in candidates.iter_mut() {
                snap_shape_to_grid(candidate, grid);
            }
            let covers_every_gate = contacts.iter().all(|(_, (x, y))| {
                candidates.iter().any(|shape| {
                    *x >= shape.x - shape.width / 2.0 - grid / 2.0
                        && *x <= shape.x + shape.width / 2.0 + grid / 2.0
                        && *y >= shape.y - shape.height / 2.0 - grid / 2.0
                        && *y <= shape.y + shape.height / 2.0 + grid / 2.0
                })
            });
            if !covers_every_gate {
                continue;
            }
            let run_ids = device_ids.iter().copied().collect::<HashSet<_>>();
            if !poly_candidates_blocked(candidates, &snapshot, &run_ids, &active_by_id, rules) {
                selected = Some(candidates.clone());
                break;
            }
        }
        let Some(candidates) = selected else {
            continue;
        };
        let bounds = candidates.iter().fold(
            PhysicalBounds {
                min_x: f64::INFINITY,
                min_y: f64::INFINITY,
                max_x: f64::NEG_INFINITY,
                max_y: f64::NEG_INFINITY,
            },
            |bounds, shape| PhysicalBounds {
                min_x: bounds.min_x.min(shape.x - shape.width / 2.0),
                min_y: bounds.min_y.min(shape.y - shape.height / 2.0),
                max_x: bounds.max_x.max(shape.x + shape.width / 2.0),
                max_y: bounds.max_y.max(shape.y + shape.height / 2.0),
            },
        );
        let geometry = candidates
            .iter()
            .map(|shape| PhysicalBounds {
                min_x: shape.x - shape.width / 2.0,
                min_y: shape.y - shape.height / 2.0,
                max_x: shape.x + shape.width / 2.0,
                max_y: shape.y + shape.height / 2.0,
            })
            .collect();
        regions.extend(candidates);
        if let Some(row) = topology
            .iter_mut()
            .find(|row| row.ordered_devices.contains(&device_ids[0]))
        {
            row.gate_straps.push(PhysicalGateStrap {
                net,
                device_ids,
                bounds,
                geometry,
            });
        }
    }
    topology.sort_by(|left, right| {
        left.y
            .total_cmp(&right.y)
            .then_with(|| left.kind.cmp(&right.kind))
    });
    let shared_devices = topology
        .iter()
        .flat_map(|row| &row.islands)
        .filter(|island| island.device_ids.len() > 1)
        .flat_map(|island| island.device_ids.iter().copied())
        .collect::<HashSet<_>>();
    shapes.retain(|shape| {
        shape.purpose != PhysicalShapePurpose::Active
            || shape
                .component_id
                .is_none_or(|component_id| !shared_devices.contains(&component_id))
    });
    shapes.extend(regions);
    topology
}

/// Replay final row-topology synthesis against a compact placement.
///
/// Placement uses this rather than maintaining separate active/poly
/// approximations, so candidate admission sees the same snapped rectilinear
/// diffusion islands and obstruction-checked gate straps that final Physical
/// IR emits.
pub(crate) fn topology_owned_device_regions(
    devices: &[PhysicalDevice],
    placements: &[PlacedDevice],
    rules: &PhysicalRuleDeck,
) -> (Vec<PhysicalShape>, Vec<PhysicalShape>) {
    let placed_by_id = placements
        .iter()
        .map(|placed| (placed.component_id, placed))
        .collect::<HashMap<_, _>>();
    let placed_devices = devices
        .iter()
        .filter(|device| placed_by_id.contains_key(&device.component_id))
        .cloned()
        .collect::<Vec<_>>();
    let geometry_devices = orient_devices_for_diffusion_sharing(
        &placed_devices,
        &placed_by_id,
        rules.manufacturing_grid_um,
    );
    let mut scratch = geometry_devices
        .iter()
        .filter_map(|device| {
            let placed = placed_by_id.get(&device.component_id)?;
            Some(
                device_footprint_with_gate_access(device, placed.x, placed.y, rules, false)
                    .into_iter()
                    .map(|(shape, _)| shape),
            )
        })
        .flatten()
        .collect::<Vec<_>>();
    synthesize_row_topology(&mut scratch, &geometry_devices, &placed_by_id, rules);
    let mut active = Vec::new();
    let mut poly = Vec::new();
    for shape in scratch {
        if matches!(shape.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff)
            && shape.purpose == PhysicalShapePurpose::Active
        {
            active.push(shape);
        } else if shape.layer == PhysicalLayer::Poly
            && shape.purpose == PhysicalShapePurpose::GateAccess
            && shape.component_id.is_none()
        {
            poly.push(shape);
        }
    }
    (active, poly)
}

fn gate_contact_point(shapes: &[PhysicalShape], device: &PhysicalDevice) -> Option<(f64, f64)> {
    let access = shapes.iter().find(|shape| {
        shape.component_id == Some(device.component_id)
            && shape.layer == PhysicalLayer::Poly
            && shape.net == Some(device.gate_net)
            && shape.purpose == PhysicalShapePurpose::GateAccess
    })?;
    shapes
        .iter()
        .find(|shape| {
            shape.component_id == Some(device.component_id)
                && shape.layer == PhysicalLayer::Contact
                && shape.net == Some(device.gate_net)
                && shape.purpose == PhysicalShapePurpose::Contact
                && contains_shape(access, shape, 0.0)
        })
        .map(|shape| (shape.x, shape.y))
}

fn poly_candidates_blocked(
    candidates: &[PhysicalShape],
    snapshot: &[PhysicalShape],
    run_ids: &HashSet<Uuid>,
    active_by_id: &HashMap<Uuid, PhysicalShape>,
    rules: &PhysicalRuleDeck,
) -> bool {
    candidates.iter().any(|candidate| {
        snapshot.iter().any(|shape| {
            if shape.layer == PhysicalLayer::Poly {
                return shape.net != candidate.net
                    && rectangles_within(candidate, shape, rules.poly.min_spacing_um);
            }
            if !matches!(shape.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff)
                || !rectangles_within(candidate, shape, 0.0)
            {
                return false;
            }
            // A route may follow a member device's existing gate axis through
            // its own active. Crossing any other active would manufacture an
            // unintended transistor and is never legal.
            let candidate_left = candidate.x - candidate.width / 2.0;
            let candidate_right = candidate.x + candidate.width / 2.0;
            let candidate_top = candidate.y - candidate.height / 2.0;
            let candidate_bottom = candidate.y + candidate.height / 2.0;
            let owners = shape.component_id.map_or_else(
                || {
                    run_ids
                        .iter()
                        .copied()
                        .filter(|device_id| {
                            active_by_id.get(device_id).is_some_and(|active| {
                                rectangles_within(candidate, active, 0.0)
                                    && rectangles_within(shape, active, 0.0)
                            })
                        })
                        .collect::<Vec<_>>()
                },
                |component_id| vec![component_id],
            );
            owners.is_empty()
                || owners.iter().any(|component_id| {
                    let Some(active) = active_by_id.get(component_id) else {
                        return true;
                    };
                    let active_left = active.x - active.width / 2.0;
                    let active_right = active.x + active.width / 2.0;
                    let active_top = active.y - active.height / 2.0;
                    let active_bottom = active.y + active.height / 2.0;
                    let intersection = PhysicalShape {
                        layer: PhysicalLayer::Poly,
                        x: (candidate_left.max(active_left) + candidate_right.min(active_right))
                            / 2.0,
                        y: (candidate_top.max(active_top) + candidate_bottom.min(active_bottom))
                            / 2.0,
                        width: candidate_right.min(active_right) - candidate_left.max(active_left),
                        height: candidate_bottom.min(active_bottom) - candidate_top.max(active_top),
                        component_id: Some(*component_id),
                        net: candidate.net,
                        purpose: PhysicalShapePurpose::GateAccess,
                    };
                    !run_ids.contains(component_id)
                        || !snapshot.iter().any(|gate| {
                            gate.component_id == Some(*component_id)
                                && gate.layer == PhysicalLayer::Poly
                                && gate.net == candidate.net
                                && gate.purpose == PhysicalShapePurpose::Gate
                                && contains_shape(gate, &intersection, 0.0)
                        })
                })
        })
    })
}

fn materialize_shared_terminal_accesses(
    shapes: &mut Vec<PhysicalShape>,
    topology: &mut [PhysicalRowTopology],
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    placed_by_id: &HashMap<Uuid, &PlacedDevice>,
    rules: &PhysicalRuleDeck,
) {
    let grid = rules.manufacturing_grid_um;
    let landing_size = device_diffusion_landing_size(rules);
    let metal1 = rules.layer_overrides.get("metal1").unwrap_or(&rules.metal);
    for access in topology
        .iter_mut()
        .flat_map(|row| &mut row.islands)
        .flat_map(|island| &mut island.accesses)
        .filter(|access| access.device_ids.len() > 1)
    {
        let left_id = access.device_ids[0];
        let right_id = access.device_ids[1];
        if !placed_by_id.contains_key(&left_id) || !placed_by_id.contains_key(&right_id) {
            continue;
        }
        let mut old_points = shapes
            .iter()
            .filter(|shape| {
                shape.layer == PhysicalLayer::Contact
                    && shape.purpose == PhysicalShapePurpose::Contact
                    && shape.net == Some(access.net)
                    && shape
                        .component_id
                        .is_some_and(|id| id == left_id || id == right_id)
            })
            .map(|shape| (shape.x, shape.y))
            .collect::<Vec<_>>();
        old_points.sort_by(|left, right| left.0.total_cmp(&right.0));
        old_points.dedup_by(|left, right| {
            (left.0 - right.0).abs() <= grid / 2.0 && (left.1 - right.1).abs() <= grid / 2.0
        });
        if old_points.is_empty() || old_points.len() > 2 {
            continue;
        }
        let matches_old_access = |shape: &PhysicalShape| {
            shape.net == Some(access.net)
                && shape
                    .component_id
                    .is_some_and(|id| id == left_id || id == right_id)
                && old_points.iter().any(|point| {
                    (shape.x - point.0).abs() <= grid / 2.0
                        && (shape.y - point.1).abs() <= grid / 2.0
                })
                && ((shape.layer == PhysicalLayer::Contact
                    && shape.purpose == PhysicalShapePurpose::Contact)
                    || (shape.layer == PhysicalLayer::Metal(1)
                        && shape.purpose == PhysicalShapePurpose::DeviceLanding))
        };
        let leaves_shared_boundary =
            nets.iter()
                .find(|net| net.id == access.net)
                .is_some_and(|net| {
                    net.terminals
                        .iter()
                        .any(|terminal| !access.device_ids.contains(&terminal.component_id))
                })
                || pins.iter().any(|pin| pin.net == access.net);
        if !leaves_shared_boundary {
            // The repeated source/drain net is completely implemented by the
            // shared active island. It is not a routable cell pin. Remove the
            // provisional contacts, M1 landings, and route tree produced for
            // the two pre-topology terminal anchors instead of leaving a
            // contact post with nowhere electrical to go.
            shapes.retain(|shape| {
                !matches_old_access(shape)
                    && !(shape.net == Some(access.net)
                        && matches!(shape.layer, PhysicalLayer::Metal(_) | PhysicalLayer::Via(_)))
            });
            access.shared_contact = false;
            continue;
        }
        let mut contact = PhysicalShape {
            layer: PhysicalLayer::Contact,
            x: access.x,
            y: access.y,
            width: rules.contact.size_um,
            height: rules.contact.size_um,
            component_id: None,
            net: Some(access.net),
            purpose: PhysicalShapePurpose::Contact,
        };
        let mut landing = PhysicalShape {
            layer: PhysicalLayer::Metal(1),
            x: access.x,
            y: access.y,
            width: old_points
                .last()
                .zip(old_points.first())
                .map_or(landing_size, |(right, left)| {
                    (right.0 - left.0).abs() + landing_size
                }),
            height: landing_size,
            component_id: None,
            net: Some(access.net),
            purpose: PhysicalShapePurpose::DeviceLanding,
        };
        snap_shape_to_grid(&mut contact, grid);
        snap_shape_to_grid(&mut landing, grid);
        let blocked = shapes.iter().any(|shape| {
            shape.layer == PhysicalLayer::Metal(1)
                && shape.net != landing.net
                && rectangles_within(&landing, shape, metal1.min_spacing_um)
        });
        if blocked {
            continue;
        }
        shapes.retain(|shape| !matches_old_access(shape));
        shapes.push(contact);
        shapes.push(landing);
        access.shared_contact = true;
    }
}

fn rectangles_within(left: &PhysicalShape, right: &PhysicalShape, spacing: f64) -> bool {
    left.x - left.width / 2.0 < right.x + right.width / 2.0 + spacing - 1e-9
        && left.x + left.width / 2.0 + spacing > right.x - right.width / 2.0 + 1e-9
        && left.y - left.height / 2.0 < right.y + right.height / 2.0 + spacing - 1e-9
        && left.y + left.height / 2.0 + spacing > right.y - right.height / 2.0 + 1e-9
}

fn orient_devices_for_diffusion_sharing(
    devices: &[PhysicalDevice],
    placed_by_id: &HashMap<Uuid, &PlacedDevice>,
    grid: f64,
) -> Vec<PhysicalDevice> {
    let by_id = devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let mut rows = BTreeMap::<(DeviceKind, i64), Vec<Uuid>>::new();
    for device in devices {
        let placed = placed_by_id[&device.component_id];
        rows.entry((device.kind, (placed.y / grid).round() as i64))
            .or_default()
            .push(device.component_id);
    }
    let mut mirrored = HashSet::new();
    for row in rows.values_mut() {
        row.sort_by(|left, right| {
            placed_by_id[left]
                .x
                .total_cmp(&placed_by_id[right].x)
                .then_with(|| left.cmp(right))
        });
        if row.len() < 2 {
            continue;
        }
        // Dynamic programming chooses source/drain orientation for the whole
        // row. MOS source and drain are electrically symmetric in this model;
        // the score maximizes equal facing nets, then minimizes flips.
        let mut scores = vec![[(i32::MIN, i32::MIN); 2]; row.len()];
        let mut parents = vec![[0usize; 2]; row.len()];
        scores[0][0] = (0, 0);
        scores[0][1] = (0, -1);
        for index in 1..row.len() {
            let left = by_id[&row[index - 1]];
            let right = by_id[&row[index]];
            for right_orientation in 0..2 {
                for left_orientation in 0..2 {
                    let prior = scores[index - 1][left_orientation];
                    if prior.0 == i32::MIN {
                        continue;
                    }
                    let left_facing = if left_orientation == 0 {
                        left.source_net
                    } else {
                        left.drain_net
                    };
                    let right_facing = if right_orientation == 0 {
                        right.drain_net
                    } else {
                        right.source_net
                    };
                    let same_cell = left.standard_cell_group.is_some()
                        && left.standard_cell_group == right.standard_cell_group;
                    let candidate = (
                        prior.0 + i32::from(same_cell && left_facing == right_facing),
                        prior.1 - i32::from(right_orientation == 1),
                    );
                    if candidate > scores[index][right_orientation] {
                        scores[index][right_orientation] = candidate;
                        parents[index][right_orientation] = left_orientation;
                    }
                }
            }
        }
        let mut orientation = usize::from(scores[row.len() - 1][1] > scores[row.len() - 1][0]);
        for index in (0..row.len()).rev() {
            if orientation == 1 {
                mirrored.insert(row[index]);
            }
            if index > 0 {
                orientation = parents[index][orientation];
            }
        }
    }
    devices
        .iter()
        .cloned()
        .map(|mut device| {
            if mirrored.contains(&device.component_id) {
                std::mem::swap(&mut device.drain_net, &mut device.source_net);
            }
            device
        })
        .collect()
}

fn tap_array_shapes(
    shapes: &[PhysicalShape],
    bounds: &PhysicalBounds,
    vdd_tracks: &[f64],
    gnd_tracks: &[f64],
    power_net: Option<usize>,
    ground_net: Option<usize>,
    max_metal_layers: u16,
    rules: &PhysicalRuleDeck,
) -> Vec<PhysicalShape> {
    let Some(max_distance) = rules.max_tap_distance_um else {
        return Vec::new();
    };
    let grid = rules.manufacturing_grid_um;
    let active_size = rules
        .diffusion
        .min_width_um
        .max(rules.diffusion.min_area_um2.sqrt())
        .max(rules.contact.size_um + 2.0 * rules.contact.enclosure_um);
    let edge_guard = active_size / 2.0 + rules.well_enclosure_um;
    let pitch = (max_distance * 0.8 / grid).floor().max(1.0) * grid;
    let start_x = bounds.min_x + edge_guard + pitch / 2.0;
    let end_x = bounds.max_x - edge_guard;
    let mut taps = Vec::new();
    for well in shapes
        .iter()
        .filter(|shape| matches!(shape.layer, PhysicalLayer::Nwell | PhysicalLayer::Pwell))
    {
        let (layer, tracks, net) = if well.layer == PhysicalLayer::Nwell {
            (PhysicalLayer::Ndiff, vdd_tracks, power_net)
        } else {
            (PhysicalLayer::Pdiff, gnd_tracks, ground_net)
        };
        let Some(net) = net else { continue };
        let well_min_y = well.y - well.height / 2.0 + edge_guard;
        let well_max_y = well.y + well.height / 2.0 - edge_guard;
        let tap_y = tracks
            .iter()
            .copied()
            .filter(|track| *track >= well_min_y && *track <= well_max_y)
            .min_by(|left, right| (left - well.y).abs().total_cmp(&(right - well.y).abs()))
            .unwrap_or_else(|| well.y.clamp(well_min_y, well_max_y));
        let mut cursor_x = start_x;
        while cursor_x <= end_x + grid / 2.0 {
            let x = snap_to_grid(cursor_x, grid);
            taps.push(PhysicalShape {
                layer,
                x,
                y: tap_y,
                width: active_size,
                height: active_size,
                component_id: None,
                net: Some(net),
                purpose: PhysicalShapePurpose::Tap,
            });
            taps.push(PhysicalShape {
                layer: PhysicalLayer::Contact,
                x,
                y: tap_y,
                width: rules.contact.size_um,
                height: rules.contact.size_um,
                component_id: None,
                net: Some(net),
                purpose: PhysicalShapePurpose::Tap,
            });
            taps.push(PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x,
                y: tap_y,
                width: device_diffusion_landing_size(rules),
                height: device_diffusion_landing_size(rules),
                component_id: None,
                net: Some(net),
                purpose: PhysicalShapePurpose::Tap,
            });
            if max_metal_layers >= 2 && tracks.iter().any(|track| (*track - tap_y).abs() <= grid) {
                add_via_stack(&mut taps, (x, tap_y), 1, 2, net, rules);
            }
            cursor_x += pitch;
        }
    }
    for shape in &mut taps {
        snap_shape_to_grid(shape, grid);
    }
    taps
}

fn compact_preview(
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    max_metal_layers: u16,
    rules: &PhysicalRuleDeck,
    planning: &PhysicalPlanningReport,
    placement: &PhysicalPlacementReport,
    flexible_gate_access: bool,
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
    // M1/M2 remain local access and power resources. Spread signal trunks over
    // the available upper horizontal layers.
    let mut signal_layers = planning
        .routing_layers
        .iter()
        .filter(|layer| {
            layer.layer >= 3
                && layer.layer <= max_metal_layers
                && !layer.reserved_for_power
                && matches!(
                    layer.preferred_direction,
                    RoutingDirection::Horizontal | RoutingDirection::Any
                )
        })
        .map(|layer| layer.layer)
        .collect::<Vec<_>>();
    if signal_layers.is_empty() {
        signal_layers.push(max_metal_layers.max(2));
    }
    // Tracks keep distinct Y coordinates even when their horizontal metals differ:
    // a via stack crosses every intermediate metal and would otherwise land on
    // another net sharing the same coordinate.
    let selected = &planning.candidates[planning.selected_candidate];
    let selected_placement = &placement.candidates[placement.selected_candidate];
    let routing_landing_size = (1..=max_metal_layers)
        .map(|layer| {
            minimum_metal_landing(
                rules,
                layer,
                layer.saturating_sub(1).max(1).min(max_metal_layers - 1),
            )
        })
        .fold(ROUTING_LANDING_SIZE, f64::max);
    let routing_clearance = (1..=max_metal_layers)
        .map(|layer| metal_rule(rules, layer).min_spacing_um)
        .fold(ROUTING_CLEARANCE, f64::max);
    let track_spacing = routing_landing_size + routing_clearance + ROUTING_SNAP_GUARD;
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
    let outward_gate_access = if flexible_gate_access {
        let mut groups = BTreeMap::<(String, DeviceKind), Vec<(f64, Uuid)>>::new();
        for device in devices {
            let Some(group) = device.standard_cell_group.as_ref() else {
                continue;
            };
            let Some(placed) = placed_by_id.get(&device.component_id) else {
                continue;
            };
            groups
                .entry((group.clone(), device.kind))
                .or_default()
                .push((placed.x, device.component_id));
        }
        let mut outward = HashSet::new();
        for devices in groups.values_mut() {
            devices.sort_by(|left, right| {
                left.0
                    .total_cmp(&right.0)
                    .then_with(|| left.1.cmp(&right.1))
            });
            for (_, component_id) in devices.iter().skip(1).step_by(2) {
                outward.insert(*component_id);
            }
        }
        outward
    } else {
        HashSet::new()
    };
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
    let center_channel = (signal_net_count.max(1) as f64 * track_spacing + 0.9).max(1.4);
    let mut shapes = vec![PhysicalShape {
        layer: PhysicalLayer::Substrate,
        x: 0.0,
        y: 0.0,
        width: cell_width,
        height: cell_height,
        component_id: None,
        net: None,
        purpose: PhysicalShapePurpose::Fabric,
    }];
    let (well_stripe_count, well_stripe_height) =
        well_stripe_geometry(cell_height, planning.row_height_um, rules);
    for stripe in 0..well_stripe_count {
        shapes.push(PhysicalShape {
            layer: if stripe % 2 == 0 {
                PhysicalLayer::Nwell
            } else {
                PhysicalLayer::Pwell
            },
            x: 0.0,
            y: bounds.min_y + (stripe as f64 + 0.5) * well_stripe_height,
            width: cell_width,
            height: well_stripe_height,
            component_id: None,
            net: None,
            purpose: PhysicalShapePurpose::Fabric,
        });
    }
    let power_net = nets
        .iter()
        .find(|net| net.role == NetRole::Power)
        .map(|net| net.id);
    let ground_net = nets
        .iter()
        .find(|net| net.role == NetRole::Ground)
        .map(|net| net.id);
    let distributed_tracks = |kind: DeviceKind, outward: f64, edge: f64| {
        let mut rows = selected_placement
            .devices
            .iter()
            .filter(|device| device.kind == kind)
            .map(|device| (device.row, device.y))
            .collect::<Vec<_>>();
        rows.sort_by_key(|(row, _)| *row);
        rows.dedup_by_key(|(row, _)| *row);
        let mut tracks = vec![edge];
        tracks.extend(rows.into_iter().map(|(_, y)| {
            (y + outward * planning.row_height_um * 0.45)
                .clamp(bounds.min_y + 0.25, bounds.max_y - 0.25)
        }));
        for track in &mut tracks {
            *track = snap_to_grid(*track, rules.manufacturing_grid_um);
        }
        tracks.sort_by(|left, right| left.total_cmp(right));
        tracks.dedup_by(|left, right| (*left - *right).abs() <= rules.manufacturing_grid_um);
        tracks
    };
    let vdd_tracks = distributed_tracks(DeviceKind::Pmos, -1.0, vdd_y);
    let gnd_tracks = distributed_tracks(DeviceKind::Nmos, 1.0, gnd_y);
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
                    rules,
                ));
                shapes.push(line_shape(
                    (left, gnd_y),
                    (right, gnd_y),
                    PhysicalLayer::Metal(1),
                    ground_net,
                    rules,
                ));
            }
        }
    } else {
        shapes.push(line_shape(
            (bounds.min_x + 0.25, vdd_y),
            (bounds.max_x - 0.25, vdd_y),
            PhysicalLayer::Metal(1),
            power_net,
            rules,
        ));
        shapes.push(line_shape(
            (bounds.min_x + 0.25, gnd_y),
            (bounds.max_x - 0.25, gnd_y),
            PhysicalLayer::Metal(1),
            ground_net,
            rules,
        ));
    }
    if max_metal_layers >= 2 {
        for (tracks, net) in [(&vdd_tracks, power_net), (&gnd_tracks, ground_net)] {
            let Some(net) = net else { continue };
            for y in tracks {
                shapes.push(power_rail_shape(
                    (bounds.min_x + 0.25, *y),
                    (bounds.max_x - 0.25, *y),
                    PhysicalLayer::Metal(2),
                    Some(net),
                    rules,
                ));
            }
        }
    }
    if max_metal_layers >= 3 {
        // Distributed M2 rails need an explicit upper-metal stitch. Without
        // it, every row is a legal but electrically isolated power island.
        // Keep VDD and GND on separated M3 spines and land every rail with a
        // complete M2-M3 connector stack before signal routing begins.
        for (tracks, net, x) in [
            (&vdd_tracks, power_net, bounds.min_x + cell_width * 0.25),
            (&gnd_tracks, ground_net, bounds.max_x - cell_width * 0.25),
        ] {
            let Some(net) = net else { continue };
            let Some((first, rest)) = tracks.split_first() else {
                continue;
            };
            let min_y = rest.iter().fold(*first, |current, y| current.min(*y));
            let max_y = rest.iter().fold(*first, |current, y| current.max(*y));
            shapes.push(power_rail_shape(
                (x, min_y),
                (x, max_y),
                PhysicalLayer::Metal(3),
                Some(net),
                rules,
            ));
            for y in tracks {
                add_via_stack(&mut shapes, (x, *y), 2, 3, net, rules);
            }
        }
    }

    let tap_shapes = tap_array_shapes(
        &shapes,
        &bounds,
        &vdd_tracks,
        &gnd_tracks,
        power_net,
        ground_net,
        max_metal_layers,
        rules,
    );
    shapes.extend(tap_shapes);
    let geometry_devices =
        orient_devices_for_diffusion_sharing(devices, &placed_by_id, rules.manufacturing_grid_um);
    let mut anchors: HashMap<usize, Vec<RouteAnchor>> = HashMap::new();
    let mut local_geometry = PhysicalCanvas::new(rules);
    for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
        let mut row = geometry_devices
            .iter()
            .filter(|device| device.kind == kind)
            .collect::<Vec<_>>();
        row.sort_by(|left, right| {
            let left_placed = placed_by_id[&left.component_id];
            let right_placed = placed_by_id[&right.component_id];
            left_placed
                .y
                .total_cmp(&right_placed.y)
                .then_with(|| left_placed.x.total_cmp(&right_placed.x))
                .then_with(|| left.name.cmp(&right.name))
        });
        for device in row {
            let placed = placed_by_id[&device.component_id];
            let access_bounds = device
                .physical_group
                .as_deref()
                .and_then(|group| region_by_group.get(group).copied());
            let x = placed.x;
            let y = placed.y;
            let preferred_outward = outward_gate_access.contains(&device.component_id);
            let mut selected_footprint = None;
            let mut uses_outward_gate = preferred_outward;
            for outward in [preferred_outward, !preferred_outward] {
                let footprint = device_footprint_with_gate_access(device, x, y, rules, outward);
                let entries = footprint
                    .iter()
                    .cloned()
                    .map(|(shape, obstruction)| {
                        (shape, format!("cell-geometry:{}", device.name), obstruction)
                    })
                    .collect::<Vec<_>>();
                if local_geometry.commit_batch(&entries).is_ok() {
                    selected_footprint = Some(footprint);
                    uses_outward_gate = outward;
                    break;
                }
            }
            let footprint = selected_footprint.unwrap_or_else(|| {
                uses_outward_gate = false;
                device_footprint_with_gate_access(device, x, y, rules, false)
            });
            shapes.extend(footprint.into_iter().map(|(shape, _)| shape));
            let terminal_offset = device_terminal_offset(device, rules);
            for (terminal_x, net) in [
                (x - terminal_offset, device.drain_net),
                (x + terminal_offset, device.source_net),
            ] {
                anchors.entry(net).or_default().push(RouteAnchor {
                    point: (terminal_x, y),
                    layer: PhysicalLayer::Metal(1),
                    access_bounds,
                    component_id: Some(device.component_id),
                });
            }
            anchors
                .entry(device.gate_net)
                .or_default()
                .push(RouteAnchor {
                    point: (
                        x,
                        device_gate_access_with_side(device, y, rules, uses_outward_gate),
                    ),
                    layer: PhysicalLayer::Metal(1),
                    access_bounds,
                    component_id: Some(device.component_id),
                });
        }
    }
    add_shared_diffusion_regions(&mut shapes, &geometry_devices, &placed_by_id, rules);

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
        .map(|(index, net)| (*net, signal_layers[index % signal_layers.len()]))
        .collect::<HashMap<_, _>>();

    let signal_pins = pins
        .iter()
        .filter(|pin| pin.role == NetRole::Input || pin.role == NetRole::Output)
        .collect::<Vec<_>>();
    let mut input_index = 0;
    let mut output_index = 0;
    let pin_x_by_net = signal_pins
        .iter()
        .map(|pin| {
            let x = if pin.role == NetRole::Input {
                let x = bounds.min_x + 0.35 + input_index as f64 * 0.7;
                input_index += 1;
                x
            } else {
                let x = bounds.max_x - 0.35 - output_index as f64 * 0.7;
                output_index += 1;
                x
            };
            (pin.net, x)
        })
        .collect::<HashMap<_, _>>();
    // Pack signal trunks by horizontal interval instead of assigning one
    // chip-wide Y track to every net. Nets whose terminal/pin spans do not
    // overlap may legally share a track coordinate; the shared canvas still
    // rejects any vertical-access or via conflict during admission.
    let mut signal_intervals = signal_nets
        .iter()
        .map(|net| {
            let mut xs = anchors
                .get(net)
                .into_iter()
                .flatten()
                .map(|anchor| anchor.point.0)
                .collect::<Vec<_>>();
            if let Some(pin_x) = pin_x_by_net.get(net) {
                xs.push(*pin_x);
            }
            let (min_x, max_x) = if xs.is_empty() {
                (0.0, 0.0)
            } else {
                (
                    xs.iter().copied().fold(f64::INFINITY, f64::min),
                    xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                )
            };
            (*net, min_x, max_x)
        })
        .collect::<Vec<_>>();
    signal_intervals.sort_by(|left, right| {
        left.1
            .total_cmp(&right.1)
            .then_with(|| left.2.total_cmp(&right.2))
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut track_ends = Vec::<f64>::new();
    let mut track_index = HashMap::<usize, usize>::new();
    for (net, min_x, max_x) in signal_intervals {
        let reusable = track_ends
            .iter()
            .position(|end| *end + track_spacing <= min_x + 1e-9);
        let index = reusable.unwrap_or_else(|| {
            track_ends.push(f64::NEG_INFINITY);
            track_ends.len() - 1
        });
        track_ends[index] = max_x;
        track_index.insert(net, index);
    }
    let track_center = track_ends.len().saturating_sub(1) as f64 / 2.0;
    let signal_track = track_index
        .into_iter()
        .map(|(net, index)| (net, (index as f64 - track_center) * track_spacing))
        .collect::<HashMap<_, _>>();

    for pin in signal_pins {
        let x = pin_x_by_net[&pin.net];
        let y = signal_track[&pin.net];
        let layer = PhysicalLayer::Metal(signal_layer[&pin.net]);
        anchors.entry(pin.net).or_default().push(RouteAnchor {
            point: (x, y),
            layer,
            access_bounds: None,
            component_id: None,
        });
        shapes.push(PhysicalShape {
            layer,
            x,
            y,
            width: minimum_metal_landing(
                rules,
                signal_layer[&pin.net],
                signal_layer[&pin.net].saturating_sub(1).max(1),
            ),
            height: minimum_metal_landing(
                rules,
                signal_layer[&pin.net],
                signal_layer[&pin.net].saturating_sub(1).max(1),
            ),
            component_id: Some(pin.component_id),
            net: Some(pin.net),
            purpose: PhysicalShapePurpose::Pin,
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
            NetRole::Power | NetRole::Ground => 1,
            NetRole::Internal if net_group_count[&net.id] > 1 => 2,
            NetRole::Input | NetRole::Output | NetRole::Internal => 3,
        });
    }
    let standard_cell_by_device = devices
        .iter()
        .filter_map(|device| {
            device
                .standard_cell_group
                .as_deref()
                .map(|group| (device.component_id, group))
        })
        .collect::<HashMap<_, _>>();
    let pin_nets = pins.iter().map(|pin| pin.net).collect::<HashSet<_>>();
    let mut deferred_local_routes = Vec::new();
    for net in routed_nets
        .iter()
        .copied()
        .filter(|net| net.role == NetRole::Internal)
    {
        let Some(points) = anchors.get(&net.id) else {
            continue;
        };
        if points.len() != 2 || pin_nets.contains(&net.id) {
            continue;
        }
        // Flexible lower metal closes short same-cell source/drain pairs
        // before global routing occupies the channel. The candidate generator
        // handles both same-row and cross-row pairs transactionally.
        let groups = points
            .iter()
            .filter_map(|anchor| anchor.component_id)
            .filter_map(|component_id| standard_cell_by_device.get(&component_id).copied())
            .collect::<HashSet<_>>();
        if groups.len() != 1 {
            continue;
        }
        let mut candidates =
            local_route_candidates(&points[0], &points[1], net.id, rules, max_metal_layers);
        candidates.sort_by(|left, right| {
            let score = |candidate: &[PhysicalShape]| {
                let conflicts = route_shape_conflicts(&shapes, candidate, net.id, rules);
                let vias = candidate
                    .iter()
                    .filter(|shape| matches!(shape.layer, PhysicalLayer::Via(_)))
                    .count();
                let area = candidate
                    .iter()
                    .map(|shape| shape.width * shape.height)
                    .sum::<f64>();
                (conflicts, vias, area)
            };
            let left_score = score(left);
            let right_score = score(right);
            left_score
                .0
                .cmp(&right_score.0)
                .then_with(|| left_score.1.cmp(&right_score.1))
                .then_with(|| left_score.2.total_cmp(&right_score.2))
        });
        if let Some(candidate) = candidates
            .into_iter()
            .find(|candidate| route_shape_conflicts(&shapes, candidate, net.id, rules) == 0)
        {
            deferred_local_routes.extend(candidate);
        }
    }
    for net in routed_nets {
        let Some(points) = anchors.get(&net.id) else {
            continue;
        };
        let signal = net.role != NetRole::Power && net.role != NetRole::Ground;
        // Same-net membership does not make two physical terminals one
        // terminal. Keep every gate/source/drain and boundary-pin anchor in
        // the route problem; they may meet only through emitted routing (or
        // through topology-proven shared diffusion/poly synthesized below).
        // Collapsing several anchors into one upper-metal pseudo-terminal can
        // hide an omitted branch while net-labelled geometry still appears
        // connected to the native audit.
        let points = points;
        let nearest_track = |tracks: &[f64], y: f64| {
            tracks
                .iter()
                .copied()
                .min_by(|left, right| {
                    (left - y)
                        .abs()
                        .total_cmp(&(right - y).abs())
                        .then_with(|| left.total_cmp(right))
                })
                .unwrap_or(y)
        };
        let track_y = match net.role {
            NetRole::Power => nearest_track(
                &vdd_tracks,
                points.iter().map(|anchor| anchor.point.1).sum::<f64>() / points.len() as f64,
            ),
            NetRole::Ground => nearest_track(
                &gnd_tracks,
                points.iter().map(|anchor| anchor.point.1).sum::<f64>() / points.len() as f64,
            ),
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
        let route_layer = if signal {
            PhysicalLayer::Metal(signal_layer[&net.id])
        } else if max_metal_layers >= 2 {
            PhysicalLayer::Metal(2)
        } else {
            PhysicalLayer::Metal(1)
        };
        if !hierarchical || signal {
            shapes.push(line_shape(
                (min_x, track_y),
                (max_x, track_y),
                route_layer,
                Some(net.id),
                rules,
            ));
        }
        for (anchor_ordinal, anchor) in points.iter().enumerate() {
            let anchor_track_y = match net.role {
                NetRole::Power => nearest_track(&vdd_tracks, anchor.point.1),
                NetRole::Ground => nearest_track(&gnd_tracks, anchor.point.1),
                _ => track_y,
            };
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
                match anchor.layer {
                    // Preserve the established M1 access search. The top
                    // routing layer remains available as a trunk rather than
                    // a drop column, which avoids unnecessary boundary
                    // excursions on ordinary device terminals.
                    PhysicalLayer::Metal(1) => (2..max_metal_layers).collect::<Vec<_>>(),
                    PhysicalLayer::Metal(layer) => (layer..=max_metal_layers).collect::<Vec<_>>(),
                    _ => vec![1],
                }
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
                (((access_max_x - access_min_x) / track_spacing).ceil() as usize).max(1);
            let mut drop_columns = vec![(0usize, anchor.point.0)];
            for step in 1..=max_jog_steps {
                for direction in [-1.0, 1.0] {
                    let x = anchor.point.0 + direction * step as f64 * track_spacing;
                    if x >= access_min_x + routing_landing_size
                        && x <= access_max_x - routing_landing_size
                    {
                        drop_columns.push((step, x));
                    }
                }
            }
            let mut best_candidate = None;
            'access_search: for drop_layer_index in candidate_layers {
                for (jog_steps, drop_x) in &drop_columns {
                    let mut candidate = Vec::new();
                    let anchor_layer = match anchor.layer {
                        PhysicalLayer::Metal(layer) => layer,
                        _ => 1,
                    };
                    let drop_layer = if anchor.layer == PhysicalLayer::Metal(1) {
                        PhysicalLayer::Metal(drop_layer_index)
                    } else {
                        anchor.layer
                    };
                    if anchor.layer == PhysicalLayer::Metal(1)
                        && (anchor.point.0 - *drop_x).abs() > f64::EPSILON
                    {
                        candidate.push(line_shape(
                            anchor.point,
                            (*drop_x, anchor.point.1),
                            drop_layer,
                            Some(net.id),
                            rules,
                        ));
                    }
                    if (anchor.point.1 - anchor_track_y).abs() > f64::EPSILON {
                        candidate.push(line_shape(
                            (*drop_x, anchor.point.1),
                            (*drop_x, anchor_track_y),
                            drop_layer,
                            Some(net.id),
                            rules,
                        ));
                    }
                    if signal && anchor.layer == PhysicalLayer::Metal(1) {
                        let target_layer = signal_layer[&net.id];
                        add_via_stack(
                            &mut candidate,
                            anchor.point,
                            1,
                            drop_layer_index,
                            net.id,
                            rules,
                        );
                        add_via_stack(
                            &mut candidate,
                            (*drop_x, anchor_track_y),
                            drop_layer_index,
                            target_layer,
                            net.id,
                            rules,
                        );
                    } else if signal {
                        let target_layer = signal_layer[&net.id];
                        add_via_stack(
                            &mut candidate,
                            anchor.point,
                            anchor_layer,
                            drop_layer_index,
                            net.id,
                            rules,
                        );
                        add_via_stack(
                            &mut candidate,
                            (*drop_x, anchor_track_y),
                            drop_layer_index,
                            target_layer,
                            net.id,
                            rules,
                        );
                    } else if !signal && drop_layer_index > 1 {
                        add_via_stack(
                            &mut candidate,
                            anchor.point,
                            1,
                            drop_layer_index,
                            net.id,
                            rules,
                        );
                        add_via_stack(
                            &mut candidate,
                            (*drop_x, anchor_track_y),
                            2,
                            drop_layer_index,
                            net.id,
                            rules,
                        );
                    }
                    let conflicts = route_shape_conflicts(&shapes, &candidate, net.id, rules);
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
            if let Some((_, _, _, mut candidate)) = best_candidate {
                if !signal {
                    let marker = access_route_marker(net.id, anchor.component_id, anchor_ordinal);
                    for shape in &mut candidate {
                        shape.component_id = Some(marker);
                    }
                }
                shapes.extend(candidate);
            }
        }
    }
    // Local M1/M2 refinement is deliberately admitted after global access.
    // This lets compact cell wiring use otherwise free lower-metal capacity
    // without displacing a route that is already needed to close the net.
    shapes.extend(deferred_local_routes);
    if hierarchical && max_metal_layers >= 2 {
        for (y, net) in [(vdd_y, power_net), (gnd_y, ground_net)] {
            let Some(net) = net else { continue };
            shapes.push(line_shape(
                (bounds.min_x + 0.25, y),
                (bounds.max_x - 0.25, y),
                PhysicalLayer::Metal(2),
                Some(net),
                rules,
            ));
            for region in &selected_placement.block_regions {
                let x = (region.min_x + region.max_x) / 2.0;
                add_via_stack(&mut shapes, (x, y), 1, 2, net, rules);
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

fn shapes_touch(left: &PhysicalShape, right: &PhysicalShape, epsilon: f64) -> bool {
    left.x - left.width / 2.0 <= right.x + right.width / 2.0 + epsilon
        && left.x + left.width / 2.0 + epsilon >= right.x - right.width / 2.0
        && left.y - left.height / 2.0 <= right.y + right.height / 2.0 + epsilon
        && left.y + left.height / 2.0 + epsilon >= right.y - right.height / 2.0
}

fn routing_shapes_connect(left: &PhysicalShape, right: &PhysicalShape, epsilon: f64) -> bool {
    if left.net.is_none() || left.net != right.net || !shapes_touch(left, right, epsilon) {
        return false;
    }
    match (left.layer, right.layer) {
        (PhysicalLayer::Metal(left_layer), PhysicalLayer::Metal(right_layer)) => {
            left_layer == right_layer
        }
        (PhysicalLayer::Via(lower), PhysicalLayer::Metal(layer))
        | (PhysicalLayer::Metal(layer), PhysicalLayer::Via(lower)) => {
            layer == lower || layer == lower + 1
        }
        _ => false,
    }
}

fn terminal_routing_islands(shapes: &[PhysicalShape], net: usize) -> Vec<Vec<usize>> {
    let routing = shapes
        .iter()
        .enumerate()
        .filter_map(|(index, shape)| {
            (shape.net == Some(net)
                && matches!(shape.layer, PhysicalLayer::Metal(_) | PhysicalLayer::Via(_)))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let access = shapes
        .iter()
        .filter(|shape| {
            shape.net == Some(net)
                && shape.layer == PhysicalLayer::Contact
                && (shape.component_id.is_some() || shape.purpose == PhysicalShapePurpose::Contact)
        })
        .collect::<Vec<_>>();
    let mut islands = Vec::new();
    let mut visited = HashSet::new();
    for start in &routing {
        if visited.contains(start) {
            continue;
        }
        let mut island = Vec::new();
        let mut pending = vec![*start];
        while let Some(index) = pending.pop() {
            if !visited.insert(index) {
                continue;
            }
            island.push(index);
            pending.extend(routing.iter().copied().filter(|candidate| {
                !visited.contains(candidate)
                    && routing_shapes_connect(&shapes[index], &shapes[*candidate], 1e-9)
            }));
        }
        let reaches_terminal = island.iter().any(|index| {
            matches!(
                shapes[*index].purpose,
                PhysicalShapePurpose::Pin | PhysicalShapePurpose::PowerRail
            ) || (shapes[*index].layer == PhysicalLayer::Metal(1)
                && access
                    .iter()
                    .any(|terminal| shapes_touch(&shapes[*index], terminal, 1e-9)))
        });
        if reaches_terminal {
            islands.push(island);
        }
    }
    islands.sort_by_key(|island| std::cmp::Reverse(island.len()));
    islands
}

fn exact_shape_exists(shapes: &[PhysicalShape], candidate: &PhysicalShape) -> bool {
    shapes.iter().any(|existing| {
        existing.layer == candidate.layer
            && existing.net == candidate.net
            && (existing.x - candidate.x).abs() <= 1e-9
            && (existing.y - candidate.y).abs() <= 1e-9
            && (existing.width - candidate.width).abs() <= 1e-9
            && (existing.height - candidate.height).abs() <= 1e-9
    })
}

fn routing_candidate_is_legal(
    canvas: &PhysicalCanvas,
    candidate: &[PhysicalShape],
    rules: &PhysicalRuleDeck,
    owner: &str,
) -> bool {
    if candidate
        .iter()
        .any(|shape| canvas.can_route(shape).is_err())
    {
        return false;
    }
    // Baseline collisions were checked through the real canvas' spatial
    // index. A small local canvas checks interactions within the speculative
    // bundle without cloning the complete routed design.
    let mut local = PhysicalCanvas::new(rules);
    local
        .commit_routing_geometry(candidate, owner.to_string())
        .is_ok()
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TrackSearchNode {
    x: usize,
    y: usize,
    layer: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrackSearchQueueEntry {
    estimated_total: i64,
    cost: i64,
    node: TrackSearchNode,
}

impl Ord for TrackSearchQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max heap. Reverse the score ordering and retain a
        // stable node tie-break so identical inputs produce identical routes.
        other
            .estimated_total
            .cmp(&self.estimated_total)
            .then_with(|| other.cost.cmp(&self.cost))
            .then_with(|| other.node.layer.cmp(&self.node.layer))
            .then_with(|| other.node.y.cmp(&self.node.y))
            .then_with(|| other.node.x.cmp(&self.node.x))
    }
}

impl PartialOrd for TrackSearchQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn sorted_track_coordinates(
    minimum: f64,
    maximum: f64,
    pitch: f64,
    grid: f64,
    anchors: impl Iterator<Item = f64>,
) -> Vec<f64> {
    let first = (minimum / pitch).floor() as i64;
    let last = (maximum / pitch).ceil() as i64;
    let mut coordinates = (first..=last)
        .map(|index| snap_to_grid(index as f64 * pitch, grid))
        .chain(anchors.map(|value| snap_to_grid(value, grid)))
        .collect::<Vec<_>>();
    coordinates.sort_by(|left, right| left.total_cmp(right));
    coordinates.dedup_by(|left, right| (*left - *right).abs() <= grid / 2.0);
    coordinates
}

fn multilayer_track_search(
    canvas: &PhysicalCanvas,
    shapes: &[PhysicalShape],
    left: &[usize],
    right: &[usize],
    net: usize,
    max_layer: u16,
    rules: &PhysicalRuleDeck,
    track_divisor: u8,
) -> Option<Vec<PhysicalShape>> {
    let trace = std::env::var_os("OPENCHIPPY_ROUTER_TRACE").is_some();
    let grid = rules.manufacturing_grid_um;
    let pitch = (1..=max_layer)
        .map(|layer| {
            let rule = metal_rule(rules, layer);
            rule.min_width_um + rule.min_spacing_um + grid * 2.0
        })
        .fold(f64::INFINITY, f64::min)
        / f64::from(track_divisor.max(1));
    if !pitch.is_finite() || pitch <= 0.0 {
        return None;
    }
    // Search the rectilinear corridor between the two electrical islands,
    // rather than constructing the Cartesian product of every track across
    // the complete chip. The canvas still checks this local graph against all
    // global obstructions.
    let anchor_indices = left.iter().chain(right.iter()).copied().collect::<Vec<_>>();
    let anchor_shapes = anchor_indices
        .iter()
        .map(|index| &shapes[*index])
        .filter(|shape| matches!(shape.layer, PhysicalLayer::Metal(_)))
        .collect::<Vec<_>>();
    let min_x = anchor_shapes
        .iter()
        .map(|shape| shape.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = anchor_shapes
        .iter()
        .map(|shape| shape.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = anchor_shapes
        .iter()
        .map(|shape| shape.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = anchor_shapes
        .iter()
        .map(|shape| shape.y)
        .fold(f64::NEG_INFINITY, f64::max);
    if ![min_x, max_x, min_y, max_y]
        .iter()
        .all(|value| value.is_finite())
    {
        return None;
    }
    let margin = pitch * 8.0;
    let xs = sorted_track_coordinates(
        min_x - margin,
        max_x + margin,
        pitch,
        grid,
        anchor_indices.iter().map(|index| shapes[*index].x),
    );
    let ys = sorted_track_coordinates(
        min_y - margin,
        max_y + margin,
        pitch,
        grid,
        anchor_indices.iter().map(|index| shapes[*index].y),
    );
    let coordinate_index = |coordinates: &[f64], value: f64| {
        coordinates
            .binary_search_by(|candidate| candidate.total_cmp(&snap_to_grid(value, grid)))
            .ok()
    };
    let nodes_for = |indices: &[usize]| {
        let mut nodes = HashSet::new();
        for index in indices {
            let shape = &shapes[*index];
            let PhysicalLayer::Metal(layer) = shape.layer else {
                continue;
            };
            let Some(center_x) = coordinate_index(&xs, shape.x) else {
                continue;
            };
            let Some(center_y) = coordinate_index(&ys, shape.y) else {
                continue;
            };
            nodes.insert(TrackSearchNode {
                x: center_x,
                y: center_y,
                layer,
            });
            if shape.width >= shape.height {
                let left =
                    shape.x - shape.width / 2.0 + metal_rule(rules, layer).min_width_um / 2.0;
                let right =
                    shape.x + shape.width / 2.0 - metal_rule(rules, layer).min_width_um / 2.0;
                for (x, coordinate) in xs.iter().enumerate() {
                    if *coordinate >= left - grid / 2.0 && *coordinate <= right + grid / 2.0 {
                        nodes.insert(TrackSearchNode {
                            x,
                            y: center_y,
                            layer,
                        });
                    }
                }
            } else {
                let top =
                    shape.y - shape.height / 2.0 + metal_rule(rules, layer).min_width_um / 2.0;
                let bottom =
                    shape.y + shape.height / 2.0 - metal_rule(rules, layer).min_width_um / 2.0;
                for (y, coordinate) in ys.iter().enumerate() {
                    if *coordinate >= top - grid / 2.0 && *coordinate <= bottom + grid / 2.0 {
                        nodes.insert(TrackSearchNode {
                            x: center_x,
                            y,
                            layer,
                        });
                    }
                }
            }
        }
        nodes
    };
    let mut starts = nodes_for(left);
    let mut targets = nodes_for(right);
    let nearest_nodes = |nodes: &mut HashSet<TrackSearchNode>,
                         opposite: &HashSet<TrackSearchNode>| {
        const ACCESS_SEED_LIMIT: usize = 64;
        if nodes.len() <= ACCESS_SEED_LIMIT || opposite.is_empty() {
            return;
        }
        let opposite_min_x = opposite
            .iter()
            .map(|node| xs[node.x])
            .fold(f64::INFINITY, f64::min);
        let opposite_max_x = opposite
            .iter()
            .map(|node| xs[node.x])
            .fold(f64::NEG_INFINITY, f64::max);
        let opposite_min_y = opposite
            .iter()
            .map(|node| ys[node.y])
            .fold(f64::INFINITY, f64::min);
        let opposite_max_y = opposite
            .iter()
            .map(|node| ys[node.y])
            .fold(f64::NEG_INFINITY, f64::max);
        let mut ranked = nodes.iter().copied().collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            let distance = |node: TrackSearchNode| {
                let x = xs[node.x];
                let y = ys[node.y];
                let dx = if x < opposite_min_x {
                    opposite_min_x - x
                } else if x > opposite_max_x {
                    x - opposite_max_x
                } else {
                    0.0
                };
                let dy = if y < opposite_min_y {
                    opposite_min_y - y
                } else if y > opposite_max_y {
                    y - opposite_max_y
                } else {
                    0.0
                };
                dx + dy
            };
            distance(*left)
                .total_cmp(&distance(*right))
                .then_with(|| left.layer.cmp(&right.layer))
                .then_with(|| left.y.cmp(&right.y))
                .then_with(|| left.x.cmp(&right.x))
        });
        nodes.clear();
        nodes.extend(ranked.into_iter().take(ACCESS_SEED_LIMIT));
    };
    let all_targets = targets.clone();
    nearest_nodes(&mut starts, &all_targets);
    nearest_nodes(&mut targets, &starts);
    if trace {
        eprintln!(
            "open-track net {net}: {}x{} tracks, {} layers, {} starts, {} targets",
            xs.len(),
            ys.len(),
            max_layer,
            starts.len(),
            targets.len()
        );
    }
    if starts.is_empty() || targets.is_empty() {
        return None;
    }
    let target_min_x = targets
        .iter()
        .map(|node| xs[node.x])
        .fold(f64::INFINITY, f64::min);
    let target_max_x = targets
        .iter()
        .map(|node| xs[node.x])
        .fold(f64::NEG_INFINITY, f64::max);
    let target_min_y = targets
        .iter()
        .map(|node| ys[node.y])
        .fold(f64::INFINITY, f64::min);
    let target_max_y = targets
        .iter()
        .map(|node| ys[node.y])
        .fold(f64::NEG_INFINITY, f64::max);
    let dbu = rules.database_units_per_micron as f64;
    let distance_to_target_box = |node: TrackSearchNode| {
        let x = xs[node.x];
        let y = ys[node.y];
        let dx = if x < target_min_x {
            target_min_x - x
        } else if x > target_max_x {
            x - target_max_x
        } else {
            0.0
        };
        let dy = if y < target_min_y {
            target_min_y - y
        } else if y > target_max_y {
            y - target_max_y
        } else {
            0.0
        };
        ((dx + dy) * dbu).round() as i64
    };

    let mut queue = BinaryHeap::new();
    let mut costs = HashMap::<TrackSearchNode, i64>::new();
    let mut parents = HashMap::<TrackSearchNode, TrackSearchNode>::new();
    for start in starts {
        costs.insert(start, 0);
        queue.push(TrackSearchQueueEntry {
            estimated_total: distance_to_target_box(start),
            cost: 0,
            node: start,
        });
    }
    let via_penalty = (pitch * dbu * 4.0).round() as i64;
    let mut reached = None;
    let mut expanded = 0usize;
    while let Some(entry) = queue.pop() {
        if costs
            .get(&entry.node)
            .is_some_and(|cost| *cost != entry.cost)
        {
            continue;
        }
        if targets.contains(&entry.node) {
            reached = Some(entry.node);
            break;
        }
        expanded += 1;
        let mut neighbors = Vec::with_capacity(6);
        if entry.node.x > 0 {
            neighbors.push(TrackSearchNode {
                x: entry.node.x - 1,
                ..entry.node
            });
        }
        if entry.node.x + 1 < xs.len() {
            neighbors.push(TrackSearchNode {
                x: entry.node.x + 1,
                ..entry.node
            });
        }
        if entry.node.y > 0 {
            neighbors.push(TrackSearchNode {
                y: entry.node.y - 1,
                ..entry.node
            });
        }
        if entry.node.y + 1 < ys.len() {
            neighbors.push(TrackSearchNode {
                y: entry.node.y + 1,
                ..entry.node
            });
        }
        if entry.node.layer > 1 {
            neighbors.push(TrackSearchNode {
                layer: entry.node.layer - 1,
                ..entry.node
            });
        }
        if entry.node.layer < max_layer {
            neighbors.push(TrackSearchNode {
                layer: entry.node.layer + 1,
                ..entry.node
            });
        }
        for neighbor in neighbors {
            let start = (xs[entry.node.x], ys[entry.node.y]);
            let end = (xs[neighbor.x], ys[neighbor.y]);
            let mut edge = Vec::new();
            let edge_cost = if neighbor.layer == entry.node.layer {
                edge.push(line_shape(
                    start,
                    end,
                    PhysicalLayer::Metal(entry.node.layer),
                    Some(net),
                    rules,
                ));
                (((start.0 - end.0).abs() + (start.1 - end.1).abs()) * dbu).round() as i64
            } else {
                add_via_stack(
                    &mut edge,
                    start,
                    entry.node.layer,
                    neighbor.layer,
                    net,
                    rules,
                );
                via_penalty
            };
            for shape in &mut edge {
                snap_shape_to_grid(shape, grid);
            }
            edge.retain(|shape| !exact_shape_exists(shapes, shape));
            if !edge.is_empty()
                && !routing_candidate_is_legal(canvas, &edge, rules, "open-track-edge")
            {
                continue;
            }
            let next_cost = entry.cost.saturating_add(edge_cost.max(1));
            if costs
                .get(&neighbor)
                .is_some_and(|existing| *existing <= next_cost)
            {
                continue;
            }
            costs.insert(neighbor, next_cost);
            parents.insert(neighbor, entry.node);
            queue.push(TrackSearchQueueEntry {
                estimated_total: next_cost.saturating_add(distance_to_target_box(neighbor)),
                cost: next_cost,
                node: neighbor,
            });
        }
    }
    if trace {
        eprintln!(
            "open-track net {net}: expanded {expanded} nodes; reached={}",
            reached.is_some()
        );
    }
    let mut current = reached?;
    let mut path = vec![current];
    while let Some(parent) = parents.get(&current).copied() {
        path.push(parent);
        current = parent;
    }
    path.reverse();
    let mut candidate = Vec::new();
    for pair in path.windows(2) {
        let start = (xs[pair[0].x], ys[pair[0].y]);
        let end = (xs[pair[1].x], ys[pair[1].y]);
        if pair[0].layer == pair[1].layer {
            candidate.push(line_shape(
                start,
                end,
                PhysicalLayer::Metal(pair[0].layer),
                Some(net),
                rules,
            ));
        } else {
            add_via_stack(
                &mut candidate,
                start,
                pair[0].layer,
                pair[1].layer,
                net,
                rules,
            );
        }
    }
    for shape in &mut candidate {
        snap_shape_to_grid(shape, grid);
    }
    candidate.retain(|shape| !exact_shape_exists(shapes, shape));
    routing_candidate_is_legal(canvas, &candidate, rules, "open-track-route").then_some(candidate)
}

fn island_launch_candidates(
    canvas: &PhysicalCanvas,
    shapes: &[PhysicalShape],
    island: &[usize],
    net: usize,
    max_layer: u16,
    rules: &PhysicalRuleDeck,
) -> Vec<((f64, f64), u16, Vec<PhysicalShape>)> {
    let mut launches = Vec::new();
    for index in island {
        let shape = &shapes[*index];
        let PhysicalLayer::Metal(from_layer) = shape.layer else {
            continue;
        };
        let sample_pitch = (metal_rule(rules, from_layer).min_width_um
            + metal_rule(rules, from_layer).min_spacing_um)
            .max(rules.manufacturing_grid_um);
        let mut points = vec![(shape.x, shape.y)];
        let horizontal = shape.width >= shape.height;
        let span = if horizontal {
            shape.width
        } else {
            shape.height
        };
        let samples = (span / sample_pitch).floor().min(32.0) as usize;
        for sample in 0..=samples {
            let offset = -span / 2.0 + sample as f64 * span / samples.max(1) as f64;
            points.push(if horizontal {
                (shape.x + offset, shape.y)
            } else {
                (shape.x, shape.y + offset)
            });
        }
        for point in &mut points {
            point.0 = snap_to_grid(point.0, rules.manufacturing_grid_um);
            point.1 = snap_to_grid(point.1, rules.manufacturing_grid_um);
        }
        points.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
        });
        points.dedup_by(|left, right| {
            (left.0 - right.0).abs() <= 1e-9 && (left.1 - right.1).abs() <= 1e-9
        });
        // Explore the nearest additional layer first. Higher layers remain
        // available when the lower escape is blocked, but do not preempt a
        // shorter legal repair merely because they were enumerated first.
        for route_layer in 2..=max_layer {
            for point in &points {
                let mut stack = Vec::new();
                add_via_stack(&mut stack, *point, from_layer, route_layer, net, rules);
                for shape in &mut stack {
                    snap_shape_to_grid(shape, rules.manufacturing_grid_um);
                }
                stack.retain(|shape| !exact_shape_exists(shapes, shape));
                if stack.is_empty() {
                    launches.push((*point, route_layer, stack));
                } else {
                    if routing_candidate_is_legal(canvas, &stack, rules, &format!("launch-{net}")) {
                        launches.push((*point, route_layer, stack));
                    }
                }
            }
        }
    }
    launches.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| left.0 .0.total_cmp(&right.0 .0))
            .then_with(|| left.0 .1.total_cmp(&right.0 .1))
            .then_with(|| left.2.len().cmp(&right.2.len()))
    });
    launches.dedup_by(|left, right| {
        left.1 == right.1
            && (left.0 .0 - right.0 .0).abs() <= 1e-9
            && (left.0 .1 - right.0 .1).abs() <= 1e-9
    });
    launches
}

fn repair_disconnected_routing(
    shapes: &mut Vec<PhysicalShape>,
    nets: &[PhysicalNet],
    rules: &PhysicalRuleDeck,
    allow_multilayer_search: bool,
) -> usize {
    let mut canvas = PhysicalCanvas::new(rules);
    for (index, shape) in shapes.iter().enumerate() {
        let obstruction = match shape.layer {
            PhysicalLayer::Metal(_) => ObstructionType::Metal,
            PhysicalLayer::Via(_) => ObstructionType::Via,
            PhysicalLayer::Contact => ObstructionType::Contact,
            PhysicalLayer::Poly => ObstructionType::Poly,
            PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => ObstructionType::Diffusion,
            PhysicalLayer::Pwell | PhysicalLayer::Nwell | PhysicalLayer::Substrate => {
                ObstructionType::Device
            }
        };
        canvas.index_unchecked(shape, format!("repair-seed-{index}"), obstruction);
    }
    let mut repaired = 0usize;
    for net in nets {
        for _ in 0..net.terminals.len().max(1) {
            let islands = terminal_routing_islands(shapes, net.id);
            if islands.len() <= 1 {
                break;
            }
            let left = &islands[0];
            let mut committed = false;
            'islands: for right in islands.iter().skip(1).take(1) {
                let max_layer = shapes
                    .iter()
                    .filter_map(|shape| match shape.layer {
                        PhysicalLayer::Metal(layer) => Some(layer),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(1);
                // Obstacle boundaries are properties of the current canvas,
                // not of an anchor pair. Compute them once per repair instead
                // of rescanning every routed shape for every pair and layer.
                let obstacle_tracks = if allow_multilayer_search {
                    HashMap::new()
                } else {
                    (1..=max_layer)
                        .map(|layer| {
                            let rule = metal_rule(rules, layer);
                            let clearance = rule.min_spacing_um
                                + rule.min_width_um / 2.0
                                + rules.manufacturing_grid_um;
                            let mut horizontal = Vec::new();
                            let mut vertical = Vec::new();
                            for shape in shapes.iter().filter(|shape| {
                                shape.layer == PhysicalLayer::Metal(layer)
                                    && shape.net != Some(net.id)
                            }) {
                                horizontal.push(snap_to_grid(
                                    shape.y - shape.height / 2.0 - clearance,
                                    rules.manufacturing_grid_um,
                                ));
                                horizontal.push(snap_to_grid(
                                    shape.y + shape.height / 2.0 + clearance,
                                    rules.manufacturing_grid_um,
                                ));
                                vertical.push(snap_to_grid(
                                    shape.x - shape.width / 2.0 - clearance,
                                    rules.manufacturing_grid_um,
                                ));
                                vertical.push(snap_to_grid(
                                    shape.x + shape.width / 2.0 + clearance,
                                    rules.manufacturing_grid_um,
                                ));
                            }
                            horizontal.sort_by(|left, right| left.total_cmp(right));
                            horizontal.dedup_by(|left, right| (*left - *right).abs() <= 1e-9);
                            vertical.sort_by(|left, right| left.total_cmp(right));
                            vertical.dedup_by(|left, right| (*left - *right).abs() <= 1e-9);
                            (layer, (horizontal, vertical))
                        })
                        .collect::<HashMap<_, _>>()
                };
                let mut common = left
                    .iter()
                    .filter_map(|index| match shapes[*index].layer {
                        PhysicalLayer::Metal(layer) => Some(layer),
                        _ => None,
                    })
                    .filter(|layer| {
                        right
                            .iter()
                            .any(|index| shapes[*index].layer == PhysicalLayer::Metal(*layer))
                    })
                    .collect::<Vec<_>>();
                common.sort_unstable();
                common.dedup();
                common.reverse();
                // The selected-candidate pass has already exhausted these
                // bounded same-layer families during parallel screening.
                if allow_multilayer_search {
                    common.clear();
                }
                for layer in common {
                    let pitch = metal_rule(rules, layer).min_width_um
                        + metal_rule(rules, layer).min_spacing_um
                        + rules.manufacturing_grid_um * 2.0;
                    let mut anchor_pairs = Vec::new();
                    for left_index in left {
                        for right_index in right {
                            let left_shape = &shapes[*left_index];
                            let right_shape = &shapes[*right_index];
                            if left_shape.layer == PhysicalLayer::Metal(layer)
                                && right_shape.layer == PhysicalLayer::Metal(layer)
                            {
                                anchor_pairs.push((
                                    (left_shape.x - right_shape.x).abs()
                                        + (left_shape.y - right_shape.y).abs(),
                                    *left_index,
                                    *right_index,
                                ));
                            }
                        }
                    }
                    anchor_pairs.sort_by(|left, right| {
                        left.0
                            .total_cmp(&right.0)
                            .then_with(|| left.1.cmp(&right.1))
                            .then_with(|| left.2.cmp(&right.2))
                    });
                    for (_, left_index, right_index) in
                        anchor_pairs.into_iter().take(REPAIR_ANCHOR_PAIR_LIMIT)
                    {
                        let start = (shapes[left_index].x, shapes[left_index].y);
                        let end = (shapes[right_index].x, shapes[right_index].y);
                        let mut horizontal_tracks = vec![start.1, end.1];
                        let mut vertical_tracks = vec![start.0, end.0];
                        if let Some((horizontal, vertical)) = obstacle_tracks.get(&layer) {
                            horizontal_tracks.extend(horizontal);
                            vertical_tracks.extend(vertical);
                        }
                        for track in &mut horizontal_tracks {
                            *track = snap_to_grid(*track, rules.manufacturing_grid_um);
                        }
                        for track in &mut vertical_tracks {
                            *track = snap_to_grid(*track, rules.manufacturing_grid_um);
                        }
                        horizontal_tracks.sort_by(|left, right| left.total_cmp(right));
                        horizontal_tracks.dedup_by(|left, right| (*left - *right).abs() <= 1e-9);
                        vertical_tracks.sort_by(|left, right| left.total_cmp(right));
                        vertical_tracks.dedup_by(|left, right| (*left - *right).abs() <= 1e-9);
                        let mut ranked_paths = vec![
                            (
                                (start.0 - end.0).abs() + (start.1 - end.1).abs(),
                                vec![start, (end.0, start.1), end],
                            ),
                            (
                                (start.0 - end.0).abs() + (start.1 - end.1).abs(),
                                vec![start, (start.0, end.1), end],
                            ),
                        ];
                        ranked_paths.extend(horizontal_tracks.into_iter().map(|y| {
                            (
                                (start.1 - y).abs() + (start.0 - end.0).abs() + (end.1 - y).abs(),
                                vec![start, (start.0, y), (end.0, y), end],
                            )
                        }));
                        ranked_paths.extend(vertical_tracks.into_iter().map(|x| {
                            (
                                (start.0 - x).abs() + (start.1 - end.1).abs() + (end.0 - x).abs(),
                                vec![start, (x, start.1), (x, end.1), end],
                            )
                        }));
                        for step in 1..=48 {
                            for direction in [-1.0, 1.0] {
                                let offset = direction * pitch * step as f64;
                                let y = start.1 + offset;
                                ranked_paths.push((
                                    (start.1 - y).abs()
                                        + (start.0 - end.0).abs()
                                        + (end.1 - y).abs(),
                                    vec![start, (start.0, y), (end.0, y), end],
                                ));
                                let x = start.0 + offset;
                                ranked_paths.push((
                                    (start.0 - x).abs()
                                        + (start.1 - end.1).abs()
                                        + (end.0 - x).abs(),
                                    vec![start, (x, start.1), (x, end.1), end],
                                ));
                            }
                        }
                        ranked_paths.sort_by(|left, right| left.0.total_cmp(&right.0));
                        for (_, path) in ranked_paths.into_iter().take(REPAIR_PATH_LIMIT) {
                            let mut candidate = path
                                .windows(2)
                                .filter(|segment| {
                                    (segment[0].0 - segment[1].0).abs() > 1e-9
                                        || (segment[0].1 - segment[1].1).abs() > 1e-9
                                })
                                .map(|segment| {
                                    line_shape(
                                        segment[0],
                                        segment[1],
                                        PhysicalLayer::Metal(layer),
                                        Some(net.id),
                                        rules,
                                    )
                                })
                                .collect::<Vec<_>>();
                            for shape in &mut candidate {
                                snap_shape_to_grid(shape, rules.manufacturing_grid_um);
                            }
                            if canvas
                                .commit_routing_geometry(
                                    &candidate,
                                    format!("open-repair-{}", net.id),
                                )
                                .is_ok()
                            {
                                shapes.extend(candidate);
                                repaired += 1;
                                committed = true;
                                break 'islands;
                            }
                        }
                    }
                }
                // Production uses one coarse, spatially bounded multilayer
                // corridor. Finer lattices remain opt-in until edge legality
                // and congestion are cached incrementally.
                if allow_multilayer_search {
                    let track_divisors: &[u8] =
                        if std::env::var_os("OPENCHIPPY_EXPERIMENTAL_ASTAR").is_some() {
                            &[1, 2, 4]
                        } else {
                            &[1]
                        };
                    for track_divisor in track_divisors.iter().copied() {
                        if let Some(candidate) = multilayer_track_search(
                            &canvas,
                            shapes,
                            left,
                            right,
                            net.id,
                            max_layer,
                            rules,
                            track_divisor,
                        ) {
                            if canvas
                                .commit_routing_geometry(
                                    &candidate,
                                    format!("open-track-repair-{}", net.id),
                                )
                                .is_ok()
                            {
                                shapes.extend(candidate);
                                repaired += 1;
                                committed = true;
                                break 'islands;
                            }
                        }
                    }
                }
                if allow_multilayer_search
                    && std::env::var_os("OPENCHIPPY_ROUTER_TRACE").is_some()
                    && !committed
                {
                    eprintln!(
                        "open-track net {} ({}) remained open: {:?}; left={:?}; right={:?}",
                        net.id,
                        net.name,
                        net.terminals,
                        left.iter().map(|index| &shapes[*index]).collect::<Vec<_>>(),
                        right
                            .iter()
                            .map(|index| &shapes[*index])
                            .collect::<Vec<_>>()
                    );
                }
                if std::env::var_os("OPENCHIPPY_TRACK_ONLY").is_some() {
                    break 'islands;
                }
                if allow_multilayer_search {
                    // Do not repeat the cross-layer dogleg and launch families
                    // already evaluated during candidate screening.
                    break 'islands;
                }
                let mut cross_layer_pairs = Vec::new();
                for left_index in left {
                    for right_index in right {
                        let (PhysicalLayer::Metal(left_layer), PhysicalLayer::Metal(right_layer)) =
                            (shapes[*left_index].layer, shapes[*right_index].layer)
                        else {
                            continue;
                        };
                        cross_layer_pairs.push((
                            (shapes[*left_index].x - shapes[*right_index].x).abs()
                                + (shapes[*left_index].y - shapes[*right_index].y).abs(),
                            *left_index,
                            left_layer,
                            *right_index,
                            right_layer,
                        ));
                    }
                }
                cross_layer_pairs.sort_by(|left, right| {
                    left.0
                        .total_cmp(&right.0)
                        .then_with(|| left.1.cmp(&right.1))
                        .then_with(|| left.3.cmp(&right.3))
                });
                let mut best_layer_repair: Option<(f64, usize, u16, Vec<PhysicalShape>)> = None;
                for (anchor_distance, left_index, left_layer, right_index, right_layer) in
                    cross_layer_pairs.into_iter().take(REPAIR_ANCHOR_PAIR_LIMIT)
                {
                    if best_layer_repair
                        .as_ref()
                        .is_some_and(|(score, _, _, _)| anchor_distance >= *score - 1e-9)
                    {
                        // Manhattan anchor distance is a lower bound for every
                        // route from this and all later (distance-sorted)
                        // pairs. No remaining candidate can beat the best
                        // complete legal repair.
                        break;
                    }
                    let start = (shapes[left_index].x, shapes[left_index].y);
                    let end = (shapes[right_index].x, shapes[right_index].y);
                    let first_route_layer = left_layer.max(right_layer).max(2);
                    for route_layer in first_route_layer..=max_layer {
                        let route_rule = metal_rule(rules, route_layer);
                        let pitch = route_rule.min_width_um
                            + route_rule.min_spacing_um
                            + rules.manufacturing_grid_um * 2.0;
                        let mut paths = vec![
                            vec![start, (end.0, start.1), end],
                            vec![start, (start.0, end.1), end],
                        ];
                        if let Some((horizontal, vertical)) = obstacle_tracks.get(&route_layer) {
                            for y in horizontal {
                                paths.push(vec![start, (start.0, *y), (end.0, *y), end]);
                            }
                            for x in vertical {
                                paths.push(vec![start, (*x, start.1), (*x, end.1), end]);
                            }
                        }
                        let layout_span = shapes
                            .iter()
                            .map(|shape| shape.width.max(shape.height))
                            .fold(0.0, f64::max)
                            .max((start.0 - end.0).abs())
                            .max((start.1 - end.1).abs());
                        let track_span = (layout_span / pitch).ceil().max(1.0) as usize;
                        for step in 1..=track_span {
                            for direction in [-1.0, 1.0] {
                                let y = start.1 + direction * pitch * step as f64;
                                paths.push(vec![start, (start.0, y), (end.0, y), end]);
                                let x = start.0 + direction * pitch * step as f64;
                                paths.push(vec![start, (x, start.1), (x, end.1), end]);
                            }
                        }
                        paths.sort_by(|left, right| {
                            let length = |path: &Vec<(f64, f64)>| {
                                path.windows(2)
                                    .map(|segment| {
                                        (segment[0].0 - segment[1].0).abs()
                                            + (segment[0].1 - segment[1].1).abs()
                                    })
                                    .sum::<f64>()
                            };
                            length(left).total_cmp(&length(right))
                        });
                        paths.dedup();
                        for path in paths.into_iter().take(REPAIR_PATH_LIMIT) {
                            let path_length = path
                                .windows(2)
                                .map(|segment| {
                                    (segment[0].0 - segment[1].0).abs()
                                        + (segment[0].1 - segment[1].1).abs()
                                })
                                .sum::<f64>();
                            let mut candidate = Vec::new();
                            add_via_stack(
                                &mut candidate,
                                start,
                                left_layer,
                                route_layer,
                                net.id,
                                rules,
                            );
                            candidate.extend(
                                path.windows(2)
                                    .filter(|segment| {
                                        (segment[0].0 - segment[1].0).abs() > 1e-9
                                            || (segment[0].1 - segment[1].1).abs() > 1e-9
                                    })
                                    .map(|segment| {
                                        line_shape(
                                            segment[0],
                                            segment[1],
                                            PhysicalLayer::Metal(route_layer),
                                            Some(net.id),
                                            rules,
                                        )
                                    }),
                            );
                            add_via_stack(
                                &mut candidate,
                                end,
                                right_layer,
                                route_layer,
                                net.id,
                                rules,
                            );
                            for shape in &mut candidate {
                                snap_shape_to_grid(shape, rules.manufacturing_grid_um);
                            }
                            candidate.retain(|shape| !exact_shape_exists(shapes, shape));
                            if candidate.is_empty() {
                                continue;
                            }
                            if !routing_candidate_is_legal(
                                &canvas,
                                &candidate,
                                rules,
                                &format!("open-layer-trial-{}", net.id),
                            ) {
                                continue;
                            }
                            let via_count = candidate
                                .iter()
                                .filter(|shape| matches!(shape.layer, PhysicalLayer::Via(_)))
                                .count();
                            let escalation = route_layer.saturating_sub(first_route_layer);
                            // Compare complete legal alternatives rather than
                            // accepting the first upper-layer escape. Vias and
                            // layer escalation are expressed as an equivalent
                            // wire-length penalty so a higher layer wins only
                            // when it avoids enough detour.
                            let score = path_length
                                + via_count as f64 * pitch * 4.0
                                + escalation as f64 * pitch * 2.0;
                            let better = best_layer_repair.as_ref().is_none_or(
                                |(best_score, best_shapes, best_layer, _)| {
                                    score < *best_score - 1e-9
                                        || ((score - *best_score).abs() <= 1e-9
                                            && (candidate.len(), route_layer)
                                                < (*best_shapes, *best_layer))
                                },
                            );
                            if better {
                                best_layer_repair =
                                    Some((score, candidate.len(), route_layer, candidate));
                            }
                            // Paths are length-ordered. With the same anchors
                            // and route layer, every later path has identical
                            // via/escalation cost and cannot dominate this
                            // first legal path. This is branch-and-bound, not a
                            // runtime search budget.
                            break;
                        }
                    }
                }
                if let Some((_, _, _, candidate)) = best_layer_repair {
                    if canvas
                        .commit_routing_geometry(
                            &candidate,
                            format!("open-layer-repair-{}", net.id),
                        )
                        .is_ok()
                    {
                        shapes.extend(candidate);
                        repaired += 1;
                        committed = true;
                        break 'islands;
                    }
                }
                let left_launches =
                    island_launch_candidates(&canvas, shapes, left, net.id, max_layer, rules);
                let right_launches =
                    island_launch_candidates(&canvas, shapes, right, net.id, max_layer, rules);
                let min_x = shapes
                    .iter()
                    .filter(|shape| {
                        matches!(
                            shape.purpose,
                            PhysicalShapePurpose::Active
                                | PhysicalShapePurpose::Gate
                                | PhysicalShapePurpose::GateAccess
                                | PhysicalShapePurpose::Contact
                                | PhysicalShapePurpose::DeviceLanding
                                | PhysicalShapePurpose::Pin
                                | PhysicalShapePurpose::Tap
                        )
                    })
                    .map(|shape| shape.x - shape.width / 2.0)
                    .fold(f64::INFINITY, f64::min);
                let max_x = shapes
                    .iter()
                    .filter(|shape| {
                        matches!(
                            shape.purpose,
                            PhysicalShapePurpose::Active
                                | PhysicalShapePurpose::Gate
                                | PhysicalShapePurpose::GateAccess
                                | PhysicalShapePurpose::Contact
                                | PhysicalShapePurpose::DeviceLanding
                                | PhysicalShapePurpose::Pin
                                | PhysicalShapePurpose::Tap
                        )
                    })
                    .map(|shape| shape.x + shape.width / 2.0)
                    .fold(f64::NEG_INFINITY, f64::max);
                let min_y = shapes
                    .iter()
                    .filter(|shape| {
                        matches!(
                            shape.purpose,
                            PhysicalShapePurpose::Active
                                | PhysicalShapePurpose::Gate
                                | PhysicalShapePurpose::GateAccess
                                | PhysicalShapePurpose::Contact
                                | PhysicalShapePurpose::DeviceLanding
                                | PhysicalShapePurpose::Pin
                                | PhysicalShapePurpose::Tap
                        )
                    })
                    .map(|shape| shape.y - shape.height / 2.0)
                    .fold(f64::INFINITY, f64::min);
                let max_y = shapes
                    .iter()
                    .filter(|shape| {
                        matches!(
                            shape.purpose,
                            PhysicalShapePurpose::Active
                                | PhysicalShapePurpose::Gate
                                | PhysicalShapePurpose::GateAccess
                                | PhysicalShapePurpose::Contact
                                | PhysicalShapePurpose::DeviceLanding
                                | PhysicalShapePurpose::Pin
                                | PhysicalShapePurpose::Tap
                        )
                    })
                    .map(|shape| shape.y + shape.height / 2.0)
                    .fold(f64::NEG_INFINITY, f64::max);
                let mut best_launch_repair = None::<(f64, usize, u16, Vec<PhysicalShape>)>;
                for (start, route_layer, left_stack) in &left_launches {
                    for (end, right_route_layer, right_stack) in &right_launches {
                        if route_layer != right_route_layer {
                            continue;
                        }
                        let rule = metal_rule(rules, *route_layer);
                        let margin =
                            rule.min_width_um + rule.min_spacing_um + rules.manufacturing_grid_um;
                        let paths = [
                            vec![*start, (end.0, start.1), *end],
                            vec![*start, (start.0, end.1), *end],
                            vec![
                                *start,
                                (start.0, min_y - margin),
                                (end.0, min_y - margin),
                                *end,
                            ],
                            vec![
                                *start,
                                (start.0, max_y + margin),
                                (end.0, max_y + margin),
                                *end,
                            ],
                            vec![
                                *start,
                                (min_x - margin, start.1),
                                (min_x - margin, end.1),
                                *end,
                            ],
                            vec![
                                *start,
                                (max_x + margin, start.1),
                                (max_x + margin, end.1),
                                *end,
                            ],
                        ];
                        for path in paths {
                            let mut candidate = left_stack.clone();
                            candidate.extend(
                                path.windows(2)
                                    .filter(|segment| {
                                        (segment[0].0 - segment[1].0).abs() > 1e-9
                                            || (segment[0].1 - segment[1].1).abs() > 1e-9
                                    })
                                    .map(|segment| {
                                        line_shape(
                                            segment[0],
                                            segment[1],
                                            PhysicalLayer::Metal(*route_layer),
                                            Some(net.id),
                                            rules,
                                        )
                                    }),
                            );
                            candidate.extend(right_stack.clone());
                            for shape in &mut candidate {
                                snap_shape_to_grid(shape, rules.manufacturing_grid_um);
                            }
                            candidate.retain(|shape| !exact_shape_exists(shapes, shape));
                            if candidate.is_empty()
                                || !routing_candidate_is_legal(
                                    &canvas,
                                    &candidate,
                                    rules,
                                    "open-launch-repair",
                                )
                            {
                                continue;
                            }
                            let route_length = candidate
                                .iter()
                                .filter(|shape| matches!(shape.layer, PhysicalLayer::Metal(_)))
                                .map(|shape| shape.width.max(shape.height))
                                .sum::<f64>();
                            let excursion = path
                                .iter()
                                .map(|point| {
                                    (min_x - point.0).max(0.0)
                                        + (point.0 - max_x).max(0.0)
                                        + (min_y - point.1).max(0.0)
                                        + (point.1 - max_y).max(0.0)
                                })
                                .sum::<f64>();
                            let score = route_length + excursion * 8.0;
                            let better = best_launch_repair.as_ref().is_none_or(
                                |(best_score, best_shapes, best_layer, _)| {
                                    score < *best_score - 1e-9
                                        || ((score - *best_score).abs() <= 1e-9
                                            && (candidate.len(), *route_layer)
                                                < (*best_shapes, *best_layer))
                                },
                            );
                            if better {
                                best_launch_repair =
                                    Some((score, candidate.len(), *route_layer, candidate));
                            }
                        }
                    }
                }
                if let Some((_, _, _, candidate)) = best_launch_repair {
                    if canvas
                        .commit_routing_geometry(
                            &candidate,
                            format!("open-launch-repair-{}", net.id),
                        )
                        .is_ok()
                    {
                        shapes.extend(candidate);
                        repaired += 1;
                        committed = true;
                        break 'islands;
                    }
                }
            }
            if !committed {
                break;
            }
        }
    }
    repaired
}

/// Split generated axial metal at electrical junctions before terminal-tree
/// reduction.
///
/// The router represents a long trunk as one rectangle.  A shortest-tree pass
/// over those rectangles cannot remove only the unused portion beyond an
/// interior junction: retaining the junction retains the complete trunk.  The
/// fragments produced here have exactly the same boolean union as the input,
/// remain grid aligned, and individually satisfy the process minimum area.
/// Pins, device/via landings, and power rails stay atomic because their full
/// enclosure or external-access geometry is part of the physical contract.
fn fragment_generated_routes_at_junctions(
    shapes: &mut Vec<PhysicalShape>,
    rules: &PhysicalRuleDeck,
) -> usize {
    let grid = rules.manufacturing_grid_um;
    let snapshot = shapes.clone();
    let mut result = Vec::with_capacity(shapes.len());
    let mut fragments_added = 0usize;

    for (index, shape) in snapshot.iter().enumerate() {
        let PhysicalLayer::Metal(layer) = shape.layer else {
            result.push(shape.clone());
            continue;
        };
        if shape.net.is_none()
            || !matches!(
                shape.purpose,
                PhysicalShapePurpose::Route | PhysicalShapePurpose::Unknown
            )
            || (shape.component_id.is_some() && !is_generated_route_shape(shape))
        {
            result.push(shape.clone());
            continue;
        }
        let horizontal = shape.width > shape.height + grid;
        let vertical = shape.height > shape.width + grid;
        if !horizontal && !vertical {
            result.push(shape.clone());
            continue;
        }

        let axis_min = if horizontal {
            shape.x - shape.width / 2.0
        } else {
            shape.y - shape.height / 2.0
        };
        let axis_max = if horizontal {
            shape.x + shape.width / 2.0
        } else {
            shape.y + shape.height / 2.0
        };
        let mut cuts = snapshot
            .iter()
            .enumerate()
            .filter_map(|(other_index, other)| {
                if other_index == index
                    || other.net != shape.net
                    || !shapes_touch(shape, other, grid / 2.0)
                {
                    return None;
                }
                let junction = match other.layer {
                    PhysicalLayer::Metal(other_layer) if other_layer == layer => {
                        let other_horizontal = other.width > other.height + grid;
                        let other_vertical = other.height > other.width + grid;
                        if (horizontal && other_vertical) || (vertical && other_horizontal) {
                            Some(if horizontal { other.x } else { other.y })
                        } else {
                            None
                        }
                    }
                    PhysicalLayer::Via(lower) if layer == lower || layer == lower + 1 => {
                        Some(if horizontal { other.x } else { other.y })
                    }
                    PhysicalLayer::Contact if layer == 1 => {
                        Some(if horizontal { other.x } else { other.y })
                    }
                    _ => None,
                }?;
                (junction > axis_min + grid && junction < axis_max - grid).then_some(junction)
            })
            .collect::<Vec<_>>();
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|left, right| (*left - *right).abs() <= grid / 2.0);
        if cuts.is_empty() {
            result.push(shape.clone());
            continue;
        }

        let mut boundaries = Vec::with_capacity(cuts.len() + 2);
        boundaries.push(axis_min);
        boundaries.extend(cuts);
        boundaries.push(axis_max);
        let rule = metal_rule(rules, layer);
        let cross_width = if horizontal {
            shape.height
        } else {
            shape.width
        };
        let raw_minimum_length = rule
            .min_width_um
            .max(rule.min_area_um2 / cross_width.max(grid));
        // Shape dimensions are center/size encoded, so both axial edges must
        // land on the grid. Round the minimum upward by a complete two-edge
        // quantum and leave one quantum of numerical margin; otherwise a
        // nominally exact min-area fragment can snap just below the rule.
        let edge_quantum = grid * 2.0;
        let minimum_length =
            (raw_minimum_length / edge_quantum).ceil() * edge_quantum + edge_quantum;
        for interval in boundaries.windows(2) {
            let mut start = interval[0];
            let mut end = interval[1];
            if end - start < minimum_length {
                let center = (start + end) / 2.0;
                start = (center - minimum_length / 2.0).max(axis_min);
                end = (center + minimum_length / 2.0).min(axis_max);
                if end - start < minimum_length - grid / 2.0 {
                    if (start - axis_min).abs() <= grid / 2.0 {
                        end = (start + minimum_length).min(axis_max);
                    } else {
                        start = (end - minimum_length).max(axis_min);
                    }
                }
            }
            let mut fragment = shape.clone();
            if horizontal {
                fragment.x = (start + end) / 2.0;
                fragment.width = end - start;
            } else {
                fragment.y = (start + end) / 2.0;
                fragment.height = end - start;
            }
            snap_shape_to_grid(&mut fragment, grid);
            result.push(fragment);
        }
        fragments_added += boundaries.len().saturating_sub(2);
    }
    *shapes = result;
    fragments_added
}

fn prune_orphan_routing(shapes: &mut Vec<PhysicalShape>, rules: &PhysicalRuleDeck) -> usize {
    fragment_generated_routes_at_junctions(shapes, rules);
    let grid = rules.manufacturing_grid_um;
    let routing_indices = shapes
        .iter()
        .enumerate()
        .filter_map(|(index, shape)| {
            matches!(shape.layer, PhysicalLayer::Metal(_) | PhysicalLayer::Via(_)).then_some(index)
        })
        .collect::<Vec<_>>();
    let mut by_net = HashMap::<usize, Vec<usize>>::new();
    for index in &routing_indices {
        if let Some(net) = shapes[*index].net {
            by_net.entry(net).or_default().push(*index);
        }
    }
    let mut retained = HashSet::new();
    for indices in by_net.values() {
        let mut adjacency = HashMap::<usize, Vec<usize>>::new();
        for (position, index) in indices.iter().enumerate() {
            for candidate in indices.iter().skip(position + 1) {
                if routing_shapes_connect(&shapes[*index], &shapes[*candidate], grid / 2.0) {
                    adjacency.entry(*index).or_default().push(*candidate);
                    adjacency.entry(*candidate).or_default().push(*index);
                }
            }
        }
        let mut terminals = HashSet::new();
        for index in indices {
            let shape = &shapes[*index];
            // A local-route marker records provenance for transactional
            // admission; it is not an electrical pin and must not keep a
            // rejected/dangling branch alive during final cleanup.
            let is_pin = matches!(
                shape.purpose,
                PhysicalShapePurpose::Pin | PhysicalShapePurpose::PowerRail
            ) || (shape.purpose == PhysicalShapePurpose::Unknown
                && shape.component_id.is_some()
                && !is_internal_route_shape(shape));
            if is_pin {
                terminals.insert(*index);
            }
        }
        // A contact is one electrical terminal, even when several admitted
        // M1 rectangles overlap its landing. Keeping every touching rectangle
        // as a terminal made duplicate access bars and their empty-space
        // overhangs immutable. Select one canonical landing; all other M1 is
        // routing that the shortest terminal tree may remove.
        for contact in shapes.iter().filter(|shape| {
            shape.layer == PhysicalLayer::Contact
                && shape.net.is_some()
                && indices
                    .first()
                    .is_some_and(|index| shapes[*index].net == shape.net)
        }) {
            let canonical = indices
                .iter()
                .filter(|index| {
                    let shape = &shapes[**index];
                    shape.layer == PhysicalLayer::Metal(1)
                        && shape.net == contact.net
                        && shapes_touch(shape, contact, grid / 2.0)
                })
                .min_by(|left, right| {
                    let rank = |shape: &PhysicalShape| match shape.purpose {
                        PhysicalShapePurpose::DeviceLanding => 0,
                        PhysicalShapePurpose::ViaLanding => 1,
                        PhysicalShapePurpose::Pin => 2,
                        _ => 3,
                    };
                    rank(&shapes[**left])
                        .cmp(&rank(&shapes[**right]))
                        .then_with(|| {
                            (shapes[**left].width * shapes[**left].height)
                                .total_cmp(&(shapes[**right].width * shapes[**right].height))
                        })
                        .then_with(|| left.cmp(right))
                });
            if let Some(canonical) = canonical {
                terminals.insert(*canonical);
            }
        }

        let mut unconnected_terminals = terminals.clone();
        while let Some(root) = unconnected_terminals.iter().min().copied() {
            let mut costs = HashMap::<usize, i64>::new();
            let mut parents = HashMap::<usize, usize>::new();
            let mut queue = BinaryHeap::new();
            costs.insert(root, 0);
            queue.push((Reverse(0_i64), Reverse(root)));
            while let Some((Reverse(cost), Reverse(index))) = queue.pop() {
                if costs.get(&index).is_some_and(|known| *known != cost) {
                    continue;
                }
                for candidate in adjacency.get(&index).into_iter().flatten() {
                    let candidate_shape = &shapes[*candidate];
                    let length = match candidate_shape.layer {
                        PhysicalLayer::Metal(_) => {
                            candidate_shape.width.max(candidate_shape.height)
                        }
                        PhysicalLayer::Via(_) => grid,
                        _ => 0.0,
                    };
                    let next_cost = cost.saturating_add((length / grid).round().max(1.0) as i64);
                    if costs
                        .get(candidate)
                        .is_some_and(|known| *known <= next_cost)
                    {
                        continue;
                    }
                    costs.insert(*candidate, next_cost);
                    parents.insert(*candidate, index);
                    queue.push((Reverse(next_cost), Reverse(*candidate)));
                }
            }
            let reached = unconnected_terminals
                .iter()
                .filter(|terminal| costs.contains_key(terminal))
                .copied()
                .collect::<Vec<_>>();
            for terminal in &reached {
                let mut current = *terminal;
                retained.insert(current);
                while current != root {
                    let Some(parent) = parents.get(&current).copied() else {
                        break;
                    };
                    retained.insert(parent);
                    current = parent;
                }
            }
            for terminal in reached {
                unconnected_terminals.remove(&terminal);
            }
            if unconnected_terminals.remove(&root) {
                retained.insert(root);
            }
        }
        // A retained via and its two enclosure landings are one physical
        // connector. Leaf reduction may see a landing as a removable branch,
        // so restore the smallest same-net metal shape on each adjacent layer.
        let retained_vias = retained
            .iter()
            .filter(|index| indices.contains(index))
            .filter_map(|index| match shapes[*index].layer {
                PhysicalLayer::Via(lower) => Some((*index, lower)),
                _ => None,
            })
            .collect::<Vec<_>>();
        for (via_index, lower) in retained_vias {
            let cut = rules
                .via_overrides
                .get(&format!("via{lower}{}", lower + 1))
                .unwrap_or(&rules.via);
            for required_layer in [lower, lower + 1] {
                if let Some(landing) = adjacency
                    .get(&via_index)
                    .into_iter()
                    .flatten()
                    .filter(|candidate| {
                        shapes[**candidate].layer == PhysicalLayer::Metal(required_layer)
                            && contains_shape(
                                &shapes[**candidate],
                                &shapes[via_index],
                                cut.enclosure_um,
                            )
                    })
                    .min_by(|left, right| {
                        let left_area = shapes[**left].width * shapes[**left].height;
                        let right_area = shapes[**right].width * shapes[**right].height;
                        left_area.total_cmp(&right_area)
                    })
                {
                    retained.insert(*landing);
                }
            }
        }
    }
    let before = shapes.len();
    let mut index = 0usize;
    shapes.retain(|shape| {
        let keep = !matches!(shape.layer, PhysicalLayer::Metal(_) | PhysicalLayer::Via(_))
            || retained.contains(&index);
        index += 1;
        keep
    });
    before - shapes.len()
}

fn trim_metal_overhangs(shapes: &mut Vec<PhysicalShape>, rules: &PhysicalRuleDeck) -> usize {
    let grid = rules.manufacturing_grid_um;
    let snapshot = shapes.clone();
    let mut trimmed = 0;
    for (index, shape) in shapes.iter_mut().enumerate() {
        let PhysicalLayer::Metal(layer) = shape.layer else {
            continue;
        };
        // Device landings and external pins are immutable terminals. Internal
        // route markers, however, are provenance rather than terminals and
        // must not exempt M1 access branches from final overhang cleanup.
        if shape.net.is_none() || (shape.component_id.is_some() && !is_generated_route_shape(shape))
        {
            continue;
        }
        let horizontal = shape.width > shape.height + grid;
        let vertical = shape.height > shape.width + grid;
        if !horizontal && !vertical {
            continue;
        }
        let mut attachment_ranges = Vec::new();
        for (other_index, other) in snapshot.iter().enumerate() {
            if other_index == index
                || other.net != shape.net
                || !routing_shapes_connect(shape, other, grid / 2.0)
            {
                continue;
            }
            if horizontal {
                attachment_ranges.push((
                    (other.x - other.width / 2.0).max(shape.x - shape.width / 2.0),
                    (other.x + other.width / 2.0).min(shape.x + shape.width / 2.0),
                ));
            } else {
                attachment_ranges.push((
                    (other.y - other.height / 2.0).max(shape.y - shape.height / 2.0),
                    (other.y + other.height / 2.0).min(shape.y + shape.height / 2.0),
                ));
            }
        }
        for terminal in snapshot.iter().filter(|other| {
            other.net == shape.net
                && other.layer == PhysicalLayer::Contact
                && shapes_touch(shape, other, grid / 2.0)
        }) {
            if horizontal {
                attachment_ranges.push((
                    (terminal.x - terminal.width / 2.0).max(shape.x - shape.width / 2.0),
                    (terminal.x + terminal.width / 2.0).min(shape.x + shape.width / 2.0),
                ));
            } else {
                attachment_ranges.push((
                    (terminal.y - terminal.height / 2.0).max(shape.y - shape.height / 2.0),
                    (terminal.y + terminal.height / 2.0).min(shape.y + shape.height / 2.0),
                ));
            }
        }
        attachment_ranges.retain(|(start, end)| end + grid / 2.0 >= *start);
        if attachment_ranges.len() < 2 {
            continue;
        }
        let attachment_min = attachment_ranges
            .iter()
            .map(|range| range.0)
            .fold(f64::INFINITY, f64::min);
        let attachment_max = attachment_ranges
            .iter()
            .map(|range| range.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let rule = metal_rule(rules, layer);
        let cross_width = if horizontal {
            shape.height
        } else {
            shape.width
        };
        let minimum_length = rule
            .min_width_um
            .max(rule.min_area_um2 / cross_width.max(grid));
        let new_length = (attachment_max - attachment_min).max(minimum_length);
        let old_length = if horizontal {
            shape.width
        } else {
            shape.height
        };
        if new_length >= old_length - grid {
            continue;
        }
        if horizontal {
            shape.x = (attachment_min + attachment_max) / 2.0;
            shape.width = new_length;
        } else {
            shape.y = (attachment_min + attachment_max) / 2.0;
            shape.height = new_length;
        }
        snap_shape_to_grid(shape, grid);
        let cross_width = if horizontal {
            shape.height
        } else {
            shape.width
        };
        let edge_quantum = grid * 2.0;
        let legal_length = (rule
            .min_width_um
            .max(rule.min_area_um2 / cross_width.max(grid))
            / edge_quantum)
            .ceil()
            * edge_quantum
            + edge_quantum;
        if horizontal {
            shape.width = shape.width.max(legal_length);
        } else {
            shape.height = shape.height.max(legal_length);
        }
        snap_shape_to_grid(shape, grid);
        while shape.width * shape.height + 1e-12 < rule.min_area_um2 {
            if horizontal {
                shape.width += edge_quantum;
            } else {
                shape.height += edge_quantum;
            }
            snap_shape_to_grid(shape, grid);
        }
        trimmed += 1;
    }
    trimmed
}

/// Fill sub-rule notches between fragments of one routed conductor.
///
/// Native connectivity correctly treats touching same-net rectangles as one
/// conductor, while a foundry deck evaluates the boolean union. Offset via
/// landings and bends can consequently leave a narrow inward-facing notch or
/// neck even though the net is connected. This pass only fills the local gap
/// or transition overlap between same-net rectangles. It never spans disjoint
/// projections, and a fill that approaches a foreign net is rejected.
fn fill_same_net_metal_notches(shapes: &mut Vec<PhysicalShape>, rules: &PhysicalRuleDeck) -> usize {
    let mut total = 0;
    // One bridge can expose the next concave corner in a composite landing.
    // Re-index after every accepted wave so the final boolean conductor, not
    // only the pre-fill rectangles, satisfies the minimum-width contract.
    for _ in 0..8 {
        let added = fill_same_net_metal_notches_once(shapes, rules);
        total += added;
        if added == 0 {
            break;
        }
    }
    total
}

fn fill_same_net_metal_notches_once(
    shapes: &mut Vec<PhysicalShape>,
    rules: &PhysicalRuleDeck,
) -> usize {
    const EPSILON: f64 = 1e-9;

    let snapshot = shapes.clone();
    let mut by_conductor = BTreeMap::<(u16, usize), Vec<usize>>::new();
    for (index, shape) in snapshot.iter().enumerate() {
        let (PhysicalLayer::Metal(layer), Some(net)) = (shape.layer, shape.net) else {
            continue;
        };
        by_conductor.entry((layer, net)).or_default().push(index);
    }

    let grid = rules.manufacturing_grid_um;
    let mut added = 0;
    let mut seen = HashSet::<(u16, usize, i64, i64, i64, i64)>::new();
    for ((layer, net), indices) in by_conductor {
        let rule = metal_rule(rules, layer);
        for left_ordinal in 0..indices.len() {
            let left = &snapshot[indices[left_ordinal]];
            let left_min_x = left.x - left.width / 2.0;
            let left_max_x = left.x + left.width / 2.0;
            let left_min_y = left.y - left.height / 2.0;
            let left_max_y = left.y + left.height / 2.0;
            for right_index in &indices[left_ordinal + 1..] {
                let right = &snapshot[*right_index];
                let right_min_x = right.x - right.width / 2.0;
                let right_max_x = right.x + right.width / 2.0;
                let right_min_y = right.y - right.height / 2.0;
                let right_max_y = right.y + right.height / 2.0;
                let overlap_x = left_max_x.min(right_max_x) - left_min_x.max(right_min_x);
                let overlap_y = left_max_y.min(right_max_y) - left_min_y.max(right_min_y);
                let gap_x = left_min_x.max(right_min_x) - left_max_x.min(right_max_x);
                let gap_y = left_min_y.max(right_min_y) - left_max_y.min(right_max_y);

                let mut fill_bounds = Vec::<(f64, f64, f64, f64)>::new();
                if gap_x > EPSILON && gap_x < rule.min_spacing_um - EPSILON && overlap_y > EPSILON {
                    let first_edge = left_max_x.min(right_max_x);
                    let second_edge = left_min_x.max(right_min_x);
                    fill_bounds.push((
                        first_edge.min(second_edge),
                        left_min_y.max(right_min_y),
                        first_edge.max(second_edge),
                        left_max_y.min(right_max_y),
                    ));
                }
                if gap_y > EPSILON && gap_y < rule.min_spacing_um - EPSILON && overlap_x > EPSILON {
                    let first_edge = left_max_y.min(right_max_y);
                    let second_edge = left_min_y.max(right_min_y);
                    fill_bounds.push((
                        left_min_x.max(right_min_x),
                        first_edge.min(second_edge),
                        left_max_x.min(right_max_x),
                        first_edge.max(second_edge),
                    ));
                }
                // Two same-net rectangles can be electrically connected by a
                // short shared edge while their boolean union still contains
                // a sub-rule neck.  This occurs most often where a minimum-
                // width route terminates against an offset square landing:
                // the rectangles have zero gap, so neither spacing repair nor
                // the positive-area overlap cases below see the junction.
                // Extend the route end across a full-width interval contained
                // by the landing.  A centered square creates four new concave
                // corners when it is offset from both rectangles; matching the
                // route's complete thickness leaves only the original two
                // corners, now separated by the process minimum width.
                let touching_tolerance = grid + EPSILON;
                let landing_route_pair = matches!(
                    (left.purpose, right.purpose),
                    (
                        PhysicalShapePurpose::ViaLanding | PhysicalShapePurpose::DeviceLanding,
                        PhysicalShapePurpose::Route
                    ) | (
                        PhysicalShapePurpose::Route,
                        PhysicalShapePurpose::ViaLanding | PhysicalShapePurpose::DeviceLanding
                    )
                );
                if landing_route_pair
                    && gap_y.abs() <= touching_tolerance
                    && overlap_x > EPSILON
                    && overlap_x < rule.min_width_um - EPSILON
                {
                    let (landing, route) = if matches!(
                        left.purpose,
                        PhysicalShapePurpose::ViaLanding | PhysicalShapePurpose::DeviceLanding
                    ) {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    let landing_min_x = landing.x - landing.width / 2.0;
                    let landing_max_x = landing.x + landing.width / 2.0;
                    if landing.width + EPSILON >= rule.min_width_um {
                        let shared_center = (left_min_x.max(right_min_x)
                            + left_max_x.min(right_max_x))
                            / 2.0;
                        let min_x = (shared_center - rule.min_width_um / 2.0)
                            .clamp(landing_min_x, landing_max_x - rule.min_width_um);
                        fill_bounds.push((
                            min_x,
                            route.y - route.height / 2.0,
                            min_x + rule.min_width_um,
                            route.y + route.height / 2.0,
                        ));
                    }
                }
                if landing_route_pair
                    && gap_x.abs() <= touching_tolerance
                    && overlap_y > EPSILON
                    && overlap_y < rule.min_width_um - EPSILON
                {
                    let (landing, route) = if matches!(
                        left.purpose,
                        PhysicalShapePurpose::ViaLanding | PhysicalShapePurpose::DeviceLanding
                    ) {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    let landing_min_y = landing.y - landing.height / 2.0;
                    let landing_max_y = landing.y + landing.height / 2.0;
                    if landing.height + EPSILON >= rule.min_width_um {
                        let shared_center = (left_min_y.max(right_min_y)
                            + left_max_y.min(right_max_y))
                            / 2.0;
                        let min_y = (shared_center - rule.min_width_um / 2.0)
                            .clamp(landing_min_y, landing_max_y - rule.min_width_um);
                        fill_bounds.push((
                            route.x - route.width / 2.0,
                            min_y,
                            route.x + route.width / 2.0,
                            min_y + rule.min_width_um,
                        ));
                    }
                }
                let left_horizontal = left.width > left.height + grid;
                let left_vertical = left.height > left.width + grid;
                let right_horizontal = right.width > right.height + grid;
                let right_vertical = right.height > right.width + grid;
                if overlap_x > EPSILON
                    && overlap_y > EPSILON
                    && overlap_x < rule.min_width_um - EPSILON
                    && overlap_y < rule.min_width_um - EPSILON
                    && ((left_horizontal && right_vertical) || (left_vertical && right_horizontal))
                {
                    let (horizontal, vertical) = if left_horizontal {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    fill_bounds.push((
                        vertical.x - vertical.width / 2.0,
                        horizontal.y - horizontal.height / 2.0,
                        vertical.x + vertical.width / 2.0,
                        horizontal.y + horizontal.height / 2.0,
                    ));
                }
                // Square a thin transition between an offset landing and a
                // route. Widen only the overlap band: expanding the complete
                // pair bounding box can approach unrelated conductors. The
                // full deficit is added on each side because foundry
                // minimum-width checks measure opposing concave corners.
                if overlap_x > EPSILON
                    && overlap_y > EPSILON
                    && overlap_x < rule.min_width_um - EPSILON
                    && overlap_y < rule.min_width_um - EPSILON
                    && matches!(
                        (left.purpose, right.purpose),
                        (
                            PhysicalShapePurpose::ViaLanding | PhysicalShapePurpose::DeviceLanding,
                            PhysicalShapePurpose::Route
                        ) | (
                            PhysicalShapePurpose::Route,
                            PhysicalShapePurpose::ViaLanding | PhysicalShapePurpose::DeviceLanding
                        )
                    )
                {
                    let (landing, route) = if matches!(
                        left.purpose,
                        PhysicalShapePurpose::ViaLanding | PhysicalShapePurpose::DeviceLanding
                    ) {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    if (left.y - right.y).abs() >= (left.x - right.x).abs() {
                        let extension = rule.min_width_um - overlap_y;
                        let route_min_y = route.y - route.height / 2.0;
                        let route_max_y = route.y + route.height / 2.0;
                        if route.y >= landing.y {
                            fill_bounds.push((
                                route.x - route.width / 2.0,
                                (route_min_y - extension).max(landing.y - landing.height / 2.0),
                                route.x + route.width / 2.0,
                                route_min_y,
                            ));
                        } else {
                            fill_bounds.push((
                                route.x - route.width / 2.0,
                                route_max_y,
                                route.x + route.width / 2.0,
                                (route_max_y + extension).min(landing.y + landing.height / 2.0),
                            ));
                        }
                    } else {
                        let extension = rule.min_width_um - overlap_x;
                        let route_min_x = route.x - route.width / 2.0;
                        let route_max_x = route.x + route.width / 2.0;
                        if route.x >= landing.x {
                            fill_bounds.push((
                                (route_min_x - extension).max(landing.x - landing.width / 2.0),
                                route.y - route.height / 2.0,
                                route_min_x,
                                route.y + route.height / 2.0,
                            ));
                        } else {
                            fill_bounds.push((
                                route_max_x,
                                route.y - route.height / 2.0,
                                (route_max_x + extension).min(landing.x + landing.width / 2.0),
                                route.y + route.height / 2.0,
                            ));
                        }
                    }
                }

                for (min_x, min_y, max_x, max_y) in fill_bounds {
                    if max_x - min_x <= EPSILON || max_y - min_y <= EPSILON {
                        continue;
                    }
                    let mut candidate = PhysicalShape {
                        layer: PhysicalLayer::Metal(layer),
                        x: (min_x + max_x) / 2.0,
                        y: (min_y + max_y) / 2.0,
                        width: max_x - min_x,
                        height: max_y - min_y,
                        component_id: None,
                        net: Some(net),
                        purpose: PhysicalShapePurpose::RouteFill,
                    };
                    snap_shape_to_grid(&mut candidate, grid);
                    if rectangle_covered_by_same_net(&candidate, shapes) {
                        continue;
                    }
                    let quantize = |value: f64| (value / grid).round() as i64;
                    let key = (
                        layer,
                        net,
                        quantize(candidate.x),
                        quantize(candidate.y),
                        quantize(candidate.width),
                        quantize(candidate.height),
                    );
                    if seen.insert(key)
                        && route_shape_conflicts(
                            shapes,
                            std::slice::from_ref(&candidate),
                            net,
                            rules,
                        ) == 0
                    {
                        shapes.push(candidate);
                        added += 1;
                    }
                }
            }
        }
    }
    added
}

fn rectangle_covered_by_same_net(candidate: &PhysicalShape, shapes: &[PhysicalShape]) -> bool {
    const EPSILON: f64 = 1e-9;
    let min_x = candidate.x - candidate.width / 2.0;
    let max_x = candidate.x + candidate.width / 2.0;
    let min_y = candidate.y - candidate.height / 2.0;
    let max_y = candidate.y + candidate.height / 2.0;
    let covering = shapes
        .iter()
        .filter(|shape| shape.layer == candidate.layer && shape.net == candidate.net)
        .filter_map(|shape| {
            let left = (shape.x - shape.width / 2.0).max(min_x);
            let right = (shape.x + shape.width / 2.0).min(max_x);
            let top = (shape.y - shape.height / 2.0).max(min_y);
            let bottom = (shape.y + shape.height / 2.0).min(max_y);
            (right > left + EPSILON && bottom > top + EPSILON).then_some((left, top, right, bottom))
        })
        .collect::<Vec<_>>();
    if covering.is_empty() {
        return false;
    }
    let mut xs = vec![min_x, max_x];
    let mut ys = vec![min_y, max_y];
    for (left, top, right, bottom) in &covering {
        xs.extend([*left, *right]);
        ys.extend([*top, *bottom]);
    }
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    xs.dedup_by(|left, right| (*left - *right).abs() <= EPSILON);
    ys.dedup_by(|left, right| (*left - *right).abs() <= EPSILON);
    xs.windows(2).all(|x| {
        ys.windows(2).all(|y| {
            let point = ((x[0] + x[1]) / 2.0, (y[0] + y[1]) / 2.0);
            covering.iter().any(|(left, top, right, bottom)| {
                point.0 >= *left - EPSILON
                    && point.0 <= *right + EPSILON
                    && point.1 >= *top - EPSILON
                    && point.1 <= *bottom + EPSILON
            })
        })
    })
}

fn routing_bbox_area(shapes: &[PhysicalShape]) -> f64 {
    let bounds = shapes
        .iter()
        .filter(|shape| {
            shape.purpose != PhysicalShapePurpose::DummyFill
                && matches!(shape.layer, PhysicalLayer::Metal(_) | PhysicalLayer::Via(_))
        })
        .fold(None::<(f64, f64, f64, f64)>, |bounds, shape| {
            let shape_bounds = (
                shape.x - shape.width / 2.0,
                shape.y - shape.height / 2.0,
                shape.x + shape.width / 2.0,
                shape.y + shape.height / 2.0,
            );
            Some(match bounds {
                None => shape_bounds,
                Some((left, top, right, bottom)) => (
                    left.min(shape_bounds.0),
                    top.min(shape_bounds.1),
                    right.max(shape_bounds.2),
                    bottom.max(shape_bounds.3),
                ),
            })
        });
    bounds
        .map(|(left, top, right, bottom)| (right - left) * (bottom - top))
        .unwrap_or(0.0)
}

fn route_endpoint_is_justified(
    shapes: &[PhysicalShape],
    shape_index: usize,
    endpoint_x: f64,
    endpoint_y: f64,
    grid: f64,
) -> bool {
    let shape = &shapes[shape_index];
    let Some(net) = shape.net else {
        return true;
    };
    // Generated rectangles are independently snapped by their two edges. A
    // one-grid cap mismatch is still continuous after manufacturing-grid
    // quantization and must not be confused with a real route shootout.
    let horizontal = shape.width > shape.height + grid;
    let cross_width = if horizontal {
        shape.height
    } else {
        shape.width
    };
    // Treat the conventional half-width end cap as part of the junction.
    // Minimum-area legalization can extend a short access rectangle slightly
    // beyond the landing it overlaps; that is a joined conductor, not a
    // shootout. Anything longer than the cap still requires another shape.
    let epsilon = (grid * 2.5).max(cross_width / 2.0);
    let probe_half_width = if horizontal {
        epsilon
    } else {
        shape.width / 2.0
    };
    let probe_half_height = if horizontal {
        shape.height / 2.0
    } else {
        epsilon
    };
    shapes.iter().enumerate().any(|(other_index, other)| {
        if other_index == shape_index || other.net != Some(net) {
            return false;
        }
        let electrically_compatible = match (shape.layer, other.layer) {
            (PhysicalLayer::Metal(left), PhysicalLayer::Metal(right)) => left == right,
            (PhysicalLayer::Metal(layer), PhysicalLayer::Via(lower)) => {
                layer == lower || layer == lower + 1
            }
            (PhysicalLayer::Metal(1), PhysicalLayer::Contact) => true,
            _ => false,
        };
        if !electrically_compatible {
            return false;
        }
        // An exact duplicate cannot justify either copy's endpoint. Counting
        // it as a continuation hid coincident rejected-route remnants.
        if shape.layer == other.layer
            && (shape.x - other.x).abs() <= epsilon
            && (shape.y - other.y).abs() <= epsilon
            && (shape.width - other.width).abs() <= epsilon
            && (shape.height - other.height).abs() <= epsilon
        {
            return false;
        }
        endpoint_x + probe_half_width >= other.x - other.width / 2.0
            && endpoint_x - probe_half_width <= other.x + other.width / 2.0
            && endpoint_y + probe_half_height >= other.y - other.height / 2.0
            && endpoint_y - probe_half_height <= other.y + other.height / 2.0
    })
}

fn unjustified_route_endpoints(shapes: &[PhysicalShape], grid: f64) -> Vec<PhysicalRouteEndpoint> {
    let mut endpoints = Vec::new();
    for (shape_index, shape) in shapes.iter().enumerate() {
        let (PhysicalLayer::Metal(layer), Some(net)) = (shape.layer, shape.net) else {
            continue;
        };
        if shape.purpose == PhysicalShapePurpose::DummyFill
            || matches!(
                shape.purpose,
                PhysicalShapePurpose::Pin
                    | PhysicalShapePurpose::PowerRail
                    | PhysicalShapePurpose::DeviceLanding
                    | PhysicalShapePurpose::Tap
                    | PhysicalShapePurpose::ViaLanding
                    | PhysicalShapePurpose::RouteFill
            )
        {
            continue;
        }
        let horizontal = shape.width > shape.height + grid;
        let vertical = shape.height > shape.width + grid;
        if !horizontal && !vertical {
            continue;
        }
        let candidates = if horizontal {
            [
                (shape.x - shape.width / 2.0, shape.y),
                (shape.x + shape.width / 2.0, shape.y),
            ]
        } else {
            [
                (shape.x, shape.y - shape.height / 2.0),
                (shape.x, shape.y + shape.height / 2.0),
            ]
        };
        for (x, y) in candidates {
            if !route_endpoint_is_justified(shapes, shape_index, x, y, grid) {
                endpoints.push(PhysicalRouteEndpoint {
                    shape_index,
                    net,
                    layer: format!("Metal({layer})"),
                    purpose: shape.purpose.as_str().to_string(),
                    x,
                    y,
                });
            }
        }
    }
    endpoints.sort_by(|left, right| {
        (left.net, left.shape_index)
            .cmp(&(right.net, right.shape_index))
            .then_with(|| left.x.total_cmp(&right.x))
            .then_with(|| left.y.total_cmp(&right.y))
    });
    endpoints
}

fn route_quality(shapes: &[PhysicalShape], grid: f64) -> PhysicalRouteQualityReport {
    let routing = shapes
        .iter()
        .enumerate()
        .filter_map(|(index, shape)| {
            (shape.purpose != PhysicalShapePurpose::DummyFill
                && matches!(shape.layer, PhysicalLayer::Metal(_) | PhysicalLayer::Via(_)))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let routed_shape_area_um2 = routing
        .iter()
        .map(|index| shapes[*index].width * shapes[*index].height)
        .sum();
    let device_bounds = shapes
        .iter()
        .filter(|shape| {
            matches!(
                shape.purpose,
                PhysicalShapePurpose::Active
                    | PhysicalShapePurpose::Gate
                    | PhysicalShapePurpose::GateAccess
                    | PhysicalShapePurpose::Contact
                    | PhysicalShapePurpose::DeviceLanding
            )
        })
        .fold(None::<(f64, f64, f64, f64)>, |bounds, shape| {
            let current = (
                shape.x - shape.width / 2.0,
                shape.y - shape.height / 2.0,
                shape.x + shape.width / 2.0,
                shape.y + shape.height / 2.0,
            );
            Some(bounds.map_or(current, |bounds| {
                (
                    bounds.0.min(current.0),
                    bounds.1.min(current.1),
                    bounds.2.max(current.2),
                    bounds.3.max(current.3),
                )
            }))
        });
    let max_device_envelope_excursion_um = device_bounds.map_or(0.0, |bounds| {
        routing
            .iter()
            .map(|index| {
                let shape = &shapes[*index];
                let left = shape.x - shape.width / 2.0;
                let top = shape.y - shape.height / 2.0;
                let right = shape.x + shape.width / 2.0;
                let bottom = shape.y + shape.height / 2.0;
                (bounds.0 - left)
                    .max(0.0)
                    .max((bounds.1 - top).max(0.0))
                    .max((right - bounds.2).max(0.0))
                    .max((bottom - bounds.3).max(0.0))
            })
            .fold(0.0, f64::max)
    });
    let terminal_bounds = shapes
        .iter()
        .filter(|shape| {
            matches!(
                shape.purpose,
                PhysicalShapePurpose::DeviceLanding
                    | PhysicalShapePurpose::Pin
                    | PhysicalShapePurpose::Tap
            )
        })
        .fold(None::<(f64, f64, f64, f64)>, |bounds, shape| {
            let current = (
                shape.x - shape.width / 2.0,
                shape.y - shape.height / 2.0,
                shape.x + shape.width / 2.0,
                shape.y + shape.height / 2.0,
            );
            Some(bounds.map_or(current, |bounds| {
                (
                    bounds.0.min(current.0),
                    bounds.1.min(current.1),
                    bounds.2.max(current.2),
                    bounds.3.max(current.3),
                )
            }))
        });
    let mut largest_terminal_excursions = terminal_bounds.map_or_else(Vec::new, |bounds| {
        routing
            .iter()
            .map(|index| {
                let shape = &shapes[*index];
                let left = shape.x - shape.width / 2.0;
                let top = shape.y - shape.height / 2.0;
                let right = shape.x + shape.width / 2.0;
                let bottom = shape.y + shape.height / 2.0;
                let excursion_um = (bounds.0 - left)
                    .max(0.0)
                    .max((bounds.1 - top).max(0.0))
                    .max((right - bounds.2).max(0.0))
                    .max((bottom - bounds.3).max(0.0));
                PhysicalRouteExcursion {
                    shape_index: *index,
                    net: shape.net,
                    layer: format!("{:?}", shape.layer),
                    purpose: shape.purpose.as_str().to_string(),
                    excursion_um,
                    bounds: PhysicalBounds {
                        min_x: left,
                        min_y: top,
                        max_x: right,
                        max_y: bottom,
                    },
                }
            })
            .filter(|excursion| excursion.excursion_um > grid / 2.0)
            .collect::<Vec<_>>()
    });
    largest_terminal_excursions.sort_by(|left, right| {
        right
            .excursion_um
            .total_cmp(&left.excursion_um)
            .then_with(|| left.shape_index.cmp(&right.shape_index))
    });
    largest_terminal_excursions.truncate(16);
    let max_terminal_envelope_excursion_um = largest_terminal_excursions
        .first()
        .map_or(0.0, |excursion| excursion.excursion_um);

    let mut by_net = BTreeMap::<usize, Vec<usize>>::new();
    for index in &routing {
        if let Some(net) = shapes[*index].net {
            by_net.entry(net).or_default().push(*index);
        }
    }
    let mut terminal_free_component_count = 0;
    let mut terminal_free_shape_count = 0;
    let mut terminal_free_components = Vec::new();
    for (net, indices) in &by_net {
        let mut unseen = indices.iter().copied().collect::<HashSet<_>>();
        while let Some(seed) = unseen.iter().copied().min() {
            unseen.remove(&seed);
            let mut stack = vec![seed];
            let mut component = vec![seed];
            while let Some(current) = stack.pop() {
                let neighbors = unseen
                    .iter()
                    .copied()
                    .filter(|other| {
                        routing_shapes_connect(&shapes[current], &shapes[*other], grid / 2.0)
                    })
                    .collect::<Vec<_>>();
                for neighbor in neighbors {
                    unseen.remove(&neighbor);
                    stack.push(neighbor);
                    component.push(neighbor);
                }
            }
            let reaches_terminal = component.iter().any(|index| {
                let route = &shapes[*index];
                matches!(
                    route.purpose,
                    PhysicalShapePurpose::DeviceLanding
                        | PhysicalShapePurpose::Pin
                        | PhysicalShapePurpose::PowerRail
                        | PhysicalShapePurpose::Tap
                ) || shapes.iter().any(|terminal| {
                    terminal.layer == PhysicalLayer::Contact
                        && terminal.purpose == PhysicalShapePurpose::Contact
                        && terminal.net == route.net
                        && matches!(route.layer, PhysicalLayer::Metal(1))
                        && shapes_touch(route, terminal, grid / 2.0)
                })
            });
            if !reaches_terminal {
                terminal_free_component_count += 1;
                terminal_free_shape_count += component.len();
                component.sort_unstable();
                let mut layers = component
                    .iter()
                    .map(|index| format!("{:?}", shapes[*index].layer))
                    .collect::<Vec<_>>();
                layers.sort();
                layers.dedup();
                let mut purposes = component
                    .iter()
                    .map(|index| shapes[*index].purpose.as_str().to_string())
                    .collect::<Vec<_>>();
                purposes.sort();
                purposes.dedup();
                let (min_x, min_y, max_x, max_y) = component.iter().fold(
                    (
                        f64::INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NEG_INFINITY,
                    ),
                    |bounds, index| {
                        let shape = &shapes[*index];
                        (
                            bounds.0.min(shape.x - shape.width / 2.0),
                            bounds.1.min(shape.y - shape.height / 2.0),
                            bounds.2.max(shape.x + shape.width / 2.0),
                            bounds.3.max(shape.y + shape.height / 2.0),
                        )
                    },
                );
                terminal_free_components.push(PhysicalRouteQualityComponent {
                    net: *net,
                    shape_indices: component,
                    layers,
                    purposes,
                    bounds: PhysicalBounds {
                        min_x,
                        min_y,
                        max_x,
                        max_y,
                    },
                });
            }
        }
    }
    terminal_free_components.sort_by_key(|component| {
        (
            component.net,
            component
                .shape_indices
                .first()
                .copied()
                .unwrap_or(usize::MAX),
        )
    });

    let mut seen_vias = HashSet::new();
    let mut exact_duplicate_via_count = 0;
    let mut unlanded_via_count = 0;
    for index in &routing {
        let shape = &shapes[*index];
        let PhysicalLayer::Via(lower) = shape.layer else {
            continue;
        };
        let key = (
            lower,
            shape.net,
            (shape.x / grid).round() as i64,
            (shape.y / grid).round() as i64,
            (shape.width / grid).round() as i64,
            (shape.height / grid).round() as i64,
        );
        if !seen_vias.insert(key) {
            exact_duplicate_via_count += 1;
        }
        let landed = [PhysicalLayer::Metal(lower), PhysicalLayer::Metal(lower + 1)]
            .into_iter()
            .all(|layer| {
                routing.iter().any(|metal_index| {
                    let metal = &shapes[*metal_index];
                    metal.layer == layer
                        && metal.net == shape.net
                        && shapes_touch(metal, shape, grid / 2.0)
                })
            });
        if !landed {
            unlanded_via_count += 1;
        }
    }

    let unjustified_route_endpoints = unjustified_route_endpoints(shapes, grid);
    PhysicalRouteQualityReport {
        routed_shape_area_um2,
        routing_bbox_area_um2: routing_bbox_area(shapes),
        max_device_envelope_excursion_um,
        max_terminal_envelope_excursion_um,
        largest_terminal_excursions,
        terminal_free_component_count,
        terminal_free_shape_count,
        terminal_free_components,
        exact_duplicate_via_count,
        unlanded_via_count,
        unjustified_route_endpoint_count: unjustified_route_endpoints.len(),
        unjustified_route_endpoints,
    }
}

fn density_fill_layer(name: &str, technology: &Technology) -> Option<PhysicalLayer> {
    match name {
        "active" => Some(PhysicalLayer::Ndiff),
        "poly" => Some(PhysicalLayer::Poly),
        // Keep process top-metal fill distinct from the highest routable
        // signal layer. GDS export maps this synthetic IR layer to the deck's
        // dedicated `top_metal` dummy purpose.
        "top_metal" => Some(PhysicalLayer::Metal(technology.max_metal_layers + 1)),
        _ => name
            .strip_prefix("metal")
            .and_then(|index| index.parse::<u16>().ok())
            .map(PhysicalLayer::Metal),
    }
}

fn process_density_fill_counts(
    shapes: &[PhysicalShape],
    technology: &Technology,
) -> BTreeMap<String, usize> {
    technology
        .physical_rules
        .density_fill
        .layers
        .keys()
        .filter_map(|material| {
            let layer = density_fill_layer(material, technology)?;
            let count = shapes
                .iter()
                .filter(|shape| {
                    shape.purpose == PhysicalShapePurpose::DummyFill && shape.layer == layer
                })
                .count();
            Some((material.clone(), count))
        })
        .collect()
}

fn density_fill_obstacle(
    material: &str,
    candidate: &PhysicalShape,
    shape: &PhysicalShape,
    rule: &DensityFillLayerRule,
) -> bool {
    if shape.purpose == PhysicalShapePurpose::DummyFill {
        return shape.layer == candidate.layer
            && rectangles_within(candidate, shape, rule.fill_spacing_um);
    }
    let relevant = match material {
        "active" => matches!(
            shape.layer,
            PhysicalLayer::Ndiff
                | PhysicalLayer::Pdiff
                | PhysicalLayer::Poly
                | PhysicalLayer::Nwell
                | PhysicalLayer::Pwell
        ),
        "poly" => matches!(
            shape.layer,
            PhysicalLayer::Ndiff
                | PhysicalLayer::Pdiff
                | PhysicalLayer::Poly
                | PhysicalLayer::Nwell
                | PhysicalLayer::Pwell
        ),
        "top_metal" => false,
        _ => shape.layer == candidate.layer,
    };
    relevant && rectangles_within(candidate, shape, rule.circuit_spacing_um)
}

/// Add deterministic process-owned density geometry only after electrical
/// routing, cleanup, and final floorplan expansion. These shapes deliberately
/// carry no net or component identity and therefore cannot change extraction.
fn add_process_density_fill(
    shapes: &mut Vec<PhysicalShape>,
    bounds: &PhysicalBounds,
    technology: &Technology,
) {
    let rules = &technology.physical_rules.density_fill;
    if rules.layers.is_empty() {
        return;
    }
    let grid = technology.physical_rules.manufacturing_grid_um;
    let area = (bounds.max_x - bounds.min_x) * (bounds.max_y - bounds.min_y);
    let mut ordered = rules.layers.keys().cloned().collect::<Vec<_>>();
    ordered.sort_by_key(|name| match name.as_str() {
        "active" => (0, 0),
        "poly" => (1, 0),
        "top_metal" => (3, 0),
        _ => (
            2,
            name.strip_prefix("metal")
                .and_then(|value| value.parse().ok())
                .unwrap_or(0),
        ),
    });
    for name in ordered {
        let rule = &rules.layers[&name];
        let Some(layer) = density_fill_layer(&name, technology) else {
            continue;
        };
        let target_area = area * rule.target_density;
        let mut emitted_area = 0.0;
        let mut candidates = Vec::new();
        if rule.support_layer.as_deref() == Some("active") {
            for support in shapes.iter().filter(|shape| {
                shape.purpose == PhysicalShapePurpose::DummyFill
                    && matches!(shape.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff)
                    && shape.width + 1e-9 >= rule.tile_width_um
                    && shape.height + 1e-9 >= rule.tile_height_um
            }) {
                candidates.push((
                    support.x,
                    support.y,
                    rule.tile_width_um,
                    rule.tile_height_um,
                ));
            }
        } else {
            let mut orientations = vec![(rule.tile_width_um, rule.tile_height_um)];
            if (rule.tile_width_um - rule.tile_height_um).abs() > 1e-9 {
                orientations.push((rule.tile_height_um, rule.tile_width_um));
            }
            for size in &rule.fallback_tile_sizes_um {
                let size = *size;
                orientations.push((size, size));
            }
            for (width, height) in orientations {
                let step_x = width + rule.fill_spacing_um;
                let step_y = height + rule.fill_spacing_um;
                // A single lattice phase leaves large unusable holes around
                // routed conductors. Shifted phases and both bar orientations
                // recover legal pockets without relaxing process clearance.
                for (phase_x, phase_y) in [
                    (0.0, 0.0),
                    (0.5, 0.0),
                    (0.0, 0.5),
                    (0.5, 0.5),
                    (0.25, 0.25),
                    (0.75, 0.25),
                    (0.25, 0.75),
                    (0.75, 0.75),
                ] {
                    let mut y = snap_to_grid(bounds.min_y + height / 2.0 + phase_y * step_y, grid);
                    while y + height / 2.0 <= bounds.max_y + 1e-9 {
                        let mut x =
                            snap_to_grid(bounds.min_x + width / 2.0 + phase_x * step_x, grid);
                        while x + width / 2.0 <= bounds.max_x + 1e-9 {
                            candidates.push((x, y, width, height));
                            x = snap_to_grid(x + step_x, grid);
                        }
                        y = snap_to_grid(y + step_y, grid);
                    }
                }
            }
        }
        for (x, y, width, height) in candidates {
            // A support relationship is a material contract, not merely a
            // density target. Every accepted support tile must receive its
            // covering material even after the supported layer reaches its
            // minimum density.
            if rule.support_layer.is_none() && emitted_area + 1e-9 >= target_area {
                break;
            }
            let candidate = PhysicalShape {
                layer,
                x,
                y,
                width,
                height,
                component_id: None,
                net: None,
                purpose: PhysicalShapePurpose::DummyFill,
            };
            if shapes
                .iter()
                .any(|shape| density_fill_obstacle(&name, &candidate, shape, rule))
            {
                continue;
            }
            emitted_area += candidate.width * candidate.height;
            shapes.push(candidate);
        }
    }
}

fn expand_floorplan_to_geometry(
    bounds: &mut PhysicalBounds,
    shapes: &[PhysicalShape],
    project: &Project,
) -> Result<(), String> {
    let Some((left, mut top, right, mut bottom)) =
        shapes
            .iter()
            .fold(None::<(f64, f64, f64, f64)>, |envelope, shape| {
                let shape_bounds = (
                    shape.x - shape.width / 2.0,
                    shape.y - shape.height / 2.0,
                    shape.x + shape.width / 2.0,
                    shape.y + shape.height / 2.0,
                );
                Some(match envelope {
                    None => shape_bounds,
                    Some((min_x, min_y, max_x, max_y)) => (
                        min_x.min(shape_bounds.0),
                        min_y.min(shape_bounds.1),
                        max_x.max(shape_bounds.2),
                        max_y.max(shape_bounds.3),
                    ),
                })
            })
    else {
        return Ok(());
    };
    let mut wells = shapes
        .iter()
        .filter(|shape| matches!(shape.layer, PhysicalLayer::Nwell | PhysicalLayer::Pwell))
        .collect::<Vec<_>>();
    wells.sort_by(|left, right| left.y.total_cmp(&right.y));
    if let Some(first) = wells.first() {
        let stripe_height = first.height;
        let stripe_origin = first.y - stripe_height / 2.0;
        top = stripe_origin + ((top - stripe_origin) / stripe_height).floor() * stripe_height;
        bottom = stripe_origin + ((bottom - stripe_origin) / stripe_height).ceil() * stripe_height;
    }
    let window = &project.technology.tapeout_window;
    let usable_half_width = window.width_um / 2.0 - window.edge_margin_um;
    let usable_half_height = window.height_um / 2.0 - window.edge_margin_um;
    if left < -usable_half_width
        || right > usable_half_width
        || top < -usable_half_height
        || bottom > usable_half_height
    {
        return Err(format!(
            "physical routing exceeds the usable {} tapeout area: geometry [{left:.3}, {top:.3}]–[{right:.3}, {bottom:.3}] µm, usable [{:.3}, {:.3}]–[{:.3}, {:.3}] µm",
            window.name,
            -usable_half_width,
            -usable_half_height,
            usable_half_width,
            usable_half_height
        ));
    }
    let grid = project.technology.physical_rules.manufacturing_grid_um;
    let floor = |value: f64| (value / grid).floor() * grid;
    let ceil = |value: f64| (value / grid).ceil() * grid;
    bounds.min_x = floor(bounds.min_x.min(left));
    bounds.min_y = floor(bounds.min_y.min(top));
    bounds.max_x = ceil(bounds.max_x.max(right));
    bounds.max_y = ceil(bounds.max_y.max(bottom));
    Ok(())
}

fn well_bank_vertical_bounds(shapes: &[PhysicalShape]) -> Option<(f64, f64)> {
    shapes
        .iter()
        .filter(|shape| matches!(shape.layer, PhysicalLayer::Nwell | PhysicalLayer::Pwell))
        .fold(None, |bounds, shape| {
            let top = shape.y - shape.height / 2.0;
            let bottom = shape.y + shape.height / 2.0;
            Some(match bounds {
                None => (top, bottom),
                Some((min_y, max_y)) => (min_y.min(top), max_y.max(bottom)),
            })
        })
}

fn feol_density_targets_met(
    shapes: &[PhysicalShape],
    bounds: &PhysicalBounds,
    technology: &Technology,
) -> bool {
    let mut feol_technology = technology.clone();
    feol_technology
        .physical_rules
        .density_fill
        .layers
        .retain(|name, _| matches!(name.as_str(), "active" | "poly"));
    if feol_technology
        .physical_rules
        .density_fill
        .layers
        .is_empty()
    {
        return true;
    }
    let mut trial = shapes.to_vec();
    add_process_density_fill(&mut trial, bounds, &feol_technology);
    let area = (bounds.max_x - bounds.min_x) * (bounds.max_y - bounds.min_y);
    feol_technology
        .physical_rules
        .density_fill
        .layers
        .iter()
        .all(|(name, rule)| {
            let fill_area = trial
                .iter()
                .filter(|shape| {
                    shape.purpose == PhysicalShapePurpose::DummyFill
                        && density_fill_layer(name, &feol_technology)
                            .is_some_and(|layer| shape.layer == layer)
                })
                .map(|shape| shape.width * shape.height)
                .sum::<f64>();
            fill_area + 1e-9 >= area * rule.target_density
        })
}

/// Reserve field bands outside topology-owned device wells before final fill.
/// GF180 dummy COMP/poly must remain clear of Nwell/LVPwell boundaries, so a
/// full-outline well fabric can never satisfy FEOL density legally.
fn reserve_feol_density_field(
    bounds: &mut PhysicalBounds,
    shapes: &[PhysicalShape],
    project: &Project,
) -> Result<(), String> {
    let fill = &project.technology.physical_rules.density_fill.layers;
    if !fill.contains_key("active") && !fill.contains_key("poly") {
        return Ok(());
    }
    let grid = project.technology.physical_rules.manufacturing_grid_um;
    let original = bounds.clone();
    if feol_density_targets_met(shapes, bounds, &project.technology) {
        return Ok(());
    }
    let window = &project.technology.tapeout_window;
    let usable_half_height = window.height_um / 2.0 - window.edge_margin_um;
    let base_height = (original.max_y - original.min_y).max(grid);
    let snap_up = |value: f64| (value / (2.0 * grid)).ceil() * 2.0 * grid;
    let mut lower_extra = 0.0;
    let mut upper_extra = snap_up(base_height);
    let mut accepted = None;
    for _ in 0..12 {
        let candidate = PhysicalBounds {
            min_x: original.min_x,
            min_y: original.min_y - upper_extra / 2.0,
            max_x: original.max_x,
            max_y: original.max_y + upper_extra / 2.0,
        };
        if candidate.min_y < -usable_half_height || candidate.max_y > usable_half_height {
            break;
        }
        if feol_density_targets_met(shapes, &candidate, &project.technology) {
            accepted = Some(candidate);
            break;
        }
        lower_extra = upper_extra;
        upper_extra = snap_up(upper_extra * 2.0);
    }
    let Some(mut best) = accepted else {
        return Err(format!(
            "process FEOL density targets cannot fit inside the usable {} tapeout height",
            window.name
        ));
    };
    for _ in 0..12 {
        if upper_extra - lower_extra <= 2.0 * grid {
            break;
        }
        let middle = snap_up((lower_extra + upper_extra) / 2.0);
        if middle <= lower_extra + grid || middle >= upper_extra - grid {
            break;
        }
        let candidate = PhysicalBounds {
            min_x: original.min_x,
            min_y: original.min_y - middle / 2.0,
            max_x: original.max_x,
            max_y: original.max_y + middle / 2.0,
        };
        if feol_density_targets_met(shapes, &candidate, &project.technology) {
            upper_extra = middle;
            best = candidate;
        } else {
            lower_extra = middle;
        }
    }
    *bounds = best;
    Ok(())
}

fn expand_well_fabric_to_floorplan(
    shapes: &mut Vec<PhysicalShape>,
    bounds: &PhysicalBounds,
    bank_vertical_bounds: Option<(f64, f64)>,
    rules: &PhysicalRuleDeck,
) {
    let grid = rules.manufacturing_grid_um;
    let center_x = (bounds.min_x + bounds.max_x) / 2.0;
    let center_y = (bounds.min_y + bounds.max_y) / 2.0;
    let width = bounds.max_x - bounds.min_x;
    let height = bounds.max_y - bounds.min_y;
    for substrate in shapes
        .iter_mut()
        .filter(|shape| shape.layer == PhysicalLayer::Substrate)
    {
        substrate.x = center_x;
        substrate.y = center_y;
        substrate.width = width;
        substrate.height = height;
        snap_shape_to_grid(substrate, grid);
    }

    let mut wells = shapes
        .iter()
        .filter(|shape| matches!(shape.layer, PhysicalLayer::Nwell | PhysicalLayer::Pwell))
        .cloned()
        .collect::<Vec<_>>();
    if wells.is_empty() {
        return;
    }
    wells.sort_by(|left, right| left.y.total_cmp(&right.y));
    let stripe_height = wells[0].height;
    let original_top = wells[0].y - stripe_height / 2.0;
    let first_layer = wells[0].layer;
    shapes.retain(|shape| !matches!(shape.layer, PhysicalLayer::Nwell | PhysicalLayer::Pwell));

    let (bank_top, bank_bottom) = bank_vertical_bounds.unwrap_or((bounds.min_y, bounds.max_y));
    let first_index = ((bank_top - original_top) / stripe_height).floor() as i64;
    let last_index = ((bank_bottom - original_top) / stripe_height).ceil() as i64 - 1;
    for index in first_index..=last_index {
        let stripe_top = (original_top + index as f64 * stripe_height).max(bank_top);
        let stripe_bottom = (original_top + (index + 1) as f64 * stripe_height).min(bank_bottom);
        if stripe_bottom - stripe_top < grid {
            continue;
        }
        let even = index.rem_euclid(2) == 0;
        let layer = if even {
            first_layer
        } else if first_layer == PhysicalLayer::Nwell {
            PhysicalLayer::Pwell
        } else {
            PhysicalLayer::Nwell
        };
        let mut stripe = PhysicalShape {
            layer,
            x: center_x,
            y: (stripe_top + stripe_bottom) / 2.0,
            width,
            height: stripe_bottom - stripe_top,
            component_id: None,
            net: None,
            purpose: PhysicalShapePurpose::Fabric,
        };
        snap_shape_to_grid(&mut stripe, grid);
        shapes.push(stripe);
    }
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
                cell_name: String::new(),
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

fn normalize_with_progress(
    project: &Project,
    progress: &mut dyn FnMut(&'static str, u8),
) -> Result<PhysicalLayoutIr, String> {
    progress("topology", 2);
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
                standard_cell_group: component
                    .name
                    .rsplit_once('·')
                    .map(|(instance_path, _)| instance_path.to_string()),
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
    progress("planning", 8);
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
    let candidate_count = planning.candidates.len().max(1);
    for candidate_index in 0..planning.candidates.len() {
        progress(
            "candidateRouting",
            10 + ((candidate_index * 52) / candidate_count) as u8,
        );
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
        if !candidate_placement.candidates[candidate_placement.selected_candidate].legal {
            // Planning feasibility covers area/routing demand; final device
            // access footprints can still make a particular aspect ratio
            // unplaceable. Do not send a partial placement into routing.
            continue;
        }
        let top_level_net_ids = top_level_nets
            .iter()
            .map(|net| net.id)
            .collect::<HashSet<_>>();
        let mut routing_plan = candidate_plan.clone();
        routing_plan
            .nets
            .retain(|net| top_level_net_ids.contains(&net.net));
        let floorplan = &candidate_plan.candidates[candidate_index];
        // Placement estimates cannot fully predict whether compact,
        // topology-shaped device neighborhoods are easier to route. Evaluate
        // the normal score winner and the geometry-topology challenger
        // through the real global/detail routers, then retain the electrically
        // and geometrically stronger result.
        let mut placement_indices = vec![candidate_placement.selected_candidate];
        if let Some(index) = candidate_placement.candidates.iter().position(|candidate| {
            candidate.strategy == physical_placement::PlacementStrategy::LeafTopology
                && candidate.legal
        }) {
            if !placement_indices.contains(&index) {
                placement_indices.push(index);
            }
        }
        let challenger = if candidate_placement.candidates.iter().any(|candidate| {
            candidate.strategy == physical_placement::PlacementStrategy::LeafGeometryTopology
                && candidate.legal
        }) {
            physical_placement::PlacementStrategy::LeafGeometryTopology
        } else {
            physical_placement::PlacementStrategy::GeometryTopology
        };
        if let Some(index) = candidate_placement
            .candidates
            .iter()
            .position(|candidate| candidate.strategy == challenger && candidate.legal)
        {
            if !placement_indices.contains(&index) {
                placement_indices.push(index);
            }
        }
        let mut evaluation: Option<CandidateEvaluation> = None;
        for placement_index in placement_indices {
            let mut routed_placement = candidate_placement.clone();
            routed_placement.selected_candidate = placement_index;
            let candidate_global = physical_global_routing::route(
                &top_level_nets,
                &pins,
                &routed_placement,
                &routing_plan,
            );
            let candidate_detailed = physical_detailed_routing::route(
                &devices,
                &nets,
                &pins,
                &candidate_global,
                &candidate_plan,
                &project.technology,
                &routed_placement,
            );
            let candidate_timing =
                physical_timing(&nets, &devices, &pins, &candidate_detailed, project);
            let summary = PhysicalCandidateTiming {
                candidate: candidate_summaries.len(),
                floorplan_candidate: candidate_index,
                placement_candidate: placement_index,
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
            let candidate = CandidateEvaluation {
                candidate: candidate_index,
                placement: routed_placement,
                global: candidate_global,
                detailed: candidate_detailed,
                timing: candidate_timing,
                summary,
            };
            if std::env::var_os("OPENCHIPPY_ROUTER_TRACE").is_some() {
                let placed = &candidate.placement.candidates[placement_index];
                eprintln!(
                    "routed placement: floorplan={} placement={} strategy={:?} sharing={} straps={} overflow={} conflicts={} wire={:.3} vias={}",
                    candidate_index,
                    placement_index,
                    placed.strategy,
                    placed.diffusion_sharing_pairs,
                    placed.gate_strap_pairs,
                    candidate.global.total_overflow,
                    candidate.detailed.conflict_count,
                    candidate.detailed.total_wire_length_um,
                    candidate.detailed.total_via_count,
                );
            }
            candidate_summaries.push(candidate.summary.clone());
            if evaluation
                .as_ref()
                .is_none_or(|current| candidate_is_better(&candidate, current))
            {
                evaluation = Some(candidate);
            }
        }
        let mut evaluation =
            evaluation.expect("at least the selected legal placement candidate is routed");
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
    let mut placement = selected.placement;
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
    // Flattened device geometry is the production default. A repeated logical
    // block is not automatically a physically characterized macro: preserving
    // it can lock in a poor template and deny the placer useful routing space.
    // The staged standard-cell candidate remains in the report for explicitly
    // characterized memories/register arrays and future measured cutover.
    let mut preview_variants = vec![(placement.clone(), true)];
    let selected_is_compacted = placement
        .candidates
        .get(placement.selected_candidate)
        .is_some_and(|candidate| candidate.topology_compacted);
    if selected_is_compacted {
        let fallback = placement
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.legal && !candidate.topology_compacted)
            .min_by(|(_, left), (_, right)| {
                left.score
                    .total_cmp(&right.score)
                    .then_with(|| left.id.cmp(&right.id))
            })
            .map(|(index, _)| index);
        if let Some(fallback) = fallback {
            let mut conventional = placement.clone();
            conventional.selected_candidate = fallback;
            preview_variants.push((conventional, true));
        }
    }
    let mut preview_jobs = Vec::new();
    for ordered_nets in route_orders {
        for (placement_variant, flexible_gate_access) in preview_variants.iter().cloned() {
            // Each logical terminal remains an independent routing
            // obligation. Same-net row-access coalescing is deliberately not
            // a candidate mode: only topology-proven diffusion/poly sharing
            // may replace multiple terminal anchors.
            preview_jobs.push((
                ordered_nets.clone(),
                placement_variant,
                flexible_gate_access,
            ));
        }
    }
    let distributed_power_nets = nets
        .iter()
        .filter(|net| matches!(net.role, NetRole::Power | NetRole::Ground))
        .map(|net| net.id)
        .collect::<Vec<_>>();
    let require_upper_power_distribution = project.technology.max_metal_layers >= 2;
    progress("geometryRefinement", 64);
    // A complete refinement attempt owns its canvas and route geometry. Run
    // those attempts concurrently, then score their immutable results in the
    // original deterministic job order. Individual nets remain serialized
    // inside an attempt because they negotiate one shared occupancy map.
    let mut preview_candidates = preview_jobs
        .into_par_iter()
        .map(|(ordered_nets, placement_variant, flexible_gate_access)| {
            let (bounds, mut shapes) = compact_preview(
                &devices,
                &ordered_nets,
                &pins,
                project.technology.max_metal_layers,
                &project.technology.physical_rules,
                &planning,
                &placement_variant,
                flexible_gate_access,
            );
            for shape in &mut shapes {
                snap_shape_to_grid(shape, grid);
            }
            deduplicate_exact_vias(&mut shapes, grid);
            let local_refinements = shapes
                .iter()
                .filter(|shape| is_local_route_shape(shape))
                .cloned()
                .collect::<Vec<_>>();
            shapes.retain(|shape| !is_local_route_shape(shape));
            let (mut shapes, rejected_conflicts) =
                commit_preview_routing(shapes, &project.technology.physical_rules);
            let mut orphan_routing_shapes_removed =
                prune_orphan_routing(&mut shapes, &project.technology.physical_rules);
            let repaired_open_nets = repair_disconnected_routing(
                &mut shapes,
                &nets,
                &project.technology.physical_rules,
                false,
            );
            admit_local_route_refinements(
                &mut shapes,
                local_refinements,
                &project.technology.physical_rules,
            );
            orphan_routing_shapes_removed +=
                prune_orphan_routing(&mut shapes, &project.technology.physical_rules);
            let connectivity_errors = physical_drc::terminal_connectivity_error_count(&shapes);
            let power_distribution_errors = if require_upper_power_distribution {
                distributed_power_nets
                    .iter()
                    .filter(|net| {
                        !shapes.iter().any(|shape| {
                            shape.net == Some(**net) && shape.layer == PhysicalLayer::Metal(2)
                        })
                    })
                    .count()
            } else {
                0
            };
            let route_area = routing_bbox_area(&shapes);
            (
                connectivity_errors,
                power_distribution_errors,
                rejected_conflicts.saturating_sub(repaired_open_nets),
                route_area,
                shapes.len(),
                orphan_routing_shapes_removed,
                bounds,
                shapes,
                placement_variant,
            )
        })
        .collect::<Vec<_>>();
    if std::env::var_os("OPENCHIPPY_ROUTER_TRACE").is_some() {
        for (index, candidate) in preview_candidates.iter().enumerate() {
            let placement_candidate = &candidate.8.candidates[candidate.8.selected_candidate];
            eprintln!(
                "refined candidate: index={index} placement={} strategy={:?} compact={} connectivity={} power_distribution={} conflicts={} bbox={:.3} shapes={} pruned={}",
                candidate.8.selected_candidate,
                placement_candidate.strategy,
                placement_candidate.topology_compacted,
                candidate.0,
                candidate.1,
                candidate.2,
                candidate.3,
                candidate.4,
                candidate.5,
            );
        }
    }
    // Cheap candidate screening intentionally avoids the multilayer graph
    // search. Do not let that approximation permanently disqualify compact
    // topology: fully close the strongest compact and conventional finalist,
    // then compare their achieved connectivity and geometry. This also keeps
    // the expensive search bounded to at most two candidates.
    let preview_order = |left: &(
        usize,
        usize,
        usize,
        f64,
        usize,
        usize,
        PhysicalBounds,
        Vec<PhysicalShape>,
        PhysicalPlacementReport,
    ),
                         right: &(
        usize,
        usize,
        usize,
        f64,
        usize,
        usize,
        PhysicalBounds,
        Vec<PhysicalShape>,
        PhysicalPlacementReport,
    )| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.total_cmp(&right.3))
            .then_with(|| left.4.cmp(&right.4))
    };
    let mut finalists = [None, None];
    for (index, candidate) in preview_candidates.iter().enumerate() {
        let compact = candidate.8.candidates[candidate.8.selected_candidate].topology_compacted;
        let slot = usize::from(compact);
        if finalists[slot]
            .is_none_or(|current| preview_order(candidate, &preview_candidates[current]).is_lt())
        {
            finalists[slot] = Some(index);
        }
    }
    for index in finalists.into_iter().flatten() {
        let candidate = &mut preview_candidates[index];
        repair_disconnected_routing(
            &mut candidate.7,
            &nets,
            &project.technology.physical_rules,
            true,
        );
        trim_metal_overhangs(&mut candidate.7, &project.technology.physical_rules);
        candidate.5 += prune_orphan_routing(&mut candidate.7, &project.technology.physical_rules);
        candidate.0 = physical_drc::terminal_connectivity_error_count(&candidate.7);
        candidate.3 = routing_bbox_area(&candidate.7);
        candidate.4 = candidate.7.len();
        if std::env::var_os("OPENCHIPPY_ROUTER_TRACE").is_some() {
            let placement_candidate = &candidate.8.candidates[candidate.8.selected_candidate];
            eprintln!(
                "legalized finalist: index={index} compact={} connectivity={} rejected_conflicts={} bbox={:.3} shapes={} pruned={}",
                placement_candidate.topology_compacted,
                candidate.0,
                candidate.2,
                candidate.3,
                candidate.4,
                candidate.5,
            );
        }
    }
    progress("geometryRefinement", 88);
    let (
        _,
        _,
        _,
        _,
        _,
        mut orphan_routing_shapes_removed,
        mut bounds,
        mut shapes,
        selected_placement,
    ) = preview_candidates
        .into_iter()
        .min_by(preview_order)
        .expect("at least one physical preview candidate");
    // Candidate screening is intentionally cheap and parallel. Spend the
    // multilayer graph search only once, on the selected geometry, so workers
    // do not run three memory-heavy routing lattices against one another.
    repair_disconnected_routing(&mut shapes, &nets, &project.technology.physical_rules, true);
    trim_metal_overhangs(&mut shapes, &project.technology.physical_rules);
    orphan_routing_shapes_removed +=
        prune_orphan_routing(&mut shapes, &project.technology.physical_rules);
    progress("connectivityCleanup", 90);
    placement = selected_placement;
    let well_bank_bounds = well_bank_vertical_bounds(&shapes);
    expand_floorplan_to_geometry(&mut bounds, &shapes, project)?;
    bounds.min_x = snap_to_grid(bounds.min_x, grid);
    bounds.min_y = snap_to_grid(bounds.min_y, grid);
    bounds.max_x = snap_to_grid(bounds.max_x, grid);
    bounds.max_y = snap_to_grid(bounds.max_y, grid);
    reserve_feol_density_field(&mut bounds, &shapes, project)?;
    expand_well_fabric_to_floorplan(
        &mut shapes,
        &bounds,
        well_bank_bounds,
        &project.technology.physical_rules,
    );
    candidate_summaries.sort_by_key(|candidate| candidate.candidate);
    let mut timing = physical_timing(&nets, &devices, &pins, &detailed_routing, project);
    timing.candidates = candidate_summaries;
    let selected_timing_candidate = timing
        .candidates
        .iter()
        .find(|candidate| {
            candidate.floorplan_candidate == selected_candidate
                && candidate.placement_candidate == placement.selected_candidate
        })
        .map(|candidate| candidate.candidate)
        .unwrap_or_else(|| {
            // Geometry refinement may select the conventional fallback even
            // when the cheap planning stage did not route that placement as
            // one of its bounded finalists. Publish the actually selected
            // placement/routing result rather than leaving timing provenance
            // pointing at a different physical candidate.
            let candidate = timing.candidates.len();
            let floorplan = &planning.candidates[selected_candidate];
            timing.candidates.push(PhysicalCandidateTiming {
                candidate,
                floorplan_candidate: selected_candidate,
                placement_candidate: placement.selected_candidate,
                strategy: floorplan.strategy,
                timing_driven: project.timing_target_ns.is_some(),
                area_um2: floorplan.width_um * floorplan.height_um,
                total_wire_length_um: detailed_routing.total_wire_length_um,
                total_via_count: detailed_routing.total_via_count,
                routing_overflow: global_routing.total_overflow,
                detail_conflicts: detailed_routing.conflict_count,
                estimated_worst_delay_ns: timing.estimated_worst_delay_ns,
                slack_ns: timing.worst_slack_ns,
                meets_timing: timing.worst_slack_ns.map(|slack| slack >= 0.0),
            });
            candidate
        });
    timing.selected_candidate = Some(selected_timing_candidate);
    let selected_devices = placement
        .candidates
        .iter()
        .find(|candidate| candidate.id == placement.selected_candidate)
        .map(|candidate| candidate.devices.as_slice())
        .unwrap_or(&[]);
    let placed_by_id = selected_devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let geometry_devices = orient_devices_for_diffusion_sharing(&devices, &placed_by_id, grid);
    let mut topology_scratch = shapes.clone();
    let mut row_topology = synthesize_row_topology(
        &mut topology_scratch,
        &geometry_devices,
        &placed_by_id,
        &project.technology.physical_rules,
    );
    materialize_shared_terminal_accesses(
        &mut shapes,
        &mut row_topology,
        &nets,
        &pins,
        &placed_by_id,
        &project.technology.physical_rules,
    );
    fill_same_net_metal_notches(&mut shapes, &project.technology.physical_rules);
    add_process_density_fill(&mut shapes, &bounds, &project.technology);
    let density_fill_counts = process_density_fill_counts(&shapes, &project.technology);
    let missing_density_layers = density_fill_counts
        .iter()
        .filter_map(|(material, count)| (*count == 0).then_some(material.as_str()))
        .collect::<Vec<_>>();
    if !missing_density_layers.is_empty() {
        return Err(format!(
            "{} defines process density fill, but physical generation emitted no dummy geometry for: {}",
            project.technology.name,
            missing_density_layers.join(", ")
        ));
    }
    let tapeout = physical_tapeout(&shapes, &bounds, project);
    let route_quality = route_quality(&shapes, grid);

    let mut ir = PhysicalLayoutIr {
        format_version: CURRENT_PHYSICAL_IR_VERSION,
        source_project_name: project.name.clone(),
        technology_name: project.technology.name.clone(),
        technology_fingerprint: project.technology.fingerprint()?,
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
        row_topology,
        route_quality,
        orphan_routing_shapes_removed,
        physical_blocks: Vec::new(),
        standard_cell_library: standard_cells::generate_for_process(&project.technology.process_id)
            .unwrap_or_else(|_| standard_cells::empty_summary(&project.technology.process_id)),
    };
    progress("physicalIr", 94);
    let block_drc = physical_drc::validate(&ir, &project.technology);
    ir.physical_blocks = physical_blocks(&ir, &block_drc);
    Ok(ir)
}

#[cfg(test)]
pub fn normalize(project: &Project) -> Result<PhysicalLayoutIr, String> {
    normalize_with_progress(project, &mut |_, _| {})
}

pub fn normalize_project(project: &Project) -> Result<PhysicalLayoutIr, String> {
    normalize_project_with_progress(project, |_, _| {})
}

pub fn normalize_project_with_progress(
    project: &Project,
    mut progress: impl FnMut(&'static str, u8),
) -> Result<PhysicalLayoutIr, String> {
    progress("topology", 1);
    let mut effective_project = project.clone();
    effective_project.technology.migrate_legacy_gds_layers();
    let implementation = if effective_project
        .components
        .iter()
        .any(|component| component.block_definition_id.is_some())
    {
        effective_project.flattened()?
    } else {
        effective_project
    };
    let mut ir = normalize_with_progress(&implementation, &mut progress)?;
    let definition_names = project
        .components
        .iter()
        .filter_map(|component| {
            let definition = component
                .block_definition_id
                .and_then(|id| project.block_definition(id))?;
            Some((component.name.as_str(), definition.name.as_str()))
        })
        .collect::<HashMap<_, _>>();
    for block in &mut ir.physical_blocks {
        block.cell_name = definition_names
            .get(block.instance_name.as_str())
            .copied()
            .unwrap_or(block.instance_name.as_str())
            .to_string();
    }
    Ok(ir)
}

#[cfg(test)]
mod tests {
    use super::{
        add_process_density_fill, commit_preview_routing, contains_shape, density_fill_layer,
        device_footprint_with_gate_access, gate_contact_point, local_route_candidates,
        local_route_marker, materialize_shared_terminal_accesses, multilayer_track_search,
        normalize, normalize_with_progress, process_density_fill_counts, prune_orphan_routing,
        route_quality, shared_signal_row_accesses, synthesize_row_topology, trim_metal_overhangs,
        DeviceKind, NetRole, PhysicalBounds, PhysicalDevice, PhysicalLayer, PhysicalNet,
        PhysicalShape, PhysicalShapePurpose, PhysicalTerminal, RouteAnchor, ROUTING_CLEARANCE,
        ROUTING_LANDING_SIZE,
    };
    use crate::model::{Project, TerminalRef};
    use crate::physical_canvas::{
        device_active_width, device_terminal_offset, ObstructionType, PhysicalCanvas,
    };
    use crate::physical_placement::PlacedDevice;
    use crate::{
        physical_drc,
        technology::{PhysicalRuleDeck, Technology},
    };
    use std::collections::{HashMap, HashSet};
    use uuid::Uuid;

    #[test]
    fn process_density_fill_is_deterministic_and_electrically_inert() {
        let technology = Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .unwrap();
        let bounds = PhysicalBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 30.0,
            max_y: 30.0,
        };
        let electrical = PhysicalShape {
            layer: PhysicalLayer::Metal(1),
            x: 15.0,
            y: 15.0,
            width: 0.23,
            height: 8.0,
            component_id: Some(Uuid::new_v4()),
            net: Some(7),
            purpose: PhysicalShapePurpose::Route,
        };
        let mut first = vec![electrical.clone()];
        let mut second = vec![electrical];
        add_process_density_fill(&mut first, &bounds, &technology);
        add_process_density_fill(&mut second, &bounds, &technology);
        assert_eq!(first, second);
        let dummy = first
            .iter()
            .filter(|shape| shape.purpose == PhysicalShapePurpose::DummyFill)
            .collect::<Vec<_>>();
        assert!(!dummy.is_empty());
        assert!(dummy
            .iter()
            .all(|shape| shape.net.is_none() && shape.component_id.is_none()));
        assert!(dummy
            .iter()
            .any(|shape| shape.layer == PhysicalLayer::Metal(6)));
        assert_eq!(
            route_quality(&first, technology.physical_rules.manufacturing_grid_um)
                .routed_shape_area_um2,
            0.23 * 8.0
        );
    }

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

    fn nested_inverter_pair() -> Project {
        let mut pair = hierarchical_inverter_pair();
        let definition = pair.capture_block("PAIR".into()).unwrap();
        let mut top = Project::default();
        top.block_definitions = pair.block_definitions;
        top.place_block(definition, 0.0, 0.0).unwrap();
        top
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
    fn well_fabric_alternates_and_topology_seeded_devices_match_polarity() {
        let ir = normalize(&inverter()).unwrap();
        let wells = ir
            .shapes
            .iter()
            .filter(|shape| matches!(shape.layer, PhysicalLayer::Pwell | PhysicalLayer::Nwell))
            .collect::<Vec<_>>();
        assert!(wells.len() >= 2);
        assert_eq!(wells[0].layer, PhysicalLayer::Nwell);
        assert_eq!(wells[1].layer, PhysicalLayer::Pwell);
        assert!(wells.windows(2).all(|pair| {
            (pair[0].height - pair[1].height).abs() < 1e-9
                && (pair[0].y + pair[0].height / 2.0 - (pair[1].y - pair[1].height / 2.0)).abs()
                    < 1e-9
        }));
        assert!(
            (wells.first().unwrap().y - wells.first().unwrap().height / 2.0 - ir.bounds.min_y)
                .abs()
                < 1e-9
        );
        assert!(
            (wells.last().unwrap().y + wells.last().unwrap().height / 2.0 - ir.bounds.max_y).abs()
                < 1e-9
        );
        let selected = &ir.placement.candidates[ir.placement.selected_candidate];
        assert!(selected.devices.iter().all(|device| device.x <= 0.0));
        for device in &selected.devices {
            let required = if device.kind == DeviceKind::Pmos {
                PhysicalLayer::Nwell
            } else {
                PhysicalLayer::Pwell
            };
            assert!(wells.iter().any(|well| {
                well.layer == required
                    && device.y >= well.y - well.height / 2.0
                    && device.y <= well.y + well.height / 2.0
            }));
        }
    }

    #[test]
    fn floorplan_expands_to_routes_and_tapeout_escape_fails_generation() {
        let project = inverter();
        let normal = normalize(&project).unwrap();
        assert!(normal.tapeout.fits);
        assert_eq!(normal.tapeout.shapes_outside_floorplan, 0);
        let wells = normal
            .shapes
            .iter()
            .filter(|shape| matches!(shape.layer, PhysicalLayer::Nwell | PhysicalLayer::Pwell))
            .collect::<Vec<_>>();
        assert!(!wells.is_empty());
        assert!(wells.iter().all(|well| {
            (well.x - well.width / 2.0 - normal.bounds.min_x).abs() < 1e-6
                && (well.x + well.width / 2.0 - normal.bounds.max_x).abs() < 1e-6
        }));
        let well_top = wells
            .iter()
            .map(|well| well.y - well.height / 2.0)
            .fold(f64::INFINITY, f64::min);
        let well_bottom = wells
            .iter()
            .map(|well| well.y + well.height / 2.0)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((well_top - normal.bounds.min_y).abs() < 1e-6);
        assert!((well_bottom - normal.bounds.max_y).abs() < 1e-6);

        let mut tiny = project;
        tiny.technology.tapeout_window.width_um = 1.0;
        tiny.technology.tapeout_window.height_um = 1.0;
        let error = normalize(&tiny).unwrap_err();
        assert!(error.contains("exceeds the usable"));
        assert!(error.contains("tapeout area"));
    }

    #[test]
    fn compatible_facing_terminals_receive_a_shared_diffusion_region() {
        let rules = PhysicalRuleDeck::default();
        let left_id = Uuid::new_v4();
        let right_id = Uuid::new_v4();
        let devices = vec![
            PhysicalDevice {
                component_id: left_id,
                name: "M1".into(),
                physical_group: None,
                standard_cell_group: Some("CELL".into()),
                kind: DeviceKind::Nmos,
                gate_net: 1,
                drain_net: 2,
                source_net: 7,
                width_um: 1.0,
                length_um: 1.0,
            },
            PhysicalDevice {
                component_id: right_id,
                name: "M2".into(),
                physical_group: None,
                standard_cell_group: Some("CELL".into()),
                kind: DeviceKind::Nmos,
                gate_net: 1,
                drain_net: 7,
                source_net: 4,
                width_um: 1.0,
                length_um: 1.0,
            },
        ];
        let pitch = device_active_width(&devices[0], &rules) + rules.diffusion.min_spacing_um;
        let placed = vec![
            PlacedDevice {
                component_id: left_id,
                name: "M1".into(),
                kind: DeviceKind::Nmos,
                x: 0.0,
                y: 0.0,
                row: 0,
                column: 0,
                site: 0,
            },
            PlacedDevice {
                component_id: right_id,
                name: "M2".into(),
                kind: DeviceKind::Nmos,
                x: pitch,
                y: 0.0,
                row: 0,
                column: 1,
                site: 1,
            },
        ];
        let placed_by_id = placed
            .iter()
            .map(|device| (device.component_id, device))
            .collect::<HashMap<_, _>>();
        let mut shapes = devices
            .iter()
            .zip(&placed)
            .flat_map(|(device, placed)| {
                device_footprint_with_gate_access(device, placed.x, placed.y, &rules, false)
                    .into_iter()
                    .map(|(shape, _)| shape)
            })
            .collect::<Vec<_>>();
        let mut topology = synthesize_row_topology(&mut shapes, &devices, &placed_by_id, &rules);
        assert_eq!(topology.len(), 1);
        assert_eq!(topology[0].ordered_devices, vec![left_id, right_id]);
        assert_eq!(topology[0].islands.len(), 1);
        assert_eq!(topology[0].islands[0].device_ids, vec![left_id, right_id]);
        assert_eq!(topology[0].islands[0].terminal_nets, vec![2, 7, 4]);
        assert_eq!(topology[0].gate_straps.len(), 1);
        assert_eq!(topology[0].gate_straps[0].net, 1);
        assert_eq!(
            topology[0].gate_straps[0].device_ids,
            vec![left_id, right_id]
        );
        assert!(
            !topology[0].gate_straps[0].geometry.is_empty(),
            "shared gate membership must retain its exact physical conductor"
        );
        let shared_bounds = shapes
            .iter()
            .find(|shape| {
                shape.layer == PhysicalLayer::Ndiff
                    && shape.purpose == PhysicalShapePurpose::Active
                    && shape.net.is_none()
                    && shape.component_id.is_none()
            })
            .map(|shape| (shape.x - shape.width / 2.0, shape.x + shape.width / 2.0))
            .unwrap();
        assert!(shared_bounds.0 <= 1e-9);

        let internal_net = PhysicalNet {
            id: 7,
            name: "internal_series_node".into(),
            role: NetRole::Internal,
            terminals: vec![
                PhysicalTerminal {
                    component_id: left_id,
                    component_name: "M1".into(),
                    terminal: "source".into(),
                },
                PhysicalTerminal {
                    component_id: right_id,
                    component_name: "M2".into(),
                    terminal: "drain".into(),
                },
            ],
        };
        materialize_shared_terminal_accesses(
            &mut shapes,
            &mut topology,
            &[internal_net],
            &[],
            &placed_by_id,
            &rules,
        );
        assert!(shapes.iter().all(|shape| {
            shape.net != Some(7)
                || !matches!(
                    shape.layer,
                    PhysicalLayer::Contact | PhysicalLayer::Metal(_)
                )
        }));
        assert!(shared_bounds.1 >= pitch - 1e-9);
        assert!(shapes.iter().any(|shape| {
            shape.layer == PhysicalLayer::Poly
                && shape.net == Some(1)
                && shape.component_id.is_none()
                && shape.purpose == PhysicalShapePurpose::GateAccess
        }));

        // Electrical net equality alone must not dissolve the boundary
        // between two separately instantiated cells. Their terminals remain
        // distinct anchors and are joined later by routed metal.
        let mut separate_cells = devices.clone();
        separate_cells[1].standard_cell_group = Some("CELL_B".into());
        let mut separate_shapes = separate_cells
            .iter()
            .zip(&placed)
            .flat_map(|(device, placed)| {
                device_footprint_with_gate_access(device, placed.x, placed.y, &rules, false)
                    .into_iter()
                    .map(|(shape, _)| shape)
            })
            .collect::<Vec<_>>();
        let separate_topology =
            synthesize_row_topology(&mut separate_shapes, &separate_cells, &placed_by_id, &rules);
        assert_eq!(separate_topology[0].islands.len(), 2);
        assert!(separate_topology[0].gate_straps.is_empty());
        assert!(!separate_shapes.iter().any(|shape| {
            shape.component_id.is_none()
                && (shape.purpose == PhysicalShapePurpose::Active
                    || shape.purpose == PhysicalShapePurpose::GateAccess)
        }));
    }

    #[test]
    fn cross_row_gate_topology_physically_reaches_every_group_member() {
        let rules = PhysicalRuleDeck::default();
        let ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        let devices = vec![
            PhysicalDevice {
                component_id: ids[0],
                name: "P1".into(),
                physical_group: None,
                standard_cell_group: Some("LOGIC".into()),
                kind: DeviceKind::Pmos,
                gate_net: 9,
                drain_net: 10,
                source_net: 11,
                width_um: 1.0,
                length_um: 0.5,
            },
            PhysicalDevice {
                component_id: ids[1],
                name: "N1".into(),
                physical_group: None,
                standard_cell_group: Some("LOGIC".into()),
                kind: DeviceKind::Nmos,
                gate_net: 9,
                drain_net: 12,
                source_net: 13,
                width_um: 1.0,
                length_um: 0.5,
            },
            PhysicalDevice {
                component_id: ids[2],
                name: "N2".into(),
                physical_group: None,
                standard_cell_group: Some("LOGIC".into()),
                kind: DeviceKind::Nmos,
                gate_net: 9,
                drain_net: 14,
                source_net: 15,
                width_um: 1.0,
                length_um: 0.5,
            },
        ];
        let placed = vec![
            PlacedDevice {
                component_id: ids[0],
                name: "P1".into(),
                kind: DeviceKind::Pmos,
                x: -2.0,
                y: -3.0,
                row: 0,
                column: 0,
                site: -400,
            },
            PlacedDevice {
                component_id: ids[1],
                name: "N1".into(),
                kind: DeviceKind::Nmos,
                x: -2.0,
                y: 3.0,
                row: 1,
                column: 0,
                site: -400,
            },
            PlacedDevice {
                component_id: ids[2],
                name: "N2".into(),
                kind: DeviceKind::Nmos,
                x: 2.0,
                y: 3.0,
                row: 1,
                column: 1,
                site: 400,
            },
        ];
        let placed_by_id = placed
            .iter()
            .map(|device| (device.component_id, device))
            .collect::<HashMap<_, _>>();
        let mut shapes = devices
            .iter()
            .zip(&placed)
            .flat_map(|(device, placed)| {
                device_footprint_with_gate_access(device, placed.x, placed.y, &rules, false)
                    .into_iter()
                    .map(|(shape, _)| shape)
            })
            .collect::<Vec<_>>();
        let topology = synthesize_row_topology(&mut shapes, &devices, &placed_by_id, &rules);
        let strap = topology
            .iter()
            .flat_map(|row| &row.gate_straps)
            .find(|strap| {
                strap.net == 9
                    && strap.device_ids.iter().copied().collect::<HashSet<_>>()
                        == ids.into_iter().collect::<HashSet<_>>()
            })
            .expect("cross-row shared gate strap");
        assert!(!strap.geometry.is_empty());
        let shared_poly = shapes
            .iter()
            .filter(|shape| {
                shape.layer == PhysicalLayer::Poly
                    && shape.net == Some(9)
                    && shape.component_id.is_none()
                    && shape.purpose == PhysicalShapePurpose::GateAccess
            })
            .collect::<Vec<_>>();
        assert!(!shared_poly.is_empty());
        assert!(strap
            .geometry
            .iter()
            .all(|bounds| shared_poly.iter().any(|shape| {
                (bounds.min_x - (shape.x - shape.width / 2.0)).abs() <= 1e-9
                    && (bounds.max_x - (shape.x + shape.width / 2.0)).abs() <= 1e-9
                    && (bounds.min_y - (shape.y - shape.height / 2.0)).abs() <= 1e-9
                    && (bounds.max_y - (shape.y + shape.height / 2.0)).abs() <= 1e-9
            })));
        for device in &devices {
            let contact = gate_contact_point(&shapes, device).expect("device gate contact");
            assert!(
                shared_poly.iter().any(|shape| {
                    contact.0 >= shape.x - shape.width / 2.0 - rules.manufacturing_grid_um / 2.0
                        && contact.0
                            <= shape.x + shape.width / 2.0 + rules.manufacturing_grid_um / 2.0
                        && contact.1
                            >= shape.y - shape.height / 2.0 - rules.manufacturing_grid_um / 2.0
                        && contact.1
                            <= shape.y + shape.height / 2.0 + rules.manufacturing_grid_um / 2.0
                }),
                "{} was recorded in the shared gate topology without physical poly",
                device.name
            );
        }
        let (committed, _) = commit_preview_routing(shapes.clone(), &rules);
        for bounds in &strap.geometry {
            assert!(
                committed.iter().any(|shape| {
                    shape.layer == PhysicalLayer::Poly
                        && shape.purpose == PhysicalShapePurpose::GateAccess
                        && shape.component_id.is_none()
                        && shape.net == Some(strap.net)
                        && (bounds.min_x - (shape.x - shape.width / 2.0)).abs() <= 1e-9
                        && (bounds.max_x - (shape.x + shape.width / 2.0)).abs() <= 1e-9
                        && (bounds.min_y - (shape.y - shape.height / 2.0)).abs() <= 1e-9
                        && (bounds.max_y - (shape.y + shape.height / 2.0)).abs() <= 1e-9
                }),
                "final preview admission dropped a topology-owned gate strap rectangle"
            );
        }
    }

    #[test]
    fn mixed_width_shared_diffusion_preserves_each_gate_channel_height() {
        let rules = PhysicalRuleDeck::default();
        let left_id = Uuid::new_v4();
        let right_id = Uuid::new_v4();
        let devices = vec![
            PhysicalDevice {
                component_id: left_id,
                name: "NARROW".into(),
                physical_group: None,
                standard_cell_group: Some("CELL".into()),
                kind: DeviceKind::Nmos,
                gate_net: 1,
                drain_net: 2,
                source_net: 7,
                width_um: 0.7,
                length_um: 0.5,
            },
            PhysicalDevice {
                component_id: right_id,
                name: "WIDE".into(),
                physical_group: None,
                standard_cell_group: Some("CELL".into()),
                kind: DeviceKind::Nmos,
                gate_net: 3,
                drain_net: 7,
                source_net: 4,
                width_um: 2.4,
                length_um: 0.5,
            },
        ];
        let pitch = device_active_width(&devices[0], &rules) + rules.diffusion.min_spacing_um;
        let placed = vec![
            PlacedDevice {
                component_id: left_id,
                name: "NARROW".into(),
                kind: DeviceKind::Nmos,
                x: 0.0,
                y: 0.0,
                row: 0,
                column: 0,
                site: 0,
            },
            PlacedDevice {
                component_id: right_id,
                name: "WIDE".into(),
                kind: DeviceKind::Nmos,
                x: pitch,
                y: 0.0,
                row: 0,
                column: 1,
                site: 1,
            },
        ];
        let placed_by_id = placed
            .iter()
            .map(|device| (device.component_id, device))
            .collect::<HashMap<_, _>>();
        let mut shapes = devices
            .iter()
            .zip(&placed)
            .flat_map(|(device, placed)| {
                device_footprint_with_gate_access(device, placed.x, placed.y, &rules, false)
                    .into_iter()
                    .map(|(shape, _)| shape)
            })
            .collect::<Vec<_>>();

        let topology = synthesize_row_topology(&mut shapes, &devices, &placed_by_id, &rules);
        let island = &topology[0].islands[0];
        assert_eq!(island.device_ids, vec![left_id, right_id]);
        assert!(
            island.geometry.len() >= 2,
            "mixed device widths require a rectilinear active island"
        );

        for (device, placed) in devices.iter().zip(&placed) {
            let gate_height = island
                .geometry
                .iter()
                .filter(|segment| {
                    placed.x >= segment.min_x - rules.manufacturing_grid_um / 2.0
                        && placed.x <= segment.max_x + rules.manufacturing_grid_um / 2.0
                })
                .map(|segment| segment.max_y - segment.min_y)
                .fold(0.0, f64::max);
            let expected = crate::physical_canvas::device_diffusion_height(device, &rules);
            assert!(
                (gate_height - expected).abs() <= rules.manufacturing_grid_um,
                "{} channel height changed from {expected:.4} to {gate_height:.4}",
                device.name
            );
        }

        let shared_shapes = shapes
            .iter()
            .filter(|shape| {
                shape.layer == PhysicalLayer::Ndiff
                    && shape.purpose == PhysicalShapePurpose::Active
                    && shape.component_id.is_none()
            })
            .count();
        assert_eq!(shared_shapes, island.geometry.len());
    }

    #[test]
    fn coincident_same_net_terminals_in_separate_cells_remain_distinct() {
        let rules = PhysicalRuleDeck::default();
        let left_id = Uuid::new_v4();
        let right_id = Uuid::new_v4();
        let devices = vec![
            PhysicalDevice {
                component_id: left_id,
                name: "M1".into(),
                physical_group: None,
                standard_cell_group: Some("CELL_A".into()),
                kind: DeviceKind::Nmos,
                gate_net: 1,
                drain_net: 2,
                source_net: 7,
                width_um: 1.0,
                length_um: 1.0,
            },
            PhysicalDevice {
                component_id: right_id,
                name: "M2".into(),
                physical_group: None,
                standard_cell_group: Some("CELL_B".into()),
                kind: DeviceKind::Nmos,
                gate_net: 3,
                drain_net: 7,
                source_net: 4,
                width_um: 1.0,
                length_um: 1.0,
            },
        ];
        let pitch = device_terminal_offset(&devices[0], &rules)
            + device_terminal_offset(&devices[1], &rules);
        let placed = vec![
            PlacedDevice {
                component_id: left_id,
                name: "M1".into(),
                kind: DeviceKind::Nmos,
                x: -pitch / 2.0,
                y: 0.0,
                row: 0,
                column: 0,
                site: 0,
            },
            PlacedDevice {
                component_id: right_id,
                name: "M2".into(),
                kind: DeviceKind::Nmos,
                x: pitch / 2.0,
                y: 0.0,
                row: 0,
                column: 1,
                site: 1,
            },
        ];
        let placed_by_id = placed
            .iter()
            .map(|device| (device.component_id, device))
            .collect::<HashMap<_, _>>();
        let mut shapes = devices
            .iter()
            .zip(&placed)
            .flat_map(|(device, placed)| {
                device_footprint_with_gate_access(device, placed.x, placed.y, &rules, false)
                    .into_iter()
                    .map(|(shape, _)| shape)
            })
            .collect::<Vec<_>>();
        let shared_x = placed[0].x + device_terminal_offset(&devices[0], &rules);
        let mut retained_shared_contact = false;
        shapes.retain(|shape| {
            let is_shared = shape.layer == PhysicalLayer::Contact
                && shape.net == Some(7)
                && (shape.x - shared_x).abs() <= rules.manufacturing_grid_um / 2.0;
            if is_shared && retained_shared_contact {
                false
            } else {
                retained_shared_contact |= is_shared;
                true
            }
        });
        let topology = synthesize_row_topology(&mut shapes, &devices, &placed_by_id, &rules);
        assert_eq!(topology[0].islands.len(), 2);
        assert!(topology[0]
            .islands
            .iter()
            .all(|island| island.device_ids.len() == 1));
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
        assert!(ir.timing.candidates.len() >= ir.planning.candidates.len());
        assert!(ir.planning.candidates.iter().enumerate().all(|(index, _)| {
            ir.timing
                .candidates
                .iter()
                .any(|candidate| candidate.floorplan_candidate == index)
        }));
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
        assert_eq!(first.timing.candidates.len(), 6);
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
        assert_eq!(ir.placement.candidates.len(), 5);
        assert!(ir.placement.candidates.iter().any(|candidate| {
            candidate.strategy == crate::physical_placement::PlacementStrategy::GateTopology
        }));
        assert!(ir.placement.candidates.iter().any(|candidate| {
            candidate.strategy == crate::physical_placement::PlacementStrategy::GeometryTopology
        }));
        assert!(ir
            .placement
            .candidates
            .iter()
            .all(|candidate| candidate.legal));
        assert!(ir.timing.candidates.iter().any(|routed| {
            ir.placement.candidates[routed.placement_candidate].strategy
                == crate::physical_placement::PlacementStrategy::GeometryTopology
        }));
        let selected = &ir.placement.candidates[ir.placement.selected_candidate];
        let mut occupied = HashSet::new();
        let grid = Technology::default().physical_rules.manufacturing_grid_um;
        for placed in &selected.devices {
            let alignment = if selected.topology_compacted {
                grid
            } else {
                ir.planning.placement_site_width_um
            };
            assert!((placed.x / alignment - (placed.x / alignment).round()).abs() < 1e-9);
            assert!(occupied.insert((placed.kind, placed.row, placed.site)));
            let island = ir
                .row_topology
                .iter()
                .flat_map(|row| &row.islands)
                .find(|island| island.device_ids.contains(&placed.component_id))
                .expect("every placed transistor belongs to one active island");
            assert!(placed.x >= island.bounds.min_x - grid / 2.0);
            assert!(placed.x <= island.bounds.max_x + grid / 2.0);
            assert!(placed.y >= island.bounds.min_y - grid / 2.0);
            assert!(placed.y <= island.bounds.max_y + grid / 2.0);
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

    #[test]
    fn local_cell_router_offers_compact_m1_and_m2_geometry() {
        let rules = &Technology::default().physical_rules;
        let first = RouteAnchor {
            point: (-0.8, 0.0),
            layer: PhysicalLayer::Metal(1),
            access_bounds: Some((-1.2, 1.2)),
            component_id: None,
        };
        let second = RouteAnchor {
            point: (0.8, 0.6),
            layer: PhysicalLayer::Metal(1),
            access_bounds: Some((-1.2, 1.2)),
            component_id: None,
        };
        let candidates = local_route_candidates(&first, &second, 7, rules, 2);
        assert!(candidates.iter().any(|candidate| {
            candidate.iter().all(|shape| {
                !matches!(shape.layer, PhysicalLayer::Metal(2) | PhysicalLayer::Via(_))
            })
        }));
        let m2_candidate = candidates
            .iter()
            .find(|candidate| {
                candidate
                    .iter()
                    .any(|shape| shape.layer == PhysicalLayer::Metal(2))
            })
            .expect("an M2-assisted local route");
        assert_eq!(
            m2_candidate
                .iter()
                .filter(|shape| shape.layer == PhysicalLayer::Via(1))
                .count(),
            2
        );
        assert!(candidates
            .iter()
            .flatten()
            .all(|shape| shape.net == Some(7)));
    }

    #[test]
    fn same_row_signal_terminals_share_one_legal_upper_metal_escape() {
        let rules = &Technology::default().physical_rules;
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let points = vec![
            RouteAnchor {
                point: (-1.0, 0.5),
                layer: PhysicalLayer::Metal(1),
                access_bounds: Some((-1.5, 1.5)),
                component_id: Some(first_id),
            },
            RouteAnchor {
                point: (1.0, 0.5),
                layer: PhysicalLayer::Metal(1),
                access_bounds: Some((-1.5, 1.5)),
                component_id: Some(second_id),
            },
        ];
        let standard_cells = HashMap::from([(first_id, "CELL"), (second_id, "CELL")]);
        let (anchors, geometry) =
            shared_signal_row_accesses(&points, 7, 5, &[], rules, &standard_cells);

        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].layer, PhysicalLayer::Metal(3));
        assert_eq!(anchors[0].access_bounds, Some((-1.5, 1.5)));
        assert!(geometry.iter().any(|shape| {
            shape.layer == PhysicalLayer::Metal(3)
                && shape.purpose == PhysicalShapePurpose::Route
                && shape.width > 1.9
        }));
        assert_eq!(
            geometry
                .iter()
                .filter(|shape| shape.layer == PhysicalLayer::Via(1))
                .count(),
            2
        );
        for via in geometry
            .iter()
            .filter(|shape| matches!(shape.layer, PhysicalLayer::Via(_)))
        {
            let PhysicalLayer::Via(lower) = via.layer else {
                unreachable!()
            };
            let cut = rules
                .via_overrides
                .get(&format!("via{lower}{}", lower + 1))
                .unwrap_or(&rules.via);
            for layer in [PhysicalLayer::Metal(lower), PhysicalLayer::Metal(lower + 1)] {
                assert!(
                    geometry.iter().any(|landing| {
                        landing.layer == layer
                            && landing.net == via.net
                            && contains_shape(landing, via, cut.enclosure_um)
                    }),
                    "{:?} lacks a process-legal {:?} enclosure after grid snapping",
                    via.layer,
                    layer
                );
            }
        }
        let mut canvas = PhysicalCanvas::new(rules);
        assert!(canvas
            .commit_routing_geometry(&geometry, "shared-row-test")
            .is_ok());
    }

    #[test]
    fn same_row_signal_terminals_do_not_share_across_standard_cells() {
        let rules = &Technology::default().physical_rules;
        let first_id = Uuid::new_v4();
        let second_id = Uuid::new_v4();
        let points = vec![
            RouteAnchor {
                point: (-2.0, 0.5),
                layer: PhysicalLayer::Metal(1),
                access_bounds: Some((-2.5, -1.5)),
                component_id: Some(first_id),
            },
            RouteAnchor {
                point: (2.0, 0.5),
                layer: PhysicalLayer::Metal(1),
                access_bounds: Some((1.5, 2.5)),
                component_id: Some(second_id),
            },
        ];
        let standard_cells = HashMap::from([(first_id, "CELL_A"), (second_id, "CELL_B")]);

        let (anchors, geometry) =
            shared_signal_row_accesses(&points, 7, 5, &[], rules, &standard_cells);

        assert_eq!(anchors, points);
        assert!(geometry.is_empty());
    }

    #[test]
    fn final_cleanup_removes_only_routing_islands_without_device_or_pin_access() {
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Contact,
                x: 0.0,
                y: 0.0,
                width: 0.22,
                height: 0.22,
                component_id: Some(Uuid::new_v4()),
                net: Some(1),
                purpose: PhysicalShapePurpose::Contact,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 0.30,
                height: 0.30,
                component_id: None,
                net: Some(1),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(3),
                x: 4.0,
                y: 4.0,
                width: 0.30,
                height: 0.30,
                component_id: None,
                net: Some(1),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Via(2),
                x: 4.0,
                y: 4.0,
                width: 0.24,
                height: 0.24,
                component_id: None,
                net: Some(1),
                purpose: PhysicalShapePurpose::Via,
            },
        ];
        let mut rules = PhysicalRuleDeck::default();
        rules.manufacturing_grid_um = 0.01;
        assert_eq!(prune_orphan_routing(&mut shapes, &rules), 2);
        assert_eq!(shapes.len(), 2);
        assert!(shapes
            .iter()
            .any(|shape| shape.layer == PhysicalLayer::Metal(1)));
    }

    #[test]
    fn final_cleanup_strips_a_dead_branch_from_a_terminal_to_terminal_route() {
        let component = Uuid::new_v4();
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Contact,
                x: 0.0,
                y: 0.0,
                width: 0.22,
                height: 0.22,
                component_id: Some(component),
                net: Some(7),
                purpose: PhysicalShapePurpose::Contact,
            },
            PhysicalShape {
                layer: PhysicalLayer::Contact,
                x: 4.0,
                y: 0.0,
                width: 0.22,
                height: 0.22,
                component_id: Some(component),
                net: Some(7),
                purpose: PhysicalShapePurpose::Contact,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 0.30,
                height: 0.30,
                component_id: None,
                net: Some(7),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 4.0,
                y: 0.0,
                width: 0.30,
                height: 0.30,
                component_id: None,
                net: Some(7),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 2.0,
                y: 0.0,
                width: 4.30,
                height: 0.30,
                component_id: None,
                net: Some(7),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 2.0,
                y: 0.90,
                width: 0.30,
                height: 1.50,
                component_id: None,
                net: Some(7),
                purpose: PhysicalShapePurpose::Route,
            },
        ];
        let mut rules = PhysicalRuleDeck::default();
        rules.manufacturing_grid_um = 0.01;
        // The branch plus both pieces of trunk outside the two contact
        // junctions are removable after junction fragmentation.
        assert_eq!(prune_orphan_routing(&mut shapes, &rules), 3);
        // The retained trunk is now represented by two junction-bounded
        // fragments rather than one indivisible rectangle.
        assert_eq!(shapes.len(), 6);
        assert!(!shapes
            .iter()
            .any(|shape| shape.layer == PhysicalLayer::Metal(1) && shape.y > 0.0));
    }

    #[test]
    fn final_cleanup_can_remove_only_the_unused_side_of_a_long_trunk() {
        let left = Uuid::new_v4();
        let right = Uuid::new_v4();
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Contact,
                x: 0.0,
                y: 0.0,
                width: 0.22,
                height: 0.22,
                component_id: Some(left),
                net: Some(9),
                purpose: PhysicalShapePurpose::Contact,
            },
            PhysicalShape {
                layer: PhysicalLayer::Contact,
                x: 4.0,
                y: 0.0,
                width: 0.22,
                height: 0.22,
                component_id: Some(right),
                net: Some(9),
                purpose: PhysicalShapePurpose::Contact,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 0.30,
                height: 0.30,
                component_id: Some(left),
                net: Some(9),
                purpose: PhysicalShapePurpose::DeviceLanding,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 4.0,
                y: 0.0,
                width: 0.30,
                height: 0.30,
                component_id: Some(right),
                net: Some(9),
                purpose: PhysicalShapePurpose::DeviceLanding,
            },
            // Only [0, 4] joins the terminals.  The [-4, 0] shootout used to
            // survive because this complete rectangle was one graph node.
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 8.0,
                height: 0.30,
                component_id: None,
                net: Some(9),
                purpose: PhysicalShapePurpose::Route,
            },
        ];
        let mut rules = PhysicalRuleDeck::default();
        rules.manufacturing_grid_um = 0.01;

        assert_eq!(prune_orphan_routing(&mut shapes, &rules), 1);
        let route = shapes
            .iter()
            .find(|shape| shape.purpose == PhysicalShapePurpose::Route)
            .expect("terminal-to-terminal trunk remains");
        assert!(route.x - route.width / 2.0 >= -0.01);
        assert!(route.x + route.width / 2.0 >= 3.99);
    }

    #[test]
    fn final_cleanup_trims_metal_past_its_outermost_connections() {
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Metal(3),
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 0.20,
                component_id: None,
                net: Some(4),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(3),
                x: -1.0,
                y: 0.0,
                width: 0.20,
                height: 2.0,
                component_id: None,
                net: Some(4),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(3),
                x: 1.0,
                y: 0.0,
                width: 0.20,
                height: 2.0,
                component_id: None,
                net: Some(4),
                purpose: PhysicalShapePurpose::Route,
            },
        ];
        let mut rules = PhysicalRuleDeck::default();
        rules.manufacturing_grid_um = 0.01;
        assert_eq!(trim_metal_overhangs(&mut shapes, &rules), 1);
        assert!(shapes[0].width < 2.5);
        assert!((shapes[0].x).abs() < 1e-6);
    }

    #[test]
    fn route_quality_reports_a_branch_that_ends_in_empty_space() {
        let shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 2.0,
                height: 0.20,
                component_id: None,
                net: Some(4),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Contact,
                x: -1.0,
                y: 0.0,
                width: 0.20,
                height: 0.20,
                component_id: Some(Uuid::new_v4()),
                net: Some(4),
                purpose: PhysicalShapePurpose::Contact,
            },
        ];
        let report = route_quality(&shapes, 0.01);
        assert_eq!(report.unjustified_route_endpoint_count, 1);
        assert!((report.unjustified_route_endpoints[0].x - 1.0).abs() < 1e-9);
    }

    #[test]
    fn final_cleanup_trims_internal_m1_access_but_preserves_device_landing() {
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 0.20,
                component_id: Some(local_route_marker(4)),
                net: Some(4),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: -1.0,
                y: 0.0,
                width: 0.20,
                height: 2.0,
                component_id: Some(local_route_marker(4)),
                net: Some(4),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 1.0,
                y: 0.0,
                width: 0.20,
                height: 2.0,
                component_id: Some(local_route_marker(4)),
                net: Some(4),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 4.0,
                y: 4.0,
                width: 0.30,
                height: 0.30,
                component_id: Some(Uuid::new_v4()),
                net: Some(9),
                purpose: PhysicalShapePurpose::DeviceLanding,
            },
        ];
        let mut rules = PhysicalRuleDeck::default();
        rules.manufacturing_grid_um = 0.01;
        assert_eq!(trim_metal_overhangs(&mut shapes, &rules), 1);
        assert!(shapes[0].width < 2.5);
        assert_eq!(shapes[3].width, 0.30);
        assert_eq!(shapes[3].height, 0.30);
    }

    #[test]
    fn route_quality_measures_only_geometry_outside_the_terminal_envelope() {
        let shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 0.30,
                height: 0.30,
                component_id: Some(Uuid::new_v4()),
                net: Some(1),
                purpose: PhysicalShapePurpose::DeviceLanding,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 0.20,
                height: 0.20,
                component_id: None,
                net: Some(1),
                purpose: PhysicalShapePurpose::Route,
            },
        ];
        let inside = route_quality(&shapes, 0.01);
        assert_eq!(inside.max_terminal_envelope_excursion_um, 0.0);
        assert!(inside.largest_terminal_excursions.is_empty());

        let mut above = shapes;
        above.push(PhysicalShape {
            layer: PhysicalLayer::Metal(1),
            x: 0.0,
            y: -5.0,
            width: 0.20,
            height: 0.20,
            component_id: None,
            net: Some(1),
            purpose: PhysicalShapePurpose::Route,
        });
        let outside = route_quality(&above, 0.01);
        assert!((outside.max_terminal_envelope_excursion_um - 4.95).abs() < 1e-9);
        assert_eq!(outside.largest_terminal_excursions[0].shape_index, 2);
    }

    #[test]
    fn generated_nand_passes_all_example_process_decks() {
        for (name, source) in [
            (
                "OpenChippy EDU",
                include_str!("../../docs/examples/openchippy-edu-5m.yaml"),
            ),
            (
                "GF180 compatibility",
                include_str!("../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"),
            ),
        ] {
            let technology = Technology::from_yaml(source)
                .unwrap_or_else(|error| panic!("{name} technology deck did not load: {error}"));
            let mut project = nand();
            project.technology = technology.clone();
            let ir = normalize(&project)
                .unwrap_or_else(|error| panic!("{name} physical generation failed: {error}"));
            let report = physical_drc::validate(&ir, &technology);
            assert_eq!(
                ir.route_quality.terminal_free_component_count, 0,
                "{name} retained terminal-free routing components"
            );
            assert_eq!(
                ir.route_quality.exact_duplicate_via_count, 0,
                "{name} retained exact duplicate vias"
            );
            assert_eq!(
                ir.route_quality.unlanded_via_count, 0,
                "{name} retained vias without both adjacent metal landings"
            );
            let unenclosed_contacts = report
                .diagnostics
                .iter()
                .filter(|diagnostic| {
                    diagnostic.rule_id == "CUT.ENCLOSURE" && diagnostic.layer == "contact"
                })
                .flat_map(|diagnostic| diagnostic.shape_indices.iter())
                .filter_map(|index| ir.shapes.get(*index))
                .map(|contact| {
                    (
                        contact.clone(),
                        ir.shapes
                            .iter()
                            .filter(|shape| {
                                shape.layer == PhysicalLayer::Metal(1) && shape.net == contact.net
                            })
                            .cloned()
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>();
            let unrouted_terminals = report
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.rule_id == "CONNECTIVITY.UNROUTED_TERMINAL")
                .flat_map(|diagnostic| diagnostic.shape_indices.iter())
                .filter_map(|index| ir.shapes.get(*index))
                .map(|terminal| {
                    (
                        terminal.clone(),
                        ir.shapes
                            .iter()
                            .filter(|shape| {
                                shape.layer == PhysicalLayer::Metal(1) && shape.net == terminal.net
                            })
                            .cloned()
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(
                report.error_count, 0,
                "{name} produced physical DRC diagnostics: {:?}; unenclosed contacts: {:?}; unrouted terminals: {:?}",
                report.diagnostics, unenclosed_contacts, unrouted_terminals,
            );
        }
    }

    #[test]
    fn gf180_reserves_legal_field_bands_for_feol_density_fill() {
        let technology = Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .expect("GF180 compatibility deck should load");
        let mut project = nand();
        project.technology = technology.clone();
        let ir = normalize(&project).expect("GF180 NAND should generate");
        let wells = ir
            .shapes
            .iter()
            .filter(|shape| matches!(shape.layer, PhysicalLayer::Nwell | PhysicalLayer::Pwell))
            .collect::<Vec<_>>();
        let well_top = wells
            .iter()
            .map(|shape| shape.y - shape.height / 2.0)
            .fold(f64::INFINITY, f64::min);
        let well_bottom = wells
            .iter()
            .map(|shape| shape.y + shape.height / 2.0)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            well_top > ir.bounds.min_y || well_bottom < ir.bounds.max_y,
            "FEOL density planning should leave field outside topology-owned wells"
        );
        let area = (ir.bounds.max_x - ir.bounds.min_x) * (ir.bounds.max_y - ir.bounds.min_y);
        for material in ["active", "poly"] {
            let rule = &technology.physical_rules.density_fill.layers[material];
            let layer = density_fill_layer(material, &technology).unwrap();
            let fill_area = ir
                .shapes
                .iter()
                .filter(|shape| {
                    shape.purpose == PhysicalShapePurpose::DummyFill && shape.layer == layer
                })
                .map(|shape| shape.width * shape.height)
                .sum::<f64>();
            assert!(
                fill_area + 1e-9 >= area * rule.target_density,
                "{material} fill area {fill_area} did not reach target {} over {area}",
                rule.target_density
            );
        }
        let counts = process_density_fill_counts(&ir.shapes, &technology);
        assert!(
            counts.values().all(|count| *count > 0),
            "every configured GF180 density material must emit geometry: {counts:?}"
        );
        let active_fill = ir
            .shapes
            .iter()
            .filter(|shape| {
                shape.purpose == PhysicalShapePurpose::DummyFill
                    && matches!(shape.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff)
            })
            .collect::<Vec<_>>();
        let poly_fill = ir
            .shapes
            .iter()
            .filter(|shape| {
                shape.purpose == PhysicalShapePurpose::DummyFill
                    && shape.layer == PhysicalLayer::Poly
            })
            .collect::<Vec<_>>();
        assert_eq!(
            poly_fill.len(),
            active_fill.len(),
            "every dummy active tile must receive covering dummy poly"
        );
        assert!(active_fill
            .iter()
            .all(|active| poly_fill.iter().any(|poly| {
                poly.x - poly.width / 2.0 <= active.x - active.width / 2.0 + 1e-9
                    && poly.x + poly.width / 2.0 >= active.x + active.width / 2.0 - 1e-9
                    && poly.y - poly.height / 2.0 <= active.y - active.height / 2.0 + 1e-9
                    && poly.y + poly.height / 2.0 >= active.y + active.height / 2.0 - 1e-9
            })));
        assert_eq!(
            physical_drc::validate(&ir, &technology).error_count,
            0,
            "reserved FEOL field and fill must remain native-DRC legal"
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
    fn mixed_width_devices_drive_channel_geometry_without_breaking_lvs() {
        let mut project = inverter();
        let pmos = project
            .components
            .iter()
            .find(|component| component.kind == "pmos")
            .unwrap()
            .id;
        let nmos = project
            .components
            .iter()
            .find(|component| component.kind == "nmos")
            .unwrap()
            .id;
        project.set_device_geometry(pmos, 2.4, 0.28).unwrap();
        project.set_device_geometry(nmos, 0.7, 0.28).unwrap();

        let ir = normalize(&project).unwrap();
        let active_height = |component_id| {
            ir.shapes
                .iter()
                .find(|shape| {
                    shape.component_id == Some(component_id)
                        && shape.purpose == PhysicalShapePurpose::Active
                })
                .map(|shape| shape.height)
                .unwrap()
        };
        assert!((active_height(pmos) - 2.4).abs() < 1e-9);
        assert!((active_height(nmos) - 0.7).abs() < 1e-9);
        let drc = physical_drc::validate(&ir, &project.technology);
        assert!(drc.diagnostics.is_empty(), "{:?}", drc.diagnostics);
        assert!(crate::physical_lvs::compare(&ir, &project.technology).matched);
        assert_eq!(ir, normalize(&project).unwrap());
    }

    #[test]
    fn physical_generation_reports_real_monotonic_stage_progress() {
        let mut updates = Vec::new();
        normalize_with_progress(&inverter(), &mut |stage, percent| {
            updates.push((stage, percent));
        })
        .unwrap();

        assert!(updates.windows(2).all(|pair| pair[0].1 <= pair[1].1));
        for expected in [
            "topology",
            "planning",
            "candidateRouting",
            "geometryRefinement",
            "connectivityCleanup",
            "physicalIr",
        ] {
            assert!(updates.iter().any(|(stage, _)| *stage == expected));
        }
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
                matches!(shape.layer, PhysicalLayer::Metal(layer) if layer >= 3)
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
        let power_nets = ir
            .nets
            .iter()
            .filter(|net| matches!(net.role, NetRole::Power | NetRole::Ground))
            .map(|net| net.id)
            .collect::<HashSet<_>>();
        let distributed_power_tracks = ir
            .shapes
            .iter()
            .filter(|shape| {
                shape.layer == PhysicalLayer::Metal(2)
                    && shape.net.is_some_and(|net_id| power_nets.contains(&net_id))
            })
            .map(|shape| (shape.y * 1000.0).round() as i64)
            .collect::<HashSet<_>>();
        assert!(
            distributed_power_tracks.len() > 2,
            "multi-row placement should synthesize more than one VDD/GND rail pair"
        );
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
        let wells = ir
            .shapes
            .iter()
            .filter(|shape| shape.layer == PhysicalLayer::Nwell)
            .collect::<Vec<_>>();
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
                    wells.iter().any(|well| {
                        well.x - well.width / 2.0 <= shape.x - shape.width / 2.0 - enclosure + 1e-9
                            && well.x + well.width / 2.0
                                >= shape.x + shape.width / 2.0 + enclosure - 1e-9
                            && well.y - well.height / 2.0
                                <= shape.y - shape.height / 2.0 - enclosure + 1e-9
                            && well.y + well.height / 2.0
                                >= shape.y + shape.height / 2.0 + enclosure - 1e-9
                    }),
                    "P diffusion escaped every N-well stripe: {:?}, wells: {:?}",
                    shape,
                    wells
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
    fn hierarchy_candidate_is_retained_without_forcing_macro_geometry() {
        let project = hierarchical_inverter_pair();
        let flattened = project.flattened().unwrap();
        let ir = normalize(&flattened).unwrap();
        assert!(ir
            .devices
            .iter()
            .all(|device| device.physical_group.is_some()));
        let selected = &ir.placement.candidates[ir.placement.selected_candidate];
        assert!(matches!(
            selected.strategy,
            crate::physical_placement::PlacementStrategy::Hierarchy
                | crate::physical_placement::PlacementStrategy::LeafTopology
                | crate::physical_placement::PlacementStrategy::GeometryTopology
                | crate::physical_placement::PlacementStrategy::LeafGeometryTopology
        ));
        let standard_cells = ir
            .placement
            .standard_cell_candidate
            .as_ref()
            .expect("repeated leaf blocks should produce a staged standard-cell candidate");
        assert_eq!(standard_cells.standard_cell_instances, 2);
        assert!(standard_cells.legal);
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
    fn nested_hierarchy_uses_lowest_reusable_instances_as_standard_cells() {
        let flattened = nested_inverter_pair().flattened().unwrap();
        let ir = normalize(&flattened).unwrap();
        let top_groups = ir
            .devices
            .iter()
            .filter_map(|device| device.physical_group.as_deref())
            .collect::<HashSet<_>>();
        let leaf_groups = ir
            .devices
            .iter()
            .filter_map(|device| device.standard_cell_group.as_deref())
            .collect::<HashSet<_>>();
        assert_eq!(top_groups.len(), 1);
        assert_eq!(leaf_groups.len(), 2);
        assert!(leaf_groups
            .iter()
            .all(|group| group.matches('·').count() == 1));
        let standard_cells = ir
            .placement
            .standard_cell_candidate
            .as_ref()
            .expect("nested reusable leaf blocks should produce a cell candidate");
        assert_eq!(standard_cells.standard_cell_instances, 2);
        assert_eq!(standard_cells.block_regions.len(), 2);
        assert_eq!(
            standard_cells
                .block_regions
                .iter()
                .map(|region| region.name.as_str())
                .collect::<HashSet<_>>(),
            leaf_groups
        );
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

    #[test]
    fn residual_open_search_crosses_multiple_layers_without_a_time_cutoff() {
        let rules = Technology::default().physical_rules;
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: -2.0,
                y: 0.0,
                width: 0.6,
                height: 0.6,
                component_id: None,
                net: Some(7),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 2.0,
                y: 0.0,
                width: 0.6,
                height: 0.6,
                component_id: None,
                net: Some(7),
                purpose: PhysicalShapePurpose::Route,
            },
        ];
        for layer in [1, 2] {
            shapes.push(PhysicalShape {
                layer: PhysicalLayer::Metal(layer),
                x: 0.0,
                y: 0.0,
                width: 0.8,
                height: 100.0,
                component_id: None,
                net: Some(8),
                purpose: PhysicalShapePurpose::Route,
            });
        }
        let mut canvas = PhysicalCanvas::new(&rules);
        for (index, shape) in shapes.iter().enumerate() {
            canvas.index_unchecked(shape, format!("fixture-{index}"), ObstructionType::Metal);
        }
        let candidate = multilayer_track_search(&canvas, &shapes, &[0], &[1], 7, 3, &rules, 1)
            .expect("the search should promote above the lower-layer barriers");
        assert!(candidate
            .iter()
            .any(|shape| shape.layer == PhysicalLayer::Metal(3)));
        assert!(candidate
            .iter()
            .any(|shape| matches!(shape.layer, PhysicalLayer::Via(_))));
        assert!(canvas
            .commit_routing_geometry(&candidate, "fixture-route")
            .is_ok());
    }

    #[test]
    fn same_net_subspacing_notches_are_filled_without_moving_terminals() {
        let rules = Technology::default().physical_rules;
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.0,
                y: 0.0,
                width: 0.4,
                height: 0.4,
                component_id: None,
                net: Some(7),
                purpose: PhysicalShapePurpose::ViaLanding,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 0.31,
                y: 0.0,
                width: 0.2,
                height: 0.7,
                component_id: None,
                net: Some(7),
                purpose: PhysicalShapePurpose::Route,
            },
        ];
        let original = shapes.clone();
        assert_eq!(super::fill_same_net_metal_notches(&mut shapes, &rules), 1);
        assert_eq!(&shapes[..2], original.as_slice());
        let fill = &shapes[2];
        assert_eq!(fill.net, Some(7));
        assert_eq!(fill.layer, PhysicalLayer::Metal(1));
        assert_eq!(fill.purpose, PhysicalShapePurpose::RouteFill);
        assert!(fill.x - fill.width / 2.0 <= 0.2 + 1e-9);
        assert!(fill.x + fill.width / 2.0 >= 0.21 - 1e-9);
    }

    #[test]
    fn orthogonal_same_net_routes_receive_a_full_width_corner() {
        let mut rules = Technology::default().physical_rules;
        rules.metal.min_width_um = 0.23;
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 2.115,
                y: 4.7875,
                width: 0.23,
                height: 0.635,
                component_id: None,
                net: Some(70),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: 1.7625,
                y: 5.17,
                width: 0.635,
                height: 0.23,
                component_id: None,
                net: Some(70),
                purpose: PhysicalShapePurpose::Route,
            },
        ];

        assert_eq!(super::fill_same_net_metal_notches(&mut shapes, &rules), 1);
        let fill = &shapes[2];
        assert_eq!(fill.purpose, PhysicalShapePurpose::RouteFill);
        assert!((fill.x - 2.115).abs() < 1e-9);
        assert!((fill.y - 5.17).abs() < 1e-9);
        assert!(fill.width >= 0.23 - 1e-9);
        assert!(fill.height >= 0.23 - 1e-9);
        assert!(fill.width <= 0.23 + 2.0 * rules.manufacturing_grid_um + 1e-9);
        assert!(fill.height <= 0.23 + 2.0 * rules.manufacturing_grid_um + 1e-9);
    }

    #[test]
    fn edge_touching_m1_route_and_landing_receive_a_full_width_neck() {
        let mut rules = Technology::default().physical_rules;
        rules.manufacturing_grid_um = 0.005;
        rules.metal.min_width_um = 0.23;
        rules
            .layer_overrides
            .entry("metal1".into())
            .or_insert_with(|| rules.metal.clone())
            .min_width_um = 0.23;
        let mut shapes = vec![
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: -26.115,
                y: -19.210,
                width: 0.640,
                height: 0.230,
                component_id: None,
                net: Some(17),
                purpose: PhysicalShapePurpose::Route,
            },
            PhysicalShape {
                layer: PhysicalLayer::Metal(1),
                x: -25.670,
                y: -18.900,
                width: 0.390,
                height: 0.390,
                component_id: None,
                net: Some(17),
                purpose: PhysicalShapePurpose::ViaLanding,
            },
        ];

        assert_eq!(super::fill_same_net_metal_notches(&mut shapes, &rules), 1);
        let fill = &shapes[2];
        assert_eq!(fill.layer, PhysicalLayer::Metal(1));
        assert_eq!(fill.net, Some(17));
        assert_eq!(fill.purpose, PhysicalShapePurpose::RouteFill);
        assert!(fill.width >= 0.23 - 1e-9);
        assert!(fill.height >= 0.23 - 1e-9);
        assert!(fill.x + fill.width / 2.0 >= -25.635 - 1e-9);
        assert!((fill.y + fill.height / 2.0 - -19.095).abs() < 1e-9);
    }
}
