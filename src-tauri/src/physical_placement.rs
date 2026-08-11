use crate::{
    physical_canvas::{
        device_active_width, device_diffusion_landing_size, device_footprint,
        device_terminal_offset, reserve_device_footprint, CanvasCollision, ObstructionType,
        PhysicalCanvas,
    },
    physical_layout::{
        topology_owned_device_regions, well_stripe_geometry, DeviceKind, NetRole, PhysicalDevice,
        PhysicalLayer, PhysicalNet, PhysicalShape, PhysicalShapePurpose,
    },
    physical_planning::PhysicalPlanningReport,
    technology::Technology,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementStrategy {
    Hierarchy,
    HierarchyTopology,
    LeafTopology,
    LeafGeometryTopology,
    Topology,
    GeometryTopology,
    GateTopology,
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
    #[serde(default)]
    pub gate_strap_pairs: usize,
    pub occupancy_retries: usize,
    pub reserved_device_shapes: usize,
    #[serde(default)]
    pub standard_cell_instances: usize,
    #[serde(default)]
    pub topology_compacted: bool,
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
    #[serde(default)]
    pub standard_cell_candidate: Option<PlacementCandidate>,
}

impl Default for PhysicalPlacementReport {
    fn default() -> Self {
        Self {
            device_pitch_um: 2.0,
            candidates: Vec::new(),
            selected_candidate: 0,
            standard_cell_candidate: None,
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
        let next_index = if !ordered.is_empty() {
            remaining
                .iter()
                .enumerate()
                .max_by(|(_, left), (_, right)| {
                    let frontier_score = |candidate: &&PhysicalDevice| {
                        ordered
                            .iter()
                            .enumerate()
                            .map(|(index, placed)| {
                                (index + 1) * shared_weight(placed, candidate, power_nets)
                            })
                            .sum::<usize>()
                    };
                    frontier_score(left)
                        .cmp(&frontier_score(right))
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

fn geometry_adjacency_weight(
    left: &PhysicalDevice,
    right: &PhysicalDevice,
    power_nets: &HashSet<usize>,
) -> usize {
    let shared_diffusion = [left.drain_net, left.source_net]
        .into_iter()
        .filter(|net| [right.drain_net, right.source_net].contains(net))
        .map(|net| if power_nets.contains(&net) { 10 } else { 24 })
        .sum::<usize>();
    let shared_gate = usize::from(left.gate_net == right.gate_net) * 12;
    shared_diffusion + shared_gate + shared_weight(left, right, power_nets)
}

fn frontier_parent_index(placed: usize, columns: usize) -> Option<usize> {
    let columns = columns.max(1);
    let row_start = placed / columns * columns;
    let local = placed % columns;
    match local {
        0 => None,
        1 | 2 => Some(row_start),
        _ => Some(placed - 2),
    }
}

/// Order devices for the actual center-out row geometry. The next device is
/// scored most strongly against the already occupied site it will physically
/// neighbor, rather than against an abstract left-to-right sequence.
fn geometry_topology_order<'a>(
    devices: Vec<&'a PhysicalDevice>,
    power_nets: &HashSet<usize>,
    columns: usize,
) -> Vec<&'a PhysicalDevice> {
    let mut remaining = devices;
    let mut ordered = Vec::<&PhysicalDevice>::new();
    while !remaining.is_empty() {
        let parent = frontier_parent_index(ordered.len(), columns)
            .and_then(|index| ordered.get(index))
            .copied();
        let next_index = remaining
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| {
                let score = |candidate: &&PhysicalDevice| {
                    let local = parent
                        .map(|placed| geometry_adjacency_weight(placed, candidate, power_nets))
                        .unwrap_or(0);
                    let placed = ordered
                        .iter()
                        .map(|placed| geometry_adjacency_weight(placed, candidate, power_nets))
                        .sum::<usize>();
                    let frontier = remaining
                        .iter()
                        .filter(|other| other.component_id != candidate.component_id)
                        .map(|other| geometry_adjacency_weight(candidate, other, power_nets))
                        .sum::<usize>();
                    (local, placed, frontier)
                };
                score(left)
                    .cmp(&score(right))
                    .then_with(|| right.name.cmp(&left.name))
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        ordered.push(remaining.remove(next_index));
    }
    ordered
}

fn group_shared_weight(
    left: &[&PhysicalDevice],
    right: &[&PhysicalDevice],
    power_nets: &HashSet<usize>,
) -> usize {
    left.iter()
        .flat_map(|left| {
            right
                .iter()
                .map(move |right| shared_weight(left, right, power_nets))
        })
        .sum()
}

fn topology_group_order<'a>(
    mut groups: Vec<(String, Vec<&'a PhysicalDevice>)>,
    power_nets: &HashSet<usize>,
) -> Vec<(String, Vec<&'a PhysicalDevice>)> {
    let mut ordered: Vec<(String, Vec<&PhysicalDevice>)> = Vec::new();
    while !groups.is_empty() {
        let next_index = groups
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| {
                let score = |candidate: &(String, Vec<&PhysicalDevice>)| {
                    if ordered.is_empty() {
                        groups
                            .iter()
                            .filter(|other| other.0 != candidate.0)
                            .map(|other| group_shared_weight(&candidate.1, &other.1, power_nets))
                            .sum::<usize>()
                    } else {
                        ordered
                            .iter()
                            .enumerate()
                            .map(|(index, placed)| {
                                (index + 1)
                                    * group_shared_weight(&candidate.1, &placed.1, power_nets)
                            })
                            .sum::<usize>()
                    }
                };
                score(left)
                    .cmp(&score(right))
                    .then_with(|| left.1.len().cmp(&right.1.len()))
                    .then_with(|| right.0.cmp(&left.0))
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        ordered.push(groups.remove(next_index));
    }
    ordered
}

fn geometry_group_weight(
    left: &[&PhysicalDevice],
    right: &[&PhysicalDevice],
    power_nets: &HashSet<usize>,
) -> usize {
    left.iter()
        .flat_map(|left| {
            right
                .iter()
                .map(move |right| geometry_adjacency_weight(left, right, power_nets))
        })
        .sum()
}

fn geometry_topology_group_order<'a>(
    mut groups: Vec<(String, Vec<&'a PhysicalDevice>)>,
    power_nets: &HashSet<usize>,
) -> Vec<(String, Vec<&'a PhysicalDevice>)> {
    let columns = groups.len().max(1);
    let mut ordered = Vec::<(String, Vec<&PhysicalDevice>)>::new();
    while !groups.is_empty() {
        let parent =
            frontier_parent_index(ordered.len(), columns).and_then(|index| ordered.get(index));
        let next_index = groups
            .iter()
            .enumerate()
            .max_by(|(_, left), (_, right)| {
                let score = |candidate: &(String, Vec<&PhysicalDevice>)| {
                    let local = parent
                        .map(|placed| geometry_group_weight(&placed.1, &candidate.1, power_nets))
                        .unwrap_or(0);
                    let placed = ordered
                        .iter()
                        .map(|placed| geometry_group_weight(&placed.1, &candidate.1, power_nets))
                        .sum::<usize>();
                    let frontier = groups
                        .iter()
                        .filter(|other| other.0 != candidate.0)
                        .map(|other| geometry_group_weight(&candidate.1, &other.1, power_nets))
                        .sum::<usize>();
                    (local, placed, frontier)
                };
                score(left)
                    .cmp(&score(right))
                    .then_with(|| left.1.len().cmp(&right.1.len()))
                    .then_with(|| right.0.cmp(&left.0))
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        ordered.push(groups.remove(next_index));
    }
    ordered
}

fn gate_topology_order<'a>(
    devices: Vec<&'a PhysicalDevice>,
    power_nets: &HashSet<usize>,
) -> Vec<&'a PhysicalDevice> {
    let mut groups = HashMap::<usize, Vec<&PhysicalDevice>>::new();
    for device in devices {
        groups.entry(device.gate_net).or_default().push(device);
    }
    let groups = groups
        .into_iter()
        .map(|(gate, devices)| (format!("{gate:020}"), devices))
        .collect::<Vec<_>>();
    topology_group_order(groups, power_nets)
        .into_iter()
        .flat_map(|(_, devices)| topology_order(devices, power_nets))
        .collect()
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
        PlacementStrategy::LeafGeometryTopology => {
            // Rank complete logical leaves once, then project that common
            // order into both polarity rows. This keeps the PMOS/NMOS halves
            // of a gate vertically aligned instead of independently
            // permuting them from polarity-specific connectivity.
            let mut groups = HashMap::<String, Vec<&PhysicalDevice>>::new();
            for device in devices {
                let group = device
                    .standard_cell_group
                    .clone()
                    .unwrap_or_else(|| format!("~{}", device.name));
                groups.entry(group).or_default().push(device);
            }
            geometry_topology_group_order(groups.into_iter().collect(), power_nets)
                .into_iter()
                .flat_map(|(_, devices)| {
                    let row = devices
                        .into_iter()
                        .filter(|device| device.kind == kind)
                        .collect::<Vec<_>>();
                    let columns = row.len().max(1);
                    geometry_topology_order(row, power_nets, columns)
                })
                .collect()
        }
        PlacementStrategy::HierarchyTopology | PlacementStrategy::LeafTopology => {
            let mut groups = HashMap::<String, Vec<&PhysicalDevice>>::new();
            for device in row {
                let group = device
                    .standard_cell_group
                    .clone()
                    .unwrap_or_else(|| format!("~{}", device.name));
                groups.entry(group).or_default().push(device);
            }
            topology_group_order(groups.into_iter().collect(), power_nets)
                .into_iter()
                .flat_map(|(_, devices)| topology_order(devices, power_nets))
                .collect()
        }
        PlacementStrategy::Topology => topology_order(row, power_nets),
        PlacementStrategy::GeometryTopology => {
            unreachable!("geometry topology requires the selected row capacity")
        }
        PlacementStrategy::GateTopology => gate_topology_order(row, power_nets),
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
    standard_cells: bool,
) -> Vec<PlacedBlockRegion> {
    let groups = devices
        .iter()
        .filter_map(|device| {
            (if standard_cells {
                &device.standard_cell_group
            } else {
                &device.physical_group
            })
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

fn canonicalize_identical_blocks(
    placements: &mut [PlacedDevice],
    devices: &[PhysicalDevice],
    site_width: f64,
    row_height: f64,
    device_pitch: f64,
    width: f64,
    height: f64,
    technology: &Technology,
    power_nets: &HashSet<usize>,
) -> Option<usize> {
    let device_by_id = devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let mut members = HashMap::<String, Vec<usize>>::new();
    for (index, placed) in placements.iter().enumerate() {
        if let Some(group) = device_by_id[&placed.component_id]
            .standard_cell_group
            .as_ref()
        {
            members.entry(group.clone()).or_default().push(index);
        }
    }
    let mut by_signature = HashMap::<String, Vec<String>>::new();
    let mut macro_canvas = PhysicalCanvas::new(&technology.physical_rules);
    let mut macro_devices = HashSet::new();
    let mut macro_instances = 0usize;
    for (group, indices) in &members {
        let mut signature = indices
            .iter()
            .map(|index| {
                let device = device_by_id[&placements[*index].component_id];
                let local_name = device
                    .name
                    .strip_prefix(&format!("{group}·"))
                    .unwrap_or(device.name.as_str());
                format!("{:?}:{local_name}", device.kind)
            })
            .collect::<Vec<_>>();
        signature.sort();
        by_signature
            .entry(signature.join("|"))
            .or_default()
            .push(group.clone());
    }
    for groups in by_signature.values_mut() {
        if groups.len() < 2 {
            continue;
        }
        let ordered_groups = topology_group_order(
            groups
                .iter()
                .map(|group| {
                    (
                        group.clone(),
                        members[group]
                            .iter()
                            .map(|index| device_by_id[&placements[*index].component_id])
                            .collect(),
                    )
                })
                .collect(),
            power_nets,
        )
        .into_iter()
        .map(|(group, _)| group)
        .collect::<Vec<_>>();
        let canonical_group = &ordered_groups[0];
        let canonical_indices = &members[canonical_group];
        let canonical_center_x = canonical_indices
            .iter()
            .map(|index| placements[*index].x)
            .sum::<f64>()
            / canonical_indices.len() as f64;
        let canonical_center_y = canonical_indices
            .iter()
            .map(|index| placements[*index].y)
            .sum::<f64>()
            / canonical_indices.len() as f64;
        let canonical_min_row = canonical_indices
            .iter()
            .map(|index| placements[*index].row)
            .min()
            .unwrap_or(0);
        let canonical = canonical_indices
            .iter()
            .map(|index| {
                let placed = &placements[*index];
                let device = device_by_id[&placed.component_id];
                let local_name = device
                    .name
                    .strip_prefix(&format!("{canonical_group}·"))
                    .unwrap_or(device.name.as_str())
                    .to_string();
                (
                    (local_name, device.kind),
                    (
                        placed.x - canonical_center_x,
                        placed.y - canonical_center_y,
                        placed.row - canonical_min_row,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
        let min_offset_x = canonical
            .values()
            .map(|(x, _, _)| *x)
            .fold(f64::INFINITY, f64::min);
        let max_offset_x = canonical
            .values()
            .map(|(x, _, _)| *x)
            .fold(f64::NEG_INFINITY, f64::max);
        let min_offset_y = canonical
            .values()
            .map(|(_, y, _)| *y)
            .fold(f64::INFINITY, f64::min);
        let max_offset_y = canonical
            .values()
            .map(|(_, y, _)| *y)
            .fold(f64::NEG_INFINITY, f64::max);
        let macro_width = max_offset_x - min_offset_x + device_pitch;
        let (_, stripe_height) =
            well_stripe_geometry(height, row_height, &technology.physical_rules);
        let stripe_pair_height = 2.0 * stripe_height;
        let raw_macro_height = max_offset_y - min_offset_y + row_height;
        let macro_height = (raw_macro_height / stripe_pair_height).ceil() * stripe_pair_height;
        let columns = ((ordered_groups.len() as f64 * macro_height / macro_width)
            .sqrt()
            .ceil() as usize)
            .clamp(1, ordered_groups.len());
        let rows = ordered_groups.len().div_ceil(columns);
        if columns as f64 * macro_width > width + 1e-9 || rows as f64 * macro_height > height + 1e-9
        {
            return None;
        }
        for (group_index, group) in ordered_groups.iter().enumerate() {
            let indices = &members[group];
            let column = group_index % columns;
            let macro_row = group_index / columns;
            let well_edge_inset = 1.25
                + technology.physical_rules.well_enclosure_um
                + technology.physical_rules.manufacturing_grid_um;
            let center_x =
                -width / 2.0 - min_offset_x + well_edge_inset + column as f64 * macro_width;
            // Move copied cells only by complete N/P stripe pairs so every
            // transistor retains the well polarity of the canonical template.
            let center_y = canonical_center_y + macro_row as f64 * macro_height;
            for index in indices {
                let device = device_by_id[&placements[*index].component_id];
                let local_name = device
                    .name
                    .strip_prefix(&format!("{group}·"))
                    .unwrap_or(device.name.as_str());
                let Some((offset_x, offset_y, row_offset)) =
                    canonical.get(&(local_name.to_string(), device.kind))
                else {
                    continue;
                };
                let placed = &mut placements[*index];
                placed.x = center_x + offset_x;
                placed.y = center_y + offset_y;
                placed.site = (placed.x / site_width).round() as i64;
                placed.row = macro_row * 1_000 + row_offset;
                placed.column =
                    (placed.site + ((width / 2.0) / site_width).ceil() as i64).max(0) as usize;
            }
            let mut entries = vec![(
                PhysicalShape {
                    layer: PhysicalLayer::Substrate,
                    x: center_x,
                    y: center_y,
                    width: macro_width,
                    height: macro_height,
                    component_id: None,
                    net: None,
                    purpose: PhysicalShapePurpose::Fabric,
                },
                format!("standard-cell:{group}"),
                ObstructionType::Device,
            )];
            for index in indices {
                let placed = &placements[*index];
                let device = device_by_id[&placed.component_id];
                macro_devices.insert(placed.component_id);
                entries.extend(
                    device_footprint(device, placed.x, placed.y, &technology.physical_rules)
                        .into_iter()
                        .map(|(shape, obstruction)| {
                            (shape, format!("{group}:{}", device.name), obstruction)
                        }),
                );
            }
            if macro_canvas.commit_batch(&entries).is_err() {
                return None;
            }
            macro_instances += 1;
        }
    }
    placements
        .iter()
        .all(|placed| {
            let device = device_by_id[&placed.component_id];
            let footprint =
                device_footprint(device, placed.x, placed.y, &technology.physical_rules);
            let inside = footprint.iter().all(|(shape, _)| {
                shape.x - shape.width / 2.0 >= -width / 2.0 - 1e-9
                    && shape.x + shape.width / 2.0 <= width / 2.0 + 1e-9
                    && shape.y - shape.height / 2.0 >= -height / 2.0 - 1e-9
                    && shape.y + shape.height / 2.0 <= height / 2.0 + 1e-9
            });
            inside
                && (macro_devices.contains(&placed.component_id)
                    || reserve_device_footprint(
                        &mut macro_canvas,
                        device,
                        placed.x,
                        placed.y,
                        &technology.physical_rules,
                    )
                    .is_ok())
        })
        .then_some(macro_instances)
}

struct RowLegalization {
    devices: Vec<PlacedDevice>,
    retries: usize,
    reserved_shapes: usize,
}

fn legalize_row(
    ordered: &[&PhysicalDevice],
    kind: DeviceKind,
    preserve_leaf_groups: bool,
    center_leaf_groups: bool,
    columns: usize,
    width: f64,
    height: f64,
    site_width: f64,
    row_height: f64,
    device_pitch: f64,
    technology: &Technology,
    canvas: &mut PhysicalCanvas,
) -> RowLegalization {
    let edge_margin = 1.25
        + technology.physical_rules.well_enclosure_um
        + technology.physical_rules.manufacturing_grid_um;
    let usable_columns = (((width - 2.0 * edge_margin).max(0.0) / device_pitch).floor() as usize)
        .saturating_add(1)
        .max(1);
    let columns = columns.min(usable_columns);
    let (stripe_count, stripe_height) =
        well_stripe_geometry(height, row_height, &technology.physical_rules);
    let min_site = ((-width / 2.0) / site_width).ceil() as i64;
    let max_site = ((width / 2.0) / site_width).floor() as i64;
    let mut placements = Vec::with_capacity(ordered.len());
    let mut retries = 0;
    let mut reserved_shapes = 0;
    let mut scheduled = Vec::with_capacity(ordered.len());
    let total_slots = if preserve_leaf_groups && center_leaf_groups {
        let mut groups = Vec::<Vec<&PhysicalDevice>>::new();
        let mut index = 0usize;
        while index < ordered.len() {
            let group = ordered[index].standard_cell_group.as_deref();
            let group_end = if group.is_some() {
                ordered[index + 1..]
                    .iter()
                    .position(|device| device.standard_cell_group.as_deref() != group)
                    .map(|offset| index + 1 + offset)
                    .unwrap_or(ordered.len())
            } else {
                index + 1
            };
            groups.push(ordered[index..group_end].to_vec());
            index = group_end;
        }
        let mut rows = Vec::<Vec<Vec<&PhysicalDevice>>>::new();
        for group in groups {
            if rows
                .last()
                .is_none_or(|row| row.iter().map(Vec::len).sum::<usize>() + group.len() > columns)
            {
                rows.push(Vec::new());
            }
            rows.last_mut().expect("row was just created").push(group);
        }
        let row_count = rows.len();
        let last_row_items = rows
            .last()
            .map(|row| row.iter().map(Vec::len).sum())
            .unwrap_or(0);
        for (row_index, groups) in rows.into_iter().enumerate() {
            let mut left = Vec::new();
            let mut center = None;
            let mut right = Vec::new();
            for (index, group) in groups.into_iter().enumerate() {
                if index == 0 {
                    center = Some(group);
                } else if index % 2 == 1 {
                    right.push(group);
                } else {
                    left.push(group);
                }
            }
            left.reverse();
            let physical_groups = left
                .into_iter()
                .chain(center)
                .chain(right)
                .collect::<Vec<_>>();
            let mut local_slot = 0usize;
            for group in physical_groups {
                for device in group {
                    scheduled.push((row_index * columns + local_slot, device));
                    local_slot += 1;
                }
            }
        }
        row_count.saturating_sub(1) * columns + last_row_items
    } else {
        let mut slot = 0usize;
        let mut index = 0usize;
        while index < ordered.len() {
            let group_end = if preserve_leaf_groups {
                let group = ordered[index].standard_cell_group.as_deref();
                if group.is_some() {
                    ordered[index + 1..]
                        .iter()
                        .position(|device| device.standard_cell_group.as_deref() != group)
                        .map(|offset| index + 1 + offset)
                        .unwrap_or(ordered.len())
                } else {
                    index + 1
                }
            } else {
                index + 1
            };
            let group_len = group_end - index;
            let remaining = columns - slot % columns;
            if preserve_leaf_groups && group_len <= columns && group_len > remaining {
                slot += remaining;
            }
            for device in &ordered[index..group_end] {
                scheduled.push((slot, *device));
                slot += 1;
            }
            index = group_end;
        }
        slot
    };
    for (slot, device) in scheduled {
        let row = slot / columns;
        let logical_column = slot % columns;
        let items_in_row = (total_slots - row * columns).min(columns);
        // The topology order begins with the most connected (and generally
        // hardest-to-route) device. Seed that device near the center of the
        // compatible well stripe, then expand to its nearest legal neighbors
        // on alternating sides. A well is an available placement region, not
        // a left-to-right queue.
        let center_column = (items_in_row - 1) as isize / 2;
        // Center-out spreading is useful for individually ranked devices, but
        // applying it to the members of a leaf group tears consecutive
        // transistors across opposite sides of the row. Keep leaf members
        // contiguous; topology still determines the order of the groups.
        let offset = if preserve_leaf_groups {
            logical_column as isize - center_column
        } else if logical_column == 0 {
            0
        } else if logical_column % 2 == 1 {
            (logical_column as isize + 1) / 2
        } else {
            -(logical_column as isize / 2)
        };
        let row_span = (items_in_row.saturating_sub(1)) as f64 * device_pitch;
        let row_left = -row_span / 2.0;
        let unsnapped_x = (row_left + (center_column + offset) as f64 * device_pitch)
            .clamp(-width / 2.0 + edge_margin, width / 2.0 - edge_margin);
        let desired_site = (unsnapped_x / site_width).round() as i64;
        let stripe = if kind == DeviceKind::Pmos {
            row * 2
        } else {
            row * 2 + 1
        };
        if stripe >= stripe_count {
            continue;
        }
        let y = ((-height / 2.0 + (stripe as f64 + 0.5) * stripe_height)
            / technology.physical_rules.manufacturing_grid_um)
            .round()
            * technology.physical_rules.manufacturing_grid_um;
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
                let well_guard =
                    if matches!(shape.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff) {
                        technology.physical_rules.well_enclosure_um
                    } else {
                        0.0
                    };
                shape.x - shape.width / 2.0 >= -width / 2.0 + well_guard - 1e-9
                    && shape.x + shape.width / 2.0 <= width / 2.0 - well_guard + 1e-9
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

fn oriented_row_devices(row: &[&PhysicalDevice]) -> Vec<PhysicalDevice> {
    if row.is_empty() {
        return Vec::new();
    }
    let mut scores = vec![[(i32::MIN, i32::MIN); 2]; row.len()];
    let mut parents = vec![[0usize; 2]; row.len()];
    scores[0][0] = (0, 0);
    scores[0][1] = (0, -1);
    for index in 1..row.len() {
        for right_orientation in 0..2 {
            for left_orientation in 0..2 {
                let prior = scores[index - 1][left_orientation];
                if prior.0 == i32::MIN {
                    continue;
                }
                let left = row[index - 1];
                let right = row[index];
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
    let mut orientations = vec![0usize; row.len()];
    orientations[row.len() - 1] = usize::from(scores[row.len() - 1][1] > scores[row.len() - 1][0]);
    for index in (1..row.len()).rev() {
        orientations[index - 1] = parents[index][orientations[index]];
    }
    row.iter()
        .zip(orientations)
        .map(|(device, orientation)| {
            let mut device = (*device).clone();
            if orientation == 1 {
                std::mem::swap(&mut device.drain_net, &mut device.source_net);
            }
            device
        })
        .collect()
}

fn oriented_device_map(
    placements: &[PlacedDevice],
    devices: &[PhysicalDevice],
    grid: f64,
) -> HashMap<Uuid, PhysicalDevice> {
    let by_id = devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let mut rows = HashMap::<(DeviceKind, i64), Vec<usize>>::new();
    for (index, placed) in placements.iter().enumerate() {
        rows.entry((placed.kind, (placed.y / grid).round() as i64))
            .or_default()
            .push(index);
    }
    let mut oriented = HashMap::with_capacity(placements.len());
    for indices in rows.values_mut() {
        indices.sort_by(|left, right| {
            placements[*left]
                .x
                .total_cmp(&placements[*right].x)
                .then_with(|| {
                    placements[*left]
                        .component_id
                        .cmp(&placements[*right].component_id)
                })
        });
        let row = indices
            .iter()
            .map(|index| by_id[&placements[*index].component_id])
            .collect::<Vec<_>>();
        for device in oriented_row_devices(&row) {
            oriented.insert(device.component_id, device);
        }
    }
    oriented
}

fn compact_oriented_row_positions(
    oriented: &[PhysicalDevice],
    rules: &crate::technology::PhysicalRuleDeck,
) -> Vec<f64> {
    if oriented.is_empty() {
        return Vec::new();
    }
    let metal1 = rules.layer_overrides.get("metal1").unwrap_or(&rules.metal);
    let landing = device_diffusion_landing_size(rules);
    let mut positions = Vec::with_capacity(oriented.len());
    positions.push(0.0);
    for index in 1..oriented.len() {
        let left = &oriented[index - 1];
        let right = &oriented[index];
        let poly_span = |device: &PhysicalDevice| {
            (rules.poly.min_width_um.max(device.length_um) + rules.manufacturing_grid_um * 4.0).max(
                rules.contact.size_um
                    + rules.contact.enclosure_um * 2.0
                    + rules.manufacturing_grid_um * 8.0,
            )
        };
        let poly_pitch = (poly_span(left) + poly_span(right)) / 2.0 + rules.poly.min_spacing_um;
        let same_cell = left.standard_cell_group.is_some()
            && left.standard_cell_group == right.standard_cell_group;
        let pitch = if same_cell && left.source_net == right.drain_net {
            (device_terminal_offset(left, rules) + device_terminal_offset(right, rules))
                .max(poly_pitch)
        } else {
            let diffusion_pitch =
                (device_active_width(left, rules) + device_active_width(right, rules)) / 2.0
                    + rules.diffusion.min_spacing_um;
            let landing_pitch = device_terminal_offset(left, rules)
                + device_terminal_offset(right, rules)
                + landing
                + metal1.min_spacing_um;
            diffusion_pitch.max(landing_pitch).max(poly_pitch)
        };
        let next = positions[index - 1] + pitch;
        positions.push((next / rules.manufacturing_grid_um).round() * rules.manufacturing_grid_um);
    }
    let center = (positions[0] + positions[positions.len() - 1]) / 2.0;
    for position in &mut positions {
        *position = ((*position - center) / rules.manufacturing_grid_um).round()
            * rules.manufacturing_grid_um;
    }
    positions
}

#[cfg(test)]
fn compact_row_positions(
    row: &[&PhysicalDevice],
    rules: &crate::technology::PhysicalRuleDeck,
) -> Vec<f64> {
    let oriented = oriented_row_devices(row);
    compact_oriented_row_positions(&oriented, rules)
}

pub(crate) fn compact_device_entries(
    placements: &[PlacedDevice],
    devices: &[PhysicalDevice],
    rules: &crate::technology::PhysicalRuleDeck,
) -> Vec<(PhysicalShape, String, ObstructionType)> {
    let by_id = devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let oriented_by_id = oriented_device_map(placements, devices, rules.manufacturing_grid_um);
    let mut entries = Vec::new();
    let mut contacts = Vec::<PhysicalShape>::new();
    for placed in placements {
        let device = oriented_by_id
            .get(&placed.component_id)
            .unwrap_or(by_id[&placed.component_id]);
        let footprint = device_footprint(device, placed.x, placed.y, rules);
        for (shape, obstruction) in footprint {
            if shape.purpose == PhysicalShapePurpose::Active {
                continue;
            }
            if shape.layer == PhysicalLayer::Contact
                && contacts.iter().any(|existing| {
                    existing.net == shape.net
                        && (existing.x - shape.x).abs() <= rules.manufacturing_grid_um / 2.0
                        && (existing.y - shape.y).abs() <= rules.manufacturing_grid_um / 2.0
                        && (existing.width - shape.width).abs() <= 1e-9
                        && (existing.height - shape.height).abs() <= 1e-9
                })
            {
                continue;
            }
            if shape.layer == PhysicalLayer::Contact {
                contacts.push(shape.clone());
            }
            entries.push((
                shape,
                format!("compact-device:{}", device.name),
                obstruction,
            ));
        }
    }
    let (topology_active, topology_poly) =
        topology_owned_device_regions(devices, placements, rules);
    entries.extend(
        topology_active
            .into_iter()
            .enumerate()
            .map(|(index, shape)| {
                (
                    shape,
                    format!("compact-topology-active:{index}"),
                    ObstructionType::Diffusion,
                )
            }),
    );
    entries.extend(topology_poly.into_iter().enumerate().map(|(index, shape)| {
        (
            shape,
            format!("compact-topology-poly:{index}"),
            ObstructionType::Poly,
        )
    }));
    entries
}

pub(crate) fn commit_compact_device_entries(
    canvas: &mut PhysicalCanvas,
    entries: &[(PhysicalShape, String, ObstructionType)],
) -> Result<Vec<u64>, Vec<CanvasCollision>> {
    let mut candidate = canvas.clone();
    let (topology_poly, remaining): (Vec<_>, Vec<_>) = entries
        .iter()
        .cloned()
        .partition(|(_, owner, _)| owner.starts_with("compact-topology-poly:"));
    let (topology_active, ordinary): (Vec<_>, Vec<_>) =
        remaining.into_iter().partition(|(shape, owner, _)| {
            owner.starts_with("compact-topology-active:") && shape.component_id.is_none()
        });
    let mut ids = candidate.commit_batch(&ordinary)?;
    let mut unseen = (0..topology_active.len()).collect::<HashSet<_>>();
    let mut active_group = 0usize;
    while let Some(seed) = unseen.iter().copied().min() {
        unseen.remove(&seed);
        let mut pending = vec![seed];
        let mut group = vec![topology_active[seed].0.clone()];
        while let Some(current) = pending.pop() {
            let current_shape = &topology_active[current].0;
            let neighbors = unseen
                .iter()
                .copied()
                .filter(|candidate_index| {
                    let other = &topology_active[*candidate_index].0;
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
                group.push(topology_active[neighbor].0.clone());
            }
        }
        ids.extend(candidate.commit_topology_active_geometry(
            &group,
            format!("compact-topology-active-island:{active_group}"),
        )?);
        active_group += 1;
    }
    let mut by_net = BTreeMap::<usize, Vec<PhysicalShape>>::new();
    for (shape, _, _) in topology_poly {
        let Some(net) = shape.net else {
            return Err(Vec::new());
        };
        by_net.entry(net).or_default().push(shape);
    }
    for (net, shapes) in by_net {
        ids.extend(
            candidate.commit_topology_poly_geometry(
                &shapes,
                format!("compact-topology-poly-net:{net}"),
            )?,
        );
    }
    *canvas = candidate;
    Ok(ids)
}

fn compact_topology_candidate(
    placements: &mut [PlacedDevice],
    devices: &[PhysicalDevice],
    width: f64,
    technology: &Technology,
) -> bool {
    let rules = &technology.physical_rules;
    let by_id = devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let mut rows = HashMap::<(DeviceKind, i64), Vec<usize>>::new();
    for (index, placed) in placements.iter().enumerate() {
        rows.entry((
            placed.kind,
            (placed.y / rules.manufacturing_grid_um).round() as i64,
        ))
        .or_default()
        .push(index);
    }
    for indices in rows.values_mut() {
        indices.sort_by(|left, right| {
            placements[*left]
                .x
                .total_cmp(&placements[*right].x)
                .then_with(|| {
                    placements[*left]
                        .component_id
                        .cmp(&placements[*right].component_id)
                })
        });
    }

    #[derive(Clone)]
    struct CompactRun {
        indices: Vec<usize>,
        alignment_group: Option<String>,
        original_center: f64,
    }

    // Preserve the placer-established relationship between the PMOS and NMOS
    // halves of one logical leaf. The previous whole-row recentering compacted
    // each polarity independently, which could move corresponding gate access
    // points to opposite sides of the design. Only each leaf's internal
    // transistor sequence is compacted here; all occurrences of that leaf use
    // one shared horizontal center. Ungrouped devices retain their own center.
    let mut compact_runs = Vec::<CompactRun>::new();
    let mut group_occurrence_centers = HashMap::<String, Vec<f64>>::new();
    for indices in rows.values() {
        let mut cursor = 0;
        while cursor < indices.len() {
            let first_index = indices[cursor];
            let alignment_group = by_id[&placements[first_index].component_id]
                .standard_cell_group
                .clone();
            let mut end = cursor + 1;
            while end < indices.len()
                && by_id[&placements[indices[end]].component_id].standard_cell_group
                    == alignment_group
            {
                end += 1;
            }
            let run_indices = indices[cursor..end].to_vec();
            let original_center = run_indices
                .iter()
                .map(|index| placements[*index].x)
                .sum::<f64>()
                / run_indices.len() as f64;
            if let Some(group) = alignment_group.as_ref() {
                group_occurrence_centers
                    .entry(group.clone())
                    .or_default()
                    .push(original_center);
            }
            compact_runs.push(CompactRun {
                indices: run_indices,
                alignment_group,
                original_center,
            });
            cursor = end;
        }
    }
    let aligned_group_centers = group_occurrence_centers
        .into_iter()
        .map(|(group, centers)| {
            let center = centers.iter().sum::<f64>() / centers.len() as f64;
            (
                group,
                (center / rules.manufacturing_grid_um).round() * rules.manufacturing_grid_um,
            )
        })
        .collect::<HashMap<_, _>>();

    let mut compacted = placements.to_vec();
    let oriented_by_id = oriented_device_map(placements, devices, rules.manufacturing_grid_um);
    for run in compact_runs {
        let row = run
            .indices
            .iter()
            .map(|index| by_id[&placements[*index].component_id])
            .collect::<Vec<_>>();
        let oriented = row
            .iter()
            .map(|device| {
                oriented_by_id
                    .get(&device.component_id)
                    .cloned()
                    .unwrap_or_else(|| (*device).clone())
            })
            .collect::<Vec<_>>();
        let center = run
            .alignment_group
            .as_ref()
            .and_then(|group| aligned_group_centers.get(group))
            .copied()
            .unwrap_or(run.original_center);
        let positions = compact_oriented_row_positions(&oriented, rules)
            .into_iter()
            .map(|x| {
                ((x + center) / rules.manufacturing_grid_um).round() * rules.manufacturing_grid_um
            })
            .collect::<Vec<_>>();
        for (index, x) in run.indices.iter().zip(positions) {
            let device = by_id[&placements[*index].component_id];
            let half_width = device_active_width(device, rules) / 2.0;
            if x - half_width < -width / 2.0 + rules.well_enclosure_um - 1e-9
                || x + half_width > width / 2.0 - rules.well_enclosure_um + 1e-9
            {
                return false;
            }
            compacted[*index].x = x;
            compacted[*index].site = (x / rules.manufacturing_grid_um).round() as i64;
        }
    }

    // Columns describe final adjacency, not the pre-compaction row order.
    for indices in rows.values_mut() {
        indices.sort_by(|left, right| {
            compacted[*left]
                .x
                .total_cmp(&compacted[*right].x)
                .then_with(|| {
                    compacted[*left]
                        .component_id
                        .cmp(&compacted[*right].component_id)
                })
        });
        for (column, index) in indices.iter().enumerate() {
            compacted[*index].column = column;
        }
    }

    // Validate every non-active part of the compact device bank atomically.
    // Active overlap is intentional only at the coincident same-net boundary
    // established above; row topology replaces those rectangles with one
    // owned island before DRC and extraction.
    let mut canvas = PhysicalCanvas::new(rules);
    let entries = compact_device_entries(&compacted, devices, rules);
    if let Err(collisions) = commit_compact_device_entries(&mut canvas, &entries) {
        if std::env::var_os("OPENCHIPPY_ROUTER_TRACE").is_some() {
            eprintln!(
                "compact topology rejected: {} collision(s), first={:?}",
                collisions.len(),
                collisions.first()
            );
        }
        return false;
    }
    placements.clone_from_slice(&compacted);
    true
}

fn candidate_metrics(
    placements: &[PlacedDevice],
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    planning: &PhysicalPlanningReport,
) -> (f64, f64, usize, usize, f64) {
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
    // Global HPWL is deliberately insensitive to which terminal inside a
    // multi-terminal net is far away.  Give short connections inside a leaf
    // logic cell their own cost so placement does not leave the detailed
    // router to bridge avoidable source/drain gaps.
    let mut leaf_locality = 0.0;
    for net in nets
        .iter()
        .filter(|net| !matches!(net.role, NetRole::Power | NetRole::Ground))
    {
        let terminals = net
            .terminals
            .iter()
            .filter_map(|terminal| {
                let device = by_id.get(&terminal.component_id)?;
                let group = device.standard_cell_group.as_deref()?;
                let position = positions.get(&terminal.component_id)?;
                Some((group, *position))
            })
            .collect::<Vec<_>>();
        for left in 0..terminals.len() {
            for right in left + 1..terminals.len() {
                if terminals[left].0 == terminals[right].0 {
                    leaf_locality += (terminals[left].1 .0 - terminals[right].1 .0).abs()
                        + (terminals[left].1 .1 - terminals[right].1 .1).abs();
                }
            }
        }
    }
    let mut sharing = 0;
    let mut gate_straps = 0;
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
            gate_straps += row
                .windows(2)
                .filter(|pair| {
                    by_id[&pair[0].component_id].gate_net == by_id[&pair[1].component_id].gate_net
                })
                .count();
        }
    }
    (
        wire_length,
        peak_utilization,
        sharing,
        gate_straps,
        leaf_locality,
    )
}

pub fn place(
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    technology: &Technology,
    planning: &PhysicalPlanningReport,
) -> PhysicalPlacementReport {
    let selected_floorplan = &planning.candidates[planning.selected_candidate];
    let metal1 = technology
        .physical_rules
        .layer_overrides
        .get("metal1")
        .unwrap_or(&technology.physical_rules.metal);
    let widest_active = devices
        .iter()
        .map(|device| device_active_width(device, &technology.physical_rules))
        .fold(technology.physical_rules.diffusion.min_width_um, f64::max);
    let diffusion_pitch = widest_active + technology.physical_rules.diffusion.min_spacing_um;
    let widest_terminal_span = devices
        .iter()
        .map(|device| 2.0 * device_terminal_offset(device, &technology.physical_rules))
        .fold(0.0, f64::max);
    let landing_pitch = widest_terminal_span
        + device_diffusion_landing_size(&technology.physical_rules)
        + metal1.min_spacing_um;
    let device_pitch = (diffusion_pitch.max(landing_pitch) / planning.placement_site_width_um)
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
    let standard_cell_groups = devices
        .iter()
        .filter_map(|device| device.standard_cell_group.as_deref())
        .collect::<HashSet<_>>()
        .len();
    let mut strategies = vec![
        PlacementStrategy::Topology,
        PlacementStrategy::GeometryTopology,
        PlacementStrategy::GateTopology,
        PlacementStrategy::Diffusion,
        PlacementStrategy::Congestion,
    ];
    if hierarchy_groups >= 2 {
        strategies.insert(0, PlacementStrategy::Hierarchy);
    }
    if standard_cell_groups >= 2 {
        strategies.push(PlacementStrategy::LeafTopology);
        strategies.push(PlacementStrategy::LeafGeometryTopology);
        strategies.push(PlacementStrategy::HierarchyTopology);
    }
    let mut candidates = Vec::new();
    for (id, strategy) in strategies.into_iter().enumerate() {
        let mut placements = Vec::with_capacity(devices.len());
        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        let mut occupancy_retries = 0;
        let mut reserved_device_shapes = 0;
        for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
            let ordered = if strategy == PlacementStrategy::GeometryTopology {
                geometry_topology_order(
                    devices
                        .iter()
                        .filter(|device| device.kind == kind)
                        .collect(),
                    &power_nets,
                    selected_floorplan.columns,
                )
            } else {
                strategy_order(devices, kind, strategy, &power_nets)
            };
            let legalized = legalize_row(
                &ordered,
                kind,
                matches!(
                    strategy,
                    PlacementStrategy::LeafTopology | PlacementStrategy::LeafGeometryTopology
                ),
                false,
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
        let topology_compacted = if matches!(
            strategy,
            PlacementStrategy::GeometryTopology | PlacementStrategy::LeafGeometryTopology
        ) {
            compact_topology_candidate(
                &mut placements,
                devices,
                selected_floorplan.width_um,
                technology,
            )
        } else {
            false
        };
        if topology_compacted {
            reserved_device_shapes =
                compact_device_entries(&placements, devices, &technology.physical_rules).len();
        }
        let mut standard_cell_instances = 0;
        if standard_cell_groups >= 2 && strategy == PlacementStrategy::HierarchyTopology {
            let baseline = placements.clone();
            if let Some(instances) = canonicalize_identical_blocks(
                &mut placements,
                devices,
                planning.placement_site_width_um,
                planning.row_height_um,
                device_pitch,
                selected_floorplan.width_um,
                selected_floorplan.height_um,
                technology,
                &power_nets,
            ) {
                standard_cell_instances = instances;
            } else {
                placements = baseline;
            }
        }
        let legal = placements.len() == devices.len();
        let (wire_length, peak_utilization, sharing, gate_straps, leaf_locality) =
            candidate_metrics(&placements, devices, nets, planning);
        let regions = block_regions(
            &placements,
            devices,
            device_pitch,
            planning.row_height_um,
            strategy == PlacementStrategy::HierarchyTopology,
        );
        candidates.push(PlacementCandidate {
            id,
            strategy,
            devices: placements,
            block_regions: regions,
            legal,
            estimated_wire_length_um: wire_length,
            peak_bin_utilization: peak_utilization,
            diffusion_sharing_pairs: sharing,
            gate_strap_pairs: gate_straps,
            occupancy_retries,
            reserved_device_shapes,
            standard_cell_instances,
            topology_compacted,
            score: if legal {
                wire_length + leaf_locality * 4.0 + peak_utilization * 25.0
                    - sharing as f64 * device_pitch * 0.35
                    - gate_straps as f64 * device_pitch * 0.75
            } else {
                1e12
            },
        });
    }
    let standard_cell_candidate = candidates
        .last()
        .is_some_and(|candidate| candidate.strategy == PlacementStrategy::HierarchyTopology)
        .then(|| candidates.pop().expect("checked standard-cell candidate"));
    let selected_candidate = candidates
        .iter()
        .filter(|candidate| candidate.legal)
        .min_by(|left, right| left.score.total_cmp(&right.score))
        .or_else(|| {
            candidates
                .iter()
                .min_by(|left, right| left.score.total_cmp(&right.score))
        })
        .map(|candidate| candidate.id)
        .unwrap_or(0);
    if std::env::var_os("OPENCHIPPY_ROUTER_TRACE").is_some() {
        eprintln!(
            "placement candidates: expected={} selected={} {:?}",
            devices.len(),
            selected_candidate,
            candidates
                .iter()
                .map(|candidate| (
                    candidate.id,
                    candidate.strategy,
                    candidate.devices.len(),
                    candidate.legal,
                    candidate.occupancy_retries,
                    candidate.diffusion_sharing_pairs,
                    candidate.gate_strap_pairs,
                    candidate.score,
                ))
                .collect::<Vec<_>>()
        );
    }
    PhysicalPlacementReport {
        device_pitch_um: device_pitch,
        candidates,
        selected_candidate,
        standard_cell_candidate,
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
            false,
            false,
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
    let (wire_length, peak_utilization, sharing, gate_straps, leaf_locality) =
        candidate_metrics(&placements, devices, nets, planning);
    let regions = block_regions(
        &placements,
        devices,
        report.device_pitch_um,
        planning.row_height_um,
        false,
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
        gate_strap_pairs: gate_straps,
        occupancy_retries,
        reserved_device_shapes,
        standard_cell_instances: 0,
        topology_compacted: false,
        score: if legal {
            wire_length + leaf_locality * 4.0 + peak_utilization * 25.0
                - sharing as f64 * report.device_pitch_um * 0.35
                - gate_straps as f64 * report.device_pitch_um * 0.20
        } else {
            1e12
        },
    });
    report.selected_candidate = id;
}

#[cfg(test)]
mod tests {
    use super::{
        commit_compact_device_entries, compact_device_entries, compact_row_positions,
        compact_topology_candidate, frontier_parent_index, geometry_topology_order, legalize_row,
        oriented_row_devices, strategy_order, PlacedDevice, PlacementStrategy,
    };
    use crate::{
        physical_canvas::{device_active_width, ObstructionType, PhysicalCanvas},
        physical_layout::{DeviceKind, PhysicalDevice, PhysicalLayer, PhysicalShapePurpose},
        technology::Technology,
    };
    use std::collections::HashSet;
    use uuid::Uuid;

    fn long_gate(name: &str, gate_net: usize) -> PhysicalDevice {
        PhysicalDevice {
            component_id: Uuid::new_v4(),
            name: name.into(),
            physical_group: None,
            standard_cell_group: Some("CELL".into()),
            kind: DeviceKind::Nmos,
            gate_net,
            drain_net: gate_net + 10,
            source_net: gate_net + 20,
            width_um: 1.0,
            length_um: 2.0,
        }
    }

    fn topology_device(
        name: &str,
        gate_net: usize,
        drain_net: usize,
        source_net: usize,
    ) -> PhysicalDevice {
        PhysicalDevice {
            component_id: Uuid::new_v4(),
            name: name.into(),
            physical_group: None,
            standard_cell_group: Some("CELL".into()),
            kind: DeviceKind::Nmos,
            gate_net,
            drain_net,
            source_net,
            width_um: 1.0,
            length_um: 0.18,
        }
    }

    #[test]
    fn series_topology_collapses_matching_terminals_to_one_coordinate() {
        let technology = Technology::default();
        let first = topology_device("M1", 1, 10, 20);
        let second = topology_device("M2", 2, 20, 30);
        let third = topology_device("M3", 3, 30, 40);
        let row = vec![&first, &second, &third];
        let oriented = oriented_row_devices(&row);
        assert_eq!(oriented[0].source_net, oriented[1].drain_net);
        assert_eq!(oriented[1].source_net, oriented[2].drain_net);

        let positions = compact_row_positions(&row, &technology.physical_rules);
        for index in 1..positions.len() {
            let expected = crate::physical_canvas::device_terminal_offset(
                &oriented[index - 1],
                &technology.physical_rules,
            ) + crate::physical_canvas::device_terminal_offset(
                &oriented[index],
                &technology.physical_rules,
            );
            assert!((positions[index] - positions[index - 1] - expected).abs() < 1e-9);
        }
    }

    #[test]
    fn compact_row_preserves_poly_spacing_for_unshared_neighbors() {
        let technology = Technology::default();
        let mut left = topology_device("M1", 1, 10, 20);
        let mut right = topology_device("M2", 2, 30, 40);
        left.length_um = 1.2;
        right.length_um = 0.9;
        let positions = compact_row_positions(&[&left, &right], &technology.physical_rules);
        let required = (left.length_um + right.length_um) / 2.0
            + technology.physical_rules.poly.min_spacing_um;
        assert!(positions[1] - positions[0] >= required - 1e-9);
    }

    #[test]
    fn compact_candidate_atomically_admits_a_shared_series_boundary() {
        let technology = Technology::default();
        let left = topology_device("M1", 1, 10, 20);
        let mut right = topology_device("M2", 2, 20, 30);
        right.gate_net = left.gate_net;
        let shared_gate_net = left.gate_net;
        let mut placements = vec![
            PlacedDevice {
                component_id: left.component_id,
                name: left.name.clone(),
                kind: DeviceKind::Nmos,
                x: -2.0,
                y: 0.0,
                row: 0,
                column: 0,
                site: -4,
            },
            PlacedDevice {
                component_id: right.component_id,
                name: right.name.clone(),
                kind: DeviceKind::Nmos,
                x: 2.0,
                y: 0.0,
                row: 0,
                column: 1,
                site: 4,
            },
        ];
        assert!(compact_topology_candidate(
            &mut placements,
            &[left.clone(), right.clone()],
            10.0,
            &technology,
        ));
        let oriented = oriented_row_devices(&[&left, &right]);
        let expected = crate::physical_canvas::device_terminal_offset(
            &oriented[0],
            &technology.physical_rules,
        ) + crate::physical_canvas::device_terminal_offset(
            &oriented[1],
            &technology.physical_rules,
        );
        assert!(placements[1].x - placements[0].x + 1e-9 >= expected);

        let entries =
            compact_device_entries(&placements, &[left, right], &technology.physical_rules);
        let active = entries
            .iter()
            .filter(|(shape, _, _)| {
                shape.purpose == crate::physical_layout::PhysicalShapePurpose::Active
            })
            .collect::<Vec<_>>();
        assert_eq!(
            active.len(),
            1,
            "one shared topology run must reserve one atomic active envelope"
        );
        assert!(
            entries.iter().any(|(shape, owner, obstruction)| {
                owner.starts_with("compact-topology-poly:")
                    && *obstruction == ObstructionType::Poly
                    && shape.layer == crate::physical_layout::PhysicalLayer::Poly
                    && shape.purpose == crate::physical_layout::PhysicalShapePurpose::GateAccess
                    && shape.component_id.is_none()
                    && shape.net == Some(shared_gate_net)
            }),
            "candidate admission must reserve the exact shared gate topology"
        );
        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        commit_compact_device_entries(&mut canvas, &entries)
            .expect("the finalized compact footprint must be atomically admissible");
        assert_eq!(
            canvas
                .occupied_on_layer(crate::physical_layout::PhysicalLayer::Ndiff)
                .len(),
            1
        );
    }

    #[test]
    fn compact_candidate_reserves_exact_mixed_width_active_union() {
        let technology = Technology::default();
        let mut narrow = topology_device("NARROW", 1, 10, 20);
        let mut wide = topology_device("WIDE", 2, 20, 30);
        narrow.width_um = 0.7;
        wide.width_um = 2.4;
        let mut placements = vec![
            PlacedDevice {
                component_id: narrow.component_id,
                name: narrow.name.clone(),
                kind: DeviceKind::Nmos,
                x: -2.0,
                y: 0.0,
                row: 0,
                column: 0,
                site: -4,
            },
            PlacedDevice {
                component_id: wide.component_id,
                name: wide.name.clone(),
                kind: DeviceKind::Nmos,
                x: 2.0,
                y: 0.0,
                row: 0,
                column: 1,
                site: 4,
            },
        ];
        assert!(compact_topology_candidate(
            &mut placements,
            &[narrow.clone(), wide.clone()],
            10.0,
            &technology,
        ));
        let entries = compact_device_entries(
            &placements,
            &[narrow.clone(), wide.clone()],
            &technology.physical_rules,
        );
        let active = entries
            .iter()
            .filter(|(shape, owner, _)| {
                shape.purpose == PhysicalShapePurpose::Active
                    && owner.starts_with("compact-topology-active:")
            })
            .map(|(shape, _, _)| shape)
            .collect::<Vec<_>>();
        assert!(
            active.len() >= 2,
            "mixed-width shared diffusion must retain a rectilinear decomposition"
        );
        let narrow_height = active
            .iter()
            .filter(|shape| {
                placements[0].x >= shape.x - shape.width / 2.0 - 1e-9
                    && placements[0].x <= shape.x + shape.width / 2.0 + 1e-9
            })
            .map(|shape| shape.height)
            .fold(0.0, f64::max);
        let wide_height = active
            .iter()
            .filter(|shape| {
                placements[1].x >= shape.x - shape.width / 2.0 - 1e-9
                    && placements[1].x <= shape.x + shape.width / 2.0 + 1e-9
            })
            .map(|shape| shape.height)
            .fold(0.0, f64::max);
        assert!((narrow_height - narrow.width_um).abs() <= 1e-9);
        assert!((wide_height - wide.width_um).abs() <= 1e-9);

        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        commit_compact_device_entries(&mut canvas, &entries)
            .expect("one rectilinear active island must admit atomically");
        assert_eq!(
            canvas.occupied_on_layer(PhysicalLayer::Ndiff).len(),
            active.len()
        );
    }

    #[test]
    fn compact_candidate_keeps_leaf_polarities_on_one_horizontal_center() {
        let technology = Technology::default();
        let mut devices = vec![
            topology_device("P1", 7, 10, 11),
            topology_device("P2", 7, 11, 12),
            topology_device("N1", 7, 20, 21),
            topology_device("N2", 7, 21, 22),
        ];
        for device in &mut devices {
            device.standard_cell_group = Some("LEAF".into());
        }
        devices[0].kind = DeviceKind::Pmos;
        devices[1].kind = DeviceKind::Pmos;
        devices[2].kind = DeviceKind::Nmos;
        devices[3].kind = DeviceKind::Nmos;
        let mut placements = vec![
            PlacedDevice {
                component_id: devices[0].component_id,
                name: devices[0].name.clone(),
                kind: DeviceKind::Pmos,
                x: -5.0,
                y: -3.0,
                row: 0,
                column: 0,
                site: -10,
            },
            PlacedDevice {
                component_id: devices[1].component_id,
                name: devices[1].name.clone(),
                kind: DeviceKind::Pmos,
                x: -1.0,
                y: -3.0,
                row: 0,
                column: 1,
                site: -2,
            },
            PlacedDevice {
                component_id: devices[2].component_id,
                name: devices[2].name.clone(),
                kind: DeviceKind::Nmos,
                x: 1.0,
                y: 3.0,
                row: 1,
                column: 0,
                site: 2,
            },
            PlacedDevice {
                component_id: devices[3].component_id,
                name: devices[3].name.clone(),
                kind: DeviceKind::Nmos,
                x: 5.0,
                y: 3.0,
                row: 1,
                column: 1,
                site: 10,
            },
        ];

        assert!(compact_topology_candidate(
            &mut placements,
            &devices,
            20.0,
            &technology,
        ));
        let p_center = (placements[0].x + placements[1].x) / 2.0;
        let n_center = (placements[2].x + placements[3].x) / 2.0;
        assert!((p_center - n_center).abs() <= technology.physical_rules.manufacturing_grid_um);
        assert!(p_center.abs() <= technology.physical_rules.manufacturing_grid_um);
    }

    #[test]
    fn device_legalization_retries_the_nearest_available_site() {
        let technology = Technology::default();
        let left = long_gate("M1", 1);
        let right = long_gate("M2", 2);
        let minimum_separation = device_active_width(&left, &technology.physical_rules)
            + technology.physical_rules.diffusion.min_spacing_um;
        let ordered = vec![&left, &right];
        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        let result = legalize_row(
            &ordered,
            DeviceKind::Nmos,
            false,
            false,
            2,
            12.0,
            6.0,
            0.5,
            1.7,
            2.0,
            &technology,
            &mut canvas,
        );

        assert_eq!(result.devices.len(), 2);
        assert!(result.retries > 0);
        assert_eq!(result.reserved_shapes, 18);
        assert!((result.devices[1].x - result.devices[0].x).abs() >= minimum_separation - 1e-9);
        assert_eq!(canvas.occupied_count(), 18);
    }

    #[test]
    fn leaf_groups_wrap_intact_at_row_boundaries() {
        let technology = Technology::default();
        let mut first = long_gate("A1", 1);
        first.standard_cell_group = Some("A".into());
        first.length_um = 0.18;
        let mut second = long_gate("B1", 2);
        second.standard_cell_group = Some("B".into());
        second.length_um = 0.18;
        let mut pair_left = long_gate("C1", 3);
        pair_left.standard_cell_group = Some("C".into());
        pair_left.length_um = 0.18;
        let mut pair_right = long_gate("C2", 4);
        pair_right.standard_cell_group = Some("C".into());
        pair_right.length_um = 0.18;
        let ordered = vec![&first, &second, &pair_left, &pair_right];
        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        let result = legalize_row(
            &ordered,
            DeviceKind::Nmos,
            true,
            false,
            3,
            12.0,
            12.0,
            0.5,
            1.7,
            2.4,
            &technology,
            &mut canvas,
        );

        assert_eq!(result.devices.len(), 4);
        assert_eq!(result.devices[2].row, result.devices[3].row);
        assert_ne!(result.devices[1].row, result.devices[2].row);
        assert!((result.devices[2].x - result.devices[3].x).abs() <= 2.4 + 1e-9);
    }

    #[test]
    fn geometry_topology_follows_center_out_physical_neighbors() {
        let root = topology_device("ROOT", 99, 1, 2);
        let right = topology_device("RIGHT", 10, 1, 3);
        let left = topology_device("LEFT", 11, 2, 4);
        let right_leaf = topology_device("RIGHT_LEAF", 99, 3, 5);
        let left_leaf = topology_device("LEFT_LEAF", 99, 4, 6);
        let ordered = geometry_topology_order(
            vec![&left_leaf, &right, &root, &right_leaf, &left],
            &HashSet::new(),
            5,
        );

        assert_eq!(ordered[0].name, "ROOT");
        for index in 1..ordered.len() {
            let parent = frontier_parent_index(index, 5).unwrap();
            let parent_nets = [ordered[parent].drain_net, ordered[parent].source_net];
            assert!(
                [ordered[index].drain_net, ordered[index].source_net]
                    .into_iter()
                    .any(|net| parent_nets.contains(&net)),
                "{} should share diffusion with its physical parent {}",
                ordered[index].name,
                ordered[parent].name
            );
        }
    }

    #[test]
    fn leaf_groups_expand_from_the_hardest_group_toward_both_sides() {
        let technology = Technology::default();
        let mut devices = ["ROOT", "RIGHT", "LEFT", "RIGHT_OUTER", "LEFT_OUTER"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let mut device = topology_device(name, index + 1, index + 10, index + 20);
                device.standard_cell_group = Some(name.into());
                device
            })
            .collect::<Vec<_>>();
        let ordered = devices
            .iter_mut()
            .map(|device| &*device)
            .collect::<Vec<_>>();
        let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
        let result = legalize_row(
            &ordered,
            DeviceKind::Nmos,
            true,
            true,
            5,
            16.0,
            8.0,
            0.5,
            1.7,
            2.4,
            &technology,
            &mut canvas,
        );

        assert_eq!(
            result
                .devices
                .iter()
                .map(|device| device.name.as_str())
                .collect::<Vec<_>>(),
            ["LEFT_OUTER", "LEFT", "ROOT", "RIGHT", "RIGHT_OUTER"]
        );
    }

    #[test]
    fn leaf_geometry_uses_one_block_order_for_both_polarity_rows() {
        let mut devices = Vec::new();
        for (group_index, group) in ["ALU", "CARRY", "SUM", "OUTPUT"].into_iter().enumerate() {
            for kind in [DeviceKind::Pmos, DeviceKind::Nmos] {
                let mut device = topology_device(
                    &format!("{group}_{kind:?}"),
                    group_index + 1,
                    group_index + 10,
                    group_index + 11,
                );
                device.kind = kind;
                device.standard_cell_group = Some(group.into());
                devices.push(device);
            }
        }
        let power_nets = HashSet::new();
        let group_order = |kind| {
            strategy_order(
                &devices,
                kind,
                PlacementStrategy::LeafGeometryTopology,
                &power_nets,
            )
            .into_iter()
            .map(|device| device.standard_cell_group.as_deref().unwrap())
            .collect::<Vec<_>>()
        };

        assert_eq!(group_order(DeviceKind::Pmos), group_order(DeviceKind::Nmos));
    }
}
