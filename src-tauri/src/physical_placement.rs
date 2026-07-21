use crate::{
    physical_layout::{DeviceKind, NetRole, PhysicalDevice, PhysicalNet},
    physical_planning::PhysicalPlanningReport,
    technology::Technology,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementStrategy {
    Topology,
    Diffusion,
    Congestion,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedDevice {
    pub component_id: Uuid,
    pub name: String,
    pub kind: DeviceKind,
    pub x: f64,
    pub y: f64,
    pub row: usize,
    pub column: usize,
    pub site: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementCandidate {
    pub id: usize,
    pub strategy: PlacementStrategy,
    pub devices: Vec<PlacedDevice>,
    pub legal: bool,
    pub estimated_wire_length_um: f64,
    pub peak_bin_utilization: f64,
    pub diffusion_sharing_pairs: usize,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalPlacementReport {
    pub device_pitch_um: f64,
    pub candidates: Vec<PlacementCandidate>,
    pub selected_candidate: usize,
}

impl Default for PhysicalPlacementReport {
    fn default() -> Self {
        Self {
            device_pitch_um: 2.0,
            candidates: Vec::new(),
            selected_candidate: 0,
        }
    }
}

fn device_nets(device: &PhysicalDevice) -> [usize; 3] {
    [device.gate_net, device.drain_net, device.source_net]
}

fn shared_weight(
    left: &PhysicalDevice,
    right: &PhysicalDevice,
    power_nets: &HashSet<usize>,
) -> usize {
    let left_nets = device_nets(left);
    let right_nets = device_nets(right);
    left_nets
        .into_iter()
        .filter(|net| !power_nets.contains(net) && right_nets.contains(net))
        .map(|net| {
            if (left.drain_net == net || left.source_net == net)
                && (right.drain_net == net || right.source_net == net)
            {
                3
            } else {
                1
            }
        })
        .sum()
}

fn topology_order<'a>(
    devices: Vec<&'a PhysicalDevice>,
    power_nets: &HashSet<usize>,
) -> Vec<&'a PhysicalDevice> {
    let mut remaining = devices;
    let mut ordered: Vec<&PhysicalDevice> = Vec::new();
    while !remaining.is_empty() {
        let next_index = if let Some(previous) = ordered.last() {
            remaining
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| {
                    shared_weight(previous, left, power_nets)
                        .cmp(&shared_weight(previous, right, power_nets))
                        .then_with(|| right.name.cmp(&left.name))
                })
                .map(|(index, _)| index)
                .unwrap_or(0)
        } else {
            remaining
                .iter()
                .enumerate()
                .max_by_key(|(_, device)| {
                    let degree = remaining
                        .iter()
                        .map(|other| shared_weight(device, other, power_nets))
                        .sum::<usize>();
                    (degree, std::cmp::Reverse(device.name.as_str()))
                })
                .map(|(index, _)| index)
                .unwrap_or(0)
        };
        ordered.push(remaining.remove(next_index));
    }
    ordered
}

fn strategy_order<'a>(
    devices: &'a [PhysicalDevice],
    kind: DeviceKind,
    strategy: PlacementStrategy,
    power_nets: &HashSet<usize>,
) -> Vec<&'a PhysicalDevice> {
    let mut row = devices
        .iter()
        .filter(|device| device.kind == kind)
        .collect::<Vec<_>>();
    match strategy {
        PlacementStrategy::Topology => topology_order(row, power_nets),
        PlacementStrategy::Diffusion => {
            row.sort_by_key(|device| {
                (
                    device.drain_net.min(device.source_net),
                    device.drain_net.max(device.source_net),
                    device.gate_net,
                    device.name.as_str(),
                )
            });
            row
        }
        PlacementStrategy::Congestion => {
            row.sort_by_key(|device| {
                (
                    std::cmp::Reverse(
                        device_nets(device)
                            .into_iter()
                            .filter(|net| !power_nets.contains(net))
                            .collect::<HashSet<_>>()
                            .len(),
                    ),
                    device.name.as_str(),
                )
            });
            let mut spread = Vec::with_capacity(row.len());
            let mut left = 0;
            let mut right = row.len();
            while left < right {
                spread.push(row[left]);
                left += 1;
                if left < right {
                    right -= 1;
                    spread.push(row[right]);
                }
            }
            spread
        }
    }
}

fn legalize_row(
    ordered: &[&PhysicalDevice],
    kind: DeviceKind,
    columns: usize,
    width: f64,
    height: f64,
    site_width: f64,
    row_height: f64,
    device_pitch: f64,
) -> Vec<PlacedDevice> {
    let edge_margin = 1.25;
    ordered
        .iter()
        .enumerate()
        .map(|(index, device)| {
            let row = index / columns;
            let logical_column = index % columns;
            let items_in_row = (ordered.len() - row * columns).min(columns);
            let column = if row % 2 == 1 {
                items_in_row - logical_column - 1
            } else {
                logical_column
            };
            let unsnapped_x =
                (column as f64 - items_in_row.saturating_sub(1) as f64 / 2.0) * device_pitch;
            let site = (unsnapped_x / site_width).round() as i64;
            let y = if kind == DeviceKind::Pmos {
                -height / 2.0 + edge_margin + row as f64 * row_height
            } else {
                height / 2.0 - edge_margin - row as f64 * row_height
            };
            PlacedDevice {
                component_id: device.component_id,
                name: device.name.clone(),
                kind,
                x: site as f64 * site_width,
                y,
                row,
                column,
                site,
            }
        })
        .filter(|placed| placed.x.abs() + 0.775 <= width / 2.0 + 1e-9)
        .collect()
}

