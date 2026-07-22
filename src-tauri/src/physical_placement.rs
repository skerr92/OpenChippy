use crate::{
    physical_canvas::{device_footprint, reserve_device_footprint, PhysicalCanvas},
    physical_layout::{DeviceKind, NetRole, PhysicalDevice, PhysicalNet},
    physical_planning::PhysicalPlanningReport,
    technology::Technology,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementStrategy {
    Hierarchy,
    Topology,
    Diffusion,
    Congestion,
    Timing,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementCandidate {
    pub id: usize,
    pub strategy: PlacementStrategy,
    pub devices: Vec<PlacedDevice>,
    pub block_regions: Vec<PlacedBlockRegion>,
    pub legal: bool,
    pub estimated_wire_length_um: f64,
    pub peak_bin_utilization: f64,
    pub diffusion_sharing_pairs: usize,
    pub occupancy_retries: usize,
    pub reserved_device_shapes: usize,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedBlockRegion {
    pub name: String,
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub device_count: usize,
    pub immutable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
        PlacementStrategy::Hierarchy => {
            let mut groups = HashMap::<String, Vec<&PhysicalDevice>>::new();
            for device in row {
                let group = device
                    .physical_group
                    .clone()
                    .unwrap_or_else(|| format!("~{}", device.name));
                groups.entry(group).or_default().push(device);
            }
            let mut groups = groups.into_iter().collect::<Vec<_>>();
            groups.sort_by(|left, right| {
                right
                    .1
                    .len()
                    .cmp(&left.1.len())
                    .then_with(|| left.0.cmp(&right.0))
            });
            groups
                .into_iter()
                .flat_map(|(_, devices)| topology_order(devices, power_nets))
                .collect()
        }
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
        PlacementStrategy::Timing => topology_order(row, power_nets),
    }
}

fn block_regions(
    placements: &[PlacedDevice],
    devices: &[PhysicalDevice],
    device_pitch: f64,
    row_height: f64,
) -> Vec<PlacedBlockRegion> {
    let groups = devices
        .iter()
        .filter_map(|device| {
            device
                .physical_group
                .as_ref()
                .map(|group| (device.component_id, group.clone()))
        })
        .collect::<HashMap<_, _>>();
    let mut grouped = HashMap::<String, Vec<&PlacedDevice>>::new();
    for placed in placements {
        if let Some(group) = groups.get(&placed.component_id) {
            grouped.entry(group.clone()).or_default().push(placed);
        }
    }
    let mut regions = grouped
        .into_iter()
        .map(|(name, placed)| PlacedBlockRegion {
            name,
            min_x: placed.iter().map(|d| d.x).fold(f64::INFINITY, f64::min) - device_pitch / 2.0,
            max_x: placed.iter().map(|d| d.x).fold(f64::NEG_INFINITY, f64::max)
                + device_pitch / 2.0,
            min_y: placed.iter().map(|d| d.y).fold(f64::INFINITY, f64::min) - row_height / 2.0,
            max_y: placed.iter().map(|d| d.y).fold(f64::NEG_INFINITY, f64::max) + row_height / 2.0,
            device_count: placed.len(),
            immutable: true,
        })
        .collect::<Vec<_>>();
    regions.sort_by(|left, right| left.name.cmp(&right.name));
    regions
}

struct RowLegalization {
    devices: Vec<PlacedDevice>,
    retries: usize,
    reserved_shapes: usize,
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
    technology: &Technology,
    canvas: &mut PhysicalCanvas,
) -> RowLegalization {
    let edge_margin = 1.25;
    let min_site = ((-width / 2.0) / site_width).ceil() as i64;
    let max_site = ((width / 2.0) / site_width).floor() as i64;
    let mut placements = Vec::with_capacity(ordered.len());
    let mut retries = 0;
    let mut reserved_shapes = 0;
    for (index, device) in ordered.iter().enumerate() {
        let row = index / columns;
        let logical_column = index % columns;
        let items_in_row = (ordered.len() - row * columns).min(columns);
        let nominal_column = if row % 2 == 1 {
            items_in_row - logical_column - 1
        } else {
            logical_column
        };
        let unsnapped_x =
            (nominal_column as f64 - items_in_row.saturating_sub(1) as f64 / 2.0) * device_pitch;
        let desired_site = (unsnapped_x / site_width).round() as i64;
        let y = if kind == DeviceKind::Pmos {
            -height / 2.0 + edge_margin + row as f64 * row_height
        } else {
            height / 2.0 - edge_margin - row as f64 * row_height
        };
        let max_delta = (max_site - min_site).unsigned_abs() as i64;
        let mut sites = vec![desired_site];
        for delta in 1..=max_delta {
            if desired_site - delta >= min_site {
                sites.push(desired_site - delta);
            }
            if desired_site + delta <= max_site {
                sites.push(desired_site + delta);
            }
        }
        let mut placed = None;
        for (attempt, site) in sites.into_iter().enumerate() {
            let x = site as f64 * site_width;
            let footprint = device_footprint(device, x, y, &technology.physical_rules);
            let inside = footprint.iter().all(|(shape, _)| {
                shape.x - shape.width / 2.0 >= -width / 2.0 - 1e-9
                    && shape.x + shape.width / 2.0 <= width / 2.0 + 1e-9
                    && shape.y - shape.height / 2.0 >= -height / 2.0 - 1e-9
                    && shape.y + shape.height / 2.0 <= height / 2.0 + 1e-9
            });
            if !inside {
                continue;
            }
            if reserve_device_footprint(canvas, device, x, y, &technology.physical_rules).is_err() {
                continue;
            }
            retries += attempt;
            reserved_shapes += footprint.len();
            placed = Some(PlacedDevice {
                component_id: device.component_id,
                name: device.name.clone(),
                kind,
                x,
                y,
                row,
                column: (site - min_site) as usize,
                site,
            });
            break;
        }
        if let Some(placed) = placed {
            placements.push(placed);
        }
    }
    RowLegalization {
        devices: placements,
        retries,
        reserved_shapes,
    }
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
    let hierarchy_groups = devices
        .iter()
        .filter_map(|device| device.physical_group.as_deref())
        .collect::<HashSet<_>>()
        .len();
    let mut strategies = vec![
        PlacementStrategy::Topology,
        PlacementStrategy::Diffusion,
        PlacementStrategy::Congestion,
    ];
    if hierarchy_groups >= 2 {
        strategies.insert(0, PlacementStrategy::Hierarchy);
    }
    let mut candidates = Vec::new();
    for (id, strategy) in strategies.into_iter().enumerate() {
        let mut placements = Vec::with_capacity(devices.len());
        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        let mut occupancy_retries = 0;
        let mut reserved_device_shapes = 0;
        for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
            let legalized = legalize_row(
                &strategy_order(devices, kind, strategy, &power_nets),
                kind,
                selected_floorplan.columns,
                selected_floorplan.width_um,
                selected_floorplan.height_um,
                planning.placement_site_width_um,
                planning.row_height_um,
                device_pitch,
                technology,
                &mut canvas,
            );
            occupancy_retries += legalized.retries;
            reserved_device_shapes += legalized.reserved_shapes;
            placements.extend(legalized.devices);
        }
        let legal = placements.len() == devices.len();
        let (wire_length, peak_utilization, sharing) =
            candidate_metrics(&placements, devices, nets, planning);
        let regions = block_regions(&placements, devices, device_pitch, planning.row_height_um);
        candidates.push(PlacementCandidate {
            id,
            strategy,
            devices: placements,
            block_regions: regions,
            legal,
            estimated_wire_length_um: wire_length,
            peak_bin_utilization: peak_utilization,
            diffusion_sharing_pairs: sharing,
            occupancy_retries,
            reserved_device_shapes,
            score: if legal {
                wire_length + peak_utilization * 25.0 - sharing as f64 * device_pitch * 0.35
            } else {
                1e12
            },
        });
    }
    let selected_candidate = candidates
        .iter()
        .filter(|candidate| {
            hierarchy_groups < 2 || candidate.strategy == PlacementStrategy::Hierarchy
        })
        .filter(|candidate| candidate.legal)
        .min_by(|left, right| left.score.total_cmp(&right.score))
        .or_else(|| {
            candidates
                .iter()
                .min_by(|left, right| left.score.total_cmp(&right.score))
        })
        .map(|candidate| candidate.id)
        .unwrap_or(0);
    PhysicalPlacementReport {
        device_pitch_um: device_pitch,
        candidates,
        selected_candidate,
    }
}

pub fn add_timing_candidate(
    report: &mut PhysicalPlacementReport,
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    planning: &PhysicalPlanningReport,
    technology: &Technology,
    critical_devices: &[Uuid],
) {
    if critical_devices.is_empty() {
        return;
    }
    let selected_floorplan = &planning.candidates[planning.selected_candidate];
    let power_nets = nets
        .iter()
        .filter(|net| matches!(net.role, NetRole::Power | NetRole::Ground))
        .map(|net| net.id)
        .collect::<HashSet<_>>();
    let critical_rank = critical_devices
        .iter()
        .enumerate()
        .map(|(rank, id)| (*id, rank))
        .collect::<HashMap<_, _>>();
    let mut placements = Vec::with_capacity(devices.len());
    let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
    let mut occupancy_retries = 0;
    let mut reserved_device_shapes = 0;
    for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
        let baseline = strategy_order(devices, kind, PlacementStrategy::Timing, &power_nets);
        let mut critical = baseline
            .iter()
            .copied()
            .filter(|device| critical_rank.contains_key(&device.component_id))
            .collect::<Vec<_>>();
        critical.sort_by_key(|device| critical_rank[&device.component_id]);
        let remaining = baseline
            .into_iter()
            .filter(|device| !critical_rank.contains_key(&device.component_id))
            .collect::<Vec<_>>();
        let mut slots = vec![None; critical.len() + remaining.len()];
        let last_slot = slots.len().saturating_sub(1);
        for (index, device) in critical.iter().enumerate() {
            let slot = if critical.len() == 1 {
                last_slot / 2
            } else {
                (index * last_slot + (critical.len() - 1) / 2) / (critical.len() - 1)
            };
            slots[slot] = Some(*device);
        }
        let mut remaining = remaining.into_iter();
        for slot in &mut slots {
            if slot.is_none() {
                *slot = remaining.next();
            }
        }
        let ordered = slots.into_iter().flatten().collect::<Vec<_>>();
        let legalized = legalize_row(
            &ordered,
            kind,
            selected_floorplan.columns,
            selected_floorplan.width_um,
            selected_floorplan.height_um,
            planning.placement_site_width_um,
            planning.row_height_um,
            report.device_pitch_um,
            technology,
            &mut canvas,
        );
        occupancy_retries += legalized.retries;
        reserved_device_shapes += legalized.reserved_shapes;
        placements.extend(legalized.devices);
    }
    let legal = placements.len() == devices.len();
    let (wire_length, peak_utilization, sharing) =
        candidate_metrics(&placements, devices, nets, planning);
    let regions = block_regions(
        &placements,
        devices,
        report.device_pitch_um,
        planning.row_height_um,
    );
    let id = report.candidates.len();
    report.candidates.push(PlacementCandidate {
        id,
        strategy: PlacementStrategy::Timing,
        devices: placements,
        block_regions: regions,
        legal,
        estimated_wire_length_um: wire_length,
        peak_bin_utilization: peak_utilization,
        diffusion_sharing_pairs: sharing,
        occupancy_retries,
        reserved_device_shapes,
        score: if legal {
            wire_length + peak_utilization * 25.0 - sharing as f64 * report.device_pitch_um * 0.35
        } else {
            1e12
        },
    });
    report.selected_candidate = id;
}

