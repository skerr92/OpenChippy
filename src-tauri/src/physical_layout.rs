use crate::model::{Component, Project, TerminalRef};
use crate::physical_detailed_routing::{self, DetailedRoutingReport};
use crate::physical_global_routing::{self, GlobalRoutingReport};
use crate::physical_placement::{self, PhysicalPlacementReport};
use crate::physical_planning::{self, PhysicalPlanningReport};
use crate::technology::PhysicalRuleDeck;
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use uuid::Uuid;

pub const CURRENT_PHYSICAL_IR_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize)]
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
    pub bounds: PhysicalBounds,
    pub shapes: Vec<PhysicalShape>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDevice {
    pub component_id: Uuid,
    pub name: String,
    pub kind: DeviceKind,
    pub gate_net: usize,
    pub drain_net: usize,
    pub source_net: usize,
    pub width_um: f64,
    pub length_um: f64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Nmos,
    Pmos,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalNet {
    pub id: usize,
    pub name: String,
    pub role: NetRole,
    pub terminals: Vec<PhysicalTerminal>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NetRole {
    Power,
    Ground,
    Input,
    Output,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalTerminal {
    pub component_id: Uuid,
    pub component_name: String,
    pub terminal: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalPin {
    pub component_id: Uuid,
    pub name: String,
    pub role: NetRole,
    pub net: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
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
}

const ROUTING_LANDING_SIZE: f64 = 0.28;
const ROUTING_VIA_SIZE: f64 = 0.24;
const ROUTING_CLEARANCE: f64 = 0.04;
const TRACK_SPACING: f64 = ROUTING_LANDING_SIZE + ROUTING_CLEARANCE;

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
            0.08
        } else {
            ROUTING_CLEARANCE
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

fn preview_spacing_conflicts(shapes: &[PhysicalShape]) -> usize {
    let mut conflicts = std::collections::HashSet::new();
    for (left_index, left) in shapes.iter().enumerate() {
        let Some(left_net) = left.net else { continue };
        let clearance = match left.layer {
            PhysicalLayer::Metal(_) => ROUTING_CLEARANCE,
            PhysicalLayer::Via(_) => 0.08,
            _ => continue,
        };
        let left_edges = (
            left.x - left.width / 2.0,
            left.y - left.height / 2.0,
            left.x + left.width / 2.0,
            left.y + left.height / 2.0,
        );
        for right in shapes.iter().skip(left_index + 1) {
            let Some(right_net) = right.net else { continue };
            if right.layer != left.layer || right_net == left_net {
                continue;
            }
            let right_edges = (
                right.x - right.width / 2.0,
                right.y - right.height / 2.0,
                right.x + right.width / 2.0,
                right.y + right.height / 2.0,
            );
            if left_edges.0 < right_edges.2 + clearance
                && left_edges.2 + clearance > right_edges.0
                && left_edges.1 < right_edges.3 + clearance
                && left_edges.3 + clearance > right_edges.1
            {
                conflicts.insert((left.layer, left_net.min(right_net), left_net.max(right_net)));
            }
        }
    }
    conflicts.len()
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
            net.role == NetRole::Input
                || net.role == NetRole::Output
                || net.role == NetRole::Internal
        })
        .count();
    let first_horizontal_layer = max_metal_layers.max(2);
    let horizontal_layer_count = usize::from(max_metal_layers - first_horizontal_layer + 1);
    // Tracks keep distinct Y coordinates even when their horizontal metals differ:
    // a via stack crosses every intermediate metal and would otherwise land on
    // another net sharing the same coordinate.
    let center_channel = (signal_net_count.max(1) as f64 * TRACK_SPACING + 0.9).max(1.4);
    let selected = &planning.candidates[planning.selected_candidate];
    let selected_placement = &placement.candidates[placement.selected_candidate];
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
        line_shape(
            (bounds.min_x + 0.25, vdd_y),
            (bounds.max_x - 0.25, vdd_y),
            PhysicalLayer::Metal(1),
            nets.iter()
                .find(|net| net.role == NetRole::Power)
                .map(|net| net.id),
        ),
        line_shape(
            (bounds.min_x + 0.25, gnd_y),
            (bounds.max_x - 0.25, gnd_y),
            PhysicalLayer::Metal(1),
            nets.iter()
                .find(|net| net.role == NetRole::Ground)
                .map(|net| net.id),
        ),
    ];

    let mut anchors: HashMap<usize, Vec<RouteAnchor>> = HashMap::new();
    for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
        let row = devices
            .iter()
            .filter(|device| device.kind == kind)
            .collect::<Vec<_>>();
        for device in row {
            let placed = placed_by_id[&device.component_id];
            let x = placed.x;
            let y = placed.y;
            let diffusion_height = 0.55 + device.width_um.min(4.0) * 0.12;
            shapes.push(PhysicalShape {
                layer: if kind == DeviceKind::Pmos {
                    PhysicalLayer::Pdiff
                } else {
                    PhysicalLayer::Ndiff
                },
                x,
                y,
                width: 1.55,
                height: diffusion_height,
                component_id: Some(device.component_id),
                net: None,
            });
            shapes.push(PhysicalShape {
                layer: PhysicalLayer::Poly,
                x,
                y,
                width: 0.22_f64.max(device.length_um * 0.22),
                height: 1.25_f64.max(diffusion_height + rules.gate_extension_um * 2.0),
                component_id: Some(device.component_id),
                net: Some(device.gate_net),
            });
            for (terminal_x, net) in [(x - 0.58, device.drain_net), (x + 0.58, device.source_net)] {
                anchors.entry(net).or_default().push(RouteAnchor {
                    point: (terminal_x, y),
                    layer: PhysicalLayer::Metal(1),
                });
                shapes.push(PhysicalShape {
                    layer: PhysicalLayer::Contact,
                    x: terminal_x,
                    y,
                    width: 0.22,
                    height: 0.22,
                    component_id: Some(device.component_id),
                    net: Some(net),
                });
            }
            anchors
                .entry(device.gate_net)
                .or_default()
                .push(RouteAnchor {
                    point: (x, y),
                    layer: PhysicalLayer::Metal(1),
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

    for net in nets {
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
        shapes.push(line_shape(
            (min_x, track_y),
            (max_x, track_y),
            route_layer,
            Some(net.id),
        ));
        for anchor in points {
            let preferred_drop = if !signal {
                2
            } else if anchor.point.1 > center_channel / 2.0 {
                max_metal_layers.saturating_sub(1).max(1)
            } else {
                max_metal_layers.saturating_sub(2).max(1)
            };
            let drop_x = anchor.point.0;
            let candidate_layers = if max_metal_layers < 3 {
                vec![1]
            } else {
                (2..max_metal_layers).collect::<Vec<_>>()
            };
            let mut candidates = Vec::new();
            for drop_layer_index in candidate_layers {
                let mut candidate = Vec::new();
                if (anchor.point.1 - track_y).abs() > f64::EPSILON {
                    candidate.push(line_shape(
                        (drop_x, anchor.point.1),
                        (drop_x, track_y),
                        if anchor.layer == PhysicalLayer::Metal(1) {
                            PhysicalLayer::Metal(drop_layer_index)
                        } else {
                            anchor.layer
                        },
                        Some(net.id),
                    ));
                }
                if signal && anchor.layer == PhysicalLayer::Metal(1) {
                    let target_layer = signal_layer[&net.id];
                    add_via_stack(&mut candidate, anchor.point, 1, drop_layer_index, net.id);
                    add_via_stack(
                        &mut candidate,
                        (drop_x, track_y),
                        drop_layer_index,
                        target_layer,
                        net.id,
                    );
                } else if !signal && drop_layer_index > 1 {
                    add_via_stack(&mut candidate, anchor.point, 1, drop_layer_index, net.id);
                    add_via_stack(
                        &mut candidate,
                        (drop_x, track_y),
                        1,
                        drop_layer_index,
                        net.id,
                    );
                }
                let conflicts = route_shape_conflicts(&shapes, &candidate, net.id);
                candidates.push((
                    conflicts,
                    drop_layer_index.abs_diff(preferred_drop),
                    candidate,
                ));
            }
            if let Some((_, _, candidate)) = candidates
                .into_iter()
                .min_by_key(|(conflicts, preference, _)| (*conflicts, *preference))
            {
                shapes.extend(candidate);
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
    let mut best = None;
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
        let candidate_global =
            physical_global_routing::route(&nets, &pins, &candidate_placement, &candidate_plan);
        let candidate_detailed = physical_detailed_routing::route(
            &nets,
            &pins,
            &candidate_global,
            &candidate_plan,
            &project.technology,
        );
        let placement_score =
            candidate_placement.candidates[candidate_placement.selected_candidate].score;
        let better = best.as_ref().is_none_or(
            |(_, placement, global, detailed): &(
                usize,
                PhysicalPlacementReport,
                GlobalRoutingReport,
                DetailedRoutingReport,
            )| {
                let best_score = placement.candidates[placement.selected_candidate].score;
                candidate_global.total_overflow < global.total_overflow
                    || (candidate_global.total_overflow == global.total_overflow
                        && (candidate_detailed.conflict_count < detailed.conflict_count
                            || (candidate_detailed.conflict_count == detailed.conflict_count
                                && placement_score < best_score)))
            },
        );
        if better {
            best = Some((
                candidate_index,
                candidate_placement,
                candidate_global,
                candidate_detailed,
            ));
        }
    }
    let (selected_candidate, placement, global_routing, detailed_routing) =
        best.ok_or_else(|| "Physical planning produced no feasible candidate.".to_string())?;
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
        preview_candidates.push((preview_spacing_conflicts(&shapes), bounds, shapes));
    }
    let (_, mut bounds, shapes) = preview_candidates
        .into_iter()
        .min_by_key(|(conflicts, _, _)| *conflicts)
        .expect("at least one physical preview candidate");
    bounds.min_x = snap_to_grid(bounds.min_x, grid);
    bounds.min_y = snap_to_grid(bounds.min_y, grid);
    bounds.max_x = snap_to_grid(bounds.max_x, grid);
    bounds.max_y = snap_to_grid(bounds.max_y, grid);

    Ok(PhysicalLayoutIr {
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
        bounds,
        shapes,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        normalize, DeviceKind, NetRole, PhysicalLayer, ROUTING_CLEARANCE, ROUTING_LANDING_SIZE,
    };
    use crate::model::{Project, TerminalRef};
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

    #[test]
    fn generated_inverter_passes_educational_physical_drc() {
        let project = inverter();
        let ir = normalize(&project).unwrap();
        let report = physical_drc::validate(&ir, &Technology::default());
        assert_eq!(
            report.error_count, 0,
            "unexpected physical DRC diagnostics: {:?}",
            report.diagnostics
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
        for route in &ir.detailed_routing.routes {
            assert!(route.pin_access_points > 0);
            for polygon in &route.polygons {
                assert_eq!(polygon.net, Some(route.net));
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
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.rule_id == "GEOMETRY.MIN_SPACING" && diagnostic.shape_indices.len() > 2
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
                &ir.nets,
                &ir.pins,
                &global,
                &planning,
                &project.technology,
            );
            assert!(
                selected_score <= (global.total_overflow, detail.conflict_count),
                "selected physical candidate must minimize routing failure before shape preference"
            );
        }
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