fn candidate_metrics(
    placements: &[PlacedDevice],
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    planning: &PhysicalPlanningReport,
) -> (f64, f64, usize) {
    let positions = placements
        .iter()
        .map(|device| (device.component_id, (device.x, device.y)))
        .collect::<HashMap<_, _>>();
    let mut wire_length = 0.0;
    let mut bin_demand = vec![0.0; planning.routing_bins.len()];
    for net in nets
        .iter()
        .filter(|net| !matches!(net.role, NetRole::Power | NetRole::Ground))
    {
        let points = net
            .terminals
            .iter()
            .filter_map(|terminal| positions.get(&terminal.component_id).copied())
            .collect::<Vec<_>>();
        if points.is_empty() {
            continue;
        }
        let min_x = points
            .iter()
            .map(|point| point.0)
            .fold(f64::INFINITY, f64::min);
        let max_x = points
            .iter()
            .map(|point| point.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_y = points
            .iter()
            .map(|point| point.1)
            .fold(f64::INFINITY, f64::min);
        let max_y = points
            .iter()
            .map(|point| point.1)
            .fold(f64::NEG_INFINITY, f64::max);
        wire_length += max_x - min_x + max_y - min_y;
        for bin in &planning.routing_bins {
            if bin.max_x >= min_x && bin.min_x <= max_x && bin.max_y >= min_y && bin.min_y <= max_y
            {
                bin_demand[bin.id] += 1.0;
            }
        }
    }
    let peak_utilization = planning
        .routing_bins
        .iter()
        .map(|bin| {
            let capacity = bin
                .layer_capacities
                .iter()
                .filter(|layer| !layer.reserved_for_power)
                .map(|layer| usize::from(layer.horizontal_tracks + layer.vertical_tracks))
                .sum::<usize>()
                .max(1) as f64;
            bin_demand[bin.id] / capacity
        })
        .fold(0.0, f64::max);
    let by_id = devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let mut sharing = 0;
    for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
        let mut rows = HashMap::<usize, Vec<&PlacedDevice>>::new();
        for placed in placements.iter().filter(|placed| placed.kind == kind) {
            rows.entry(placed.row).or_default().push(placed);
        }
        for row in rows.values_mut() {
            row.sort_by_key(|placed| placed.column);
            sharing += row
                .windows(2)
                .filter(|pair| {
                    let left = by_id[&pair[0].component_id];
                    let right = by_id[&pair[1].component_id];
                    [left.drain_net, left.source_net]
                        .into_iter()
                        .any(|net| [right.drain_net, right.source_net].contains(&net))
                })
                .count();
        }
    }
    (wire_length, peak_utilization, sharing)
}

pub fn place(
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    technology: &Technology,
    planning: &PhysicalPlanningReport,
) -> PhysicalPlacementReport {
    let selected_floorplan = &planning.candidates[planning.selected_candidate];
    let device_pitch = ((1.55 + technology.physical_rules.diffusion.min_spacing_um)
        / planning.placement_site_width_um)
        .ceil()
        * planning.placement_site_width_um;
    let power_nets = nets
        .iter()
        .filter(|net| matches!(net.role, NetRole::Power | NetRole::Ground))
        .map(|net| net.id)
        .collect::<HashSet<_>>();
    let strategies = [
        PlacementStrategy::Topology,
        PlacementStrategy::Diffusion,
        PlacementStrategy::Congestion,
    ];
    let mut candidates = Vec::new();
    for (id, strategy) in strategies.into_iter().enumerate() {
        let mut placements = Vec::with_capacity(devices.len());
        for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
            placements.extend(legalize_row(
                &strategy_order(devices, kind, strategy, &power_nets),
                kind,
                selected_floorplan.columns,
                selected_floorplan.width_um,
                selected_floorplan.height_um,
                planning.placement_site_width_um,
                planning.row_height_um,
                device_pitch,
            ));
        }
        let legal = placements.len() == devices.len();
        let (wire_length, peak_utilization, sharing) =
            candidate_metrics(&placements, devices, nets, planning);
        candidates.push(PlacementCandidate {
            id,
            strategy,
            devices: placements,
            legal,
            estimated_wire_length_um: wire_length,
            peak_bin_utilization: peak_utilization,
            diffusion_sharing_pairs: sharing,
            score: if legal {
                wire_length + peak_utilization * 25.0 - sharing as f64 * device_pitch * 0.35
            } else {
                1e12
            },
        });
    }
    let selected_candidate = candidates
        .iter()
        .min_by(|left, right| left.score.total_cmp(&right.score))
        .map(|candidate| candidate.id)
        .unwrap_or(0);
    PhysicalPlacementReport {
        device_pitch_um: device_pitch,
        candidates,
        selected_candidate,
    }
}