#[cfg(test)]
mod tests {
    use super::legalize_row;
    use crate::{
        physical_canvas::PhysicalCanvas,
        physical_layout::{DeviceKind, PhysicalDevice},
        technology::Technology,
    };
    use uuid::Uuid;

    fn long_gate(name: &str, gate_net: usize) -> PhysicalDevice {
        PhysicalDevice {
            component_id: Uuid::new_v4(),
            name: name.into(),
            physical_group: None,
            kind: DeviceKind::Nmos,
            gate_net,
            drain_net: gate_net + 10,
            source_net: gate_net + 20,
            width_um: 1.0,
            length_um: 10.0,
        }
    }

    #[test]
    fn device_legalization_retries_the_nearest_available_site() {
        let technology = Technology::default();
        let left = long_gate("M1", 1);
        let right = long_gate("M2", 2);
        let ordered = vec![&left, &right];
        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        let result = legalize_row(
            &ordered,
            DeviceKind::Nmos,
            2,
            8.0,
            6.0,
            0.5,
            1.7,
            2.0,
            &technology,
            &mut canvas,
        );

        assert_eq!(result.devices.len(), 2);
        assert!(result.retries > 0);
        assert_eq!(result.reserved_shapes, 8);
        assert!((result.devices[1].x - result.devices[0].x).abs() >= 2.4 - 1e-9);
        assert_eq!(canvas.occupied_count(), 8);
    }
}
