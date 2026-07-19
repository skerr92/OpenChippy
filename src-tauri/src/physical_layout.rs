use crate::model::{Component, Project, TerminalRef};
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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

fn compact_preview(
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    max_metal_layers: u16,
) -> (PhysicalBounds, Vec<PhysicalShape>) {
    const X_PITCH: f64 = 2.0;
    const Y_PITCH: f64 = 1.7;

    let pmos_count = devices
        .iter()
        .filter(|device| device.kind == DeviceKind::Pmos)
        .count();
    let nmos_count = devices
        .iter()
        .filter(|device| device.kind == DeviceKind::Nmos)
        .count();
    let largest_row = pmos_count.max(nmos_count).max(1);
    let pin_count = pins
        .iter()
        .filter(|pin| pin.role == NetRole::Input || pin.role == NetRole::Output)
        .count();
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
    let mut columns = 1;
    let mut best_score = f64::INFINITY;
    for candidate in 1..=largest_row {
        let pmos_rows = pmos_count.max(1).div_ceil(candidate);
        let nmos_rows = nmos_count.max(1).div_ceil(candidate);
        let width = candidate as f64 * X_PITCH + 2.0;
        let height = (pmos_rows + nmos_rows) as f64 * Y_PITCH + center_channel + 2.6;
        let pin_width = pin_count.div_ceil(2) as f64 * 0.7 + 2.0;
        let aspect = width.max(pin_width) / height;
        let score = aspect.ln().abs();
        if score < best_score {
            best_score = score;
            columns = candidate;
        }
    }
    let pmos_rows = pmos_count.max(1).div_ceil(columns);
    let nmos_rows = nmos_count.max(1).div_ceil(columns);
    let input_pins = pins.iter().filter(|pin| pin.role == NetRole::Input).count();
    let output_pins = pins
        .iter()
        .filter(|pin| pin.role == NetRole::Output)
        .count();
    let pin_columns = input_pins.max(output_pins);
    let cell_width = (columns as f64 * X_PITCH + 2.0).max(pin_columns as f64 * 0.7 + 2.0);
    let cell_height = (pmos_rows + nmos_rows) as f64 * Y_PITCH + center_channel + 2.6;
    let vdd_y = -cell_height / 2.0 + 0.5;
    let gnd_y = cell_height / 2.0 - 0.5;
    let bounds = PhysicalBounds {
        min_x: -cell_width / 2.0,
        min_y: -cell_height / 2.0,
        max_x: cell_width / 2.0,
        max_y: cell_height / 2.0,
    };
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
            y: (bounds.min_y - center_channel / 2.0) / 2.0,
            width: cell_width - 0.5,
            height: -center_channel / 2.0 - bounds.min_y - 0.35,
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
        for (index, device) in row.iter().enumerate() {
            let row_index = index / columns;
            let column_index = index % columns;
            let items_in_row = (row.len() - row_index * columns).min(columns);
            let kind_rows = if kind == DeviceKind::Pmos {
                pmos_rows
            } else {
                nmos_rows
            };
            let x = (column_index as f64 - (items_in_row.saturating_sub(1) as f64 / 2.0)) * X_PITCH
                + (row_index as f64 - kind_rows.saturating_sub(1) as f64 / 2.0) * 0.28
                + if kind == DeviceKind::Pmos {
                    -0.18
                } else {
                    0.18
                };
            let y = if kind == DeviceKind::Pmos {
                -center_channel / 2.0 - 0.7 - row_index as f64 * Y_PITCH
            } else {
                center_channel / 2.0 + 0.7 + row_index as f64 * Y_PITCH
            };
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
                height: 1.25,
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
            let drop_layer_index = if !signal {
                if max_metal_layers >= 3 {
                    2
                } else {
                    1
                }
            } else if max_metal_layers < 3 {
                1
            } else if anchor.point.1 > center_channel / 2.0 {
                max_metal_layers - 1
            } else {
                max_metal_layers.saturating_sub(2).max(1)
            };
            let drop_layer = PhysicalLayer::Metal(drop_layer_index);
            let drop_x = anchor.point.0;
            if (drop_x - anchor.point.0).abs() > f64::EPSILON {
                shapes.push(line_shape(
                    anchor.point,
                    (drop_x, anchor.point.1),
                    PhysicalLayer::Metal(1),
                    Some(net.id),
                ));
            }
            if (anchor.point.1 - track_y).abs() > f64::EPSILON {
                shapes.push(line_shape(
                    (drop_x, anchor.point.1),
                    (drop_x, track_y),
                    if anchor.layer == PhysicalLayer::Metal(1) {
                        drop_layer
                    } else {
                        anchor.layer
                    },
                    Some(net.id),
                ));
            }
            if signal && anchor.layer == PhysicalLayer::Metal(1) {
                let target_layer = signal_layer[&net.id];
                if drop_layer_index > 1 {
                    shapes.push(PhysicalShape {
                        layer: PhysicalLayer::Metal(1),
                        x: drop_x,
                        y: anchor.point.1,
                        width: ROUTING_LANDING_SIZE,
                        height: ROUTING_LANDING_SIZE,
                        component_id: None,
                        net: Some(net.id),
                    });
                    for intermediate_layer in 2..drop_layer_index {
                        shapes.push(PhysicalShape {
                            layer: PhysicalLayer::Metal(intermediate_layer),
                            x: drop_x,
                            y: anchor.point.1,
                            width: ROUTING_LANDING_SIZE,
                            height: ROUTING_LANDING_SIZE,
                            component_id: None,
                            net: Some(net.id),
                        });
                    }
                    for lower_layer in 1..drop_layer_index {
                        shapes.push(PhysicalShape {
                            layer: PhysicalLayer::Via(lower_layer),
                            x: drop_x,
                            y: anchor.point.1,
                            width: ROUTING_VIA_SIZE,
                            height: ROUTING_VIA_SIZE,
                            component_id: None,
                            net: Some(net.id),
                        });
                    }
                }
                for intermediate_layer in (drop_layer_index + 1)..target_layer {
                    shapes.push(PhysicalShape {
                        layer: PhysicalLayer::Metal(intermediate_layer),
                        x: drop_x,
                        y: track_y,
                        width: ROUTING_LANDING_SIZE,
                        height: ROUTING_LANDING_SIZE,
                        component_id: None,
                        net: Some(net.id),
                    });
                }
                for lower_layer in drop_layer_index..target_layer {
                    shapes.push(PhysicalShape {
                        layer: PhysicalLayer::Via(lower_layer),
                        x: drop_x,
                        y: track_y,
                        width: ROUTING_VIA_SIZE,
                        height: ROUTING_VIA_SIZE,
                        component_id: None,
                        net: Some(net.id),
                    });
                }
            } else if !signal && drop_layer_index > 1 {
                for y in [anchor.point.1, track_y] {
                    shapes.push(PhysicalShape {
                        layer: PhysicalLayer::Metal(1),
                        x: drop_x,
                        y,
                        width: ROUTING_LANDING_SIZE,
                        height: ROUTING_LANDING_SIZE,
                        component_id: None,
                        net: Some(net.id),
                    });
                    shapes.push(PhysicalShape {
                        layer: PhysicalLayer::Via(1),
                        x: drop_x,
                        y,
                        width: ROUTING_VIA_SIZE,
                        height: ROUTING_VIA_SIZE,
                        component_id: None,
                        net: Some(net.id),
                    });
                }
            }
        }
    }
    (bounds, shapes)
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
    let (bounds, shapes) =
        compact_preview(&devices, &nets, &pins, project.technology.max_metal_layers);

    Ok(PhysicalLayoutIr {
        format_version: CURRENT_PHYSICAL_IR_VERSION,
        source_project_name: project.name.clone(),
        technology_name: project.technology.name.clone(),
        max_metal_layers: project.technology.max_metal_layers,
        devices,
        nets,
        pins,
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
            aspect_ratio < 1.5,
            "large arrays should fold toward a square, got {width} × {height}"
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
