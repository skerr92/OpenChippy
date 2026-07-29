use crate::{
    physical_canvas::{
        device_active_width, device_diffusion_height, device_diffusion_landing_size,
        device_terminal_offset,
    },
    physical_layout::{NetRole, PhysicalDevice, PhysicalNet, PhysicalPin},
    technology::{RoutingDirection, Technology},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FloorplanStrategy {
    Square,
    Balanced,
    Topology,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhysicalNetClass {
    Power,
    Global,
    Signal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingLayerPlan {
    pub layer: u16,
    pub pitch_um: f64,
    pub offset_um: f64,
    pub preferred_direction: RoutingDirection,
    pub capacity_adjustment: f64,
    pub reserved_for_power: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedNet {
    pub net: usize,
    pub name: String,
    pub class: PhysicalNetClass,
    pub priority: u16,
    pub terminal_count: usize,
    pub estimated_demand: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingBinLayerCapacity {
    pub layer: u16,
    pub horizontal_tracks: u16,
    pub vertical_tracks: u16,
    pub reserved_for_power: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingBin {
    pub id: usize,
    pub column: usize,
    pub row: usize,
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub layer_capacities: Vec<RoutingBinLayerCapacity>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorplanCandidate {
    pub id: usize,
    pub strategy: FloorplanStrategy,
    pub width_um: f64,
    pub height_um: f64,
    pub columns: usize,
    pub pmos_rows: usize,
    pub nmos_rows: usize,
    pub growth_passes: u16,
    pub device_density: f64,
    pub estimated_routing_utilization: f64,
    pub estimated_routing_demand: f64,
    pub estimated_routing_capacity: f64,
    pub pin_access_points: usize,
    pub feasible: bool,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalPlanningReport {
    pub placement_site_width_um: f64,
    pub row_height_um: f64,
    pub target_device_density: f64,
    pub target_routing_utilization: f64,
    pub floorplan_growth_factor: f64,
    pub max_floorplan_growth_passes: u16,
    pub global_route_max_iterations: u16,
    pub global_route_stall_iterations: u16,
    pub detailed_route_max_iterations: u16,
    pub routing_layers: Vec<RoutingLayerPlan>,
    pub nets: Vec<PlannedNet>,
    pub candidates: Vec<FloorplanCandidate>,
    pub selected_candidate: usize,
    pub coarse_bin_size_um: f64,
    pub bin_columns: usize,
    pub bin_rows: usize,
    pub routing_bins: Vec<RoutingBin>,
}

impl Default for PhysicalPlanningReport {
    fn default() -> Self {
        Self {
            placement_site_width_um: 0.5,
            row_height_um: 1.7,
            target_device_density: 0.65,
            target_routing_utilization: 0.75,
            floorplan_growth_factor: 0.08,
            max_floorplan_growth_passes: 3,
            global_route_max_iterations: 30,
            global_route_stall_iterations: 3,
            detailed_route_max_iterations: 10,
            routing_layers: Vec::new(),
            nets: Vec::new(),
            candidates: Vec::new(),
            selected_candidate: 0,
            coarse_bin_size_um: 0.0,
            bin_columns: 0,
            bin_rows: 0,
            routing_bins: Vec::new(),
        }
    }
}

fn snap_up(value: f64, quantum: f64) -> f64 {
    (value / quantum).ceil().max(1.0) * quantum
}

fn dimensions(
    columns: usize,
    pmos_count: usize,
    nmos_count: usize,
    channel_height: f64,
    site_width: f64,
    row_height: f64,
    device_pitch: f64,
) -> (f64, f64, usize, usize) {
    let pmos_rows = pmos_count.max(1).div_ceil(columns);
    let nmos_rows = nmos_count.max(1).div_ceil(columns);
    let width = snap_up(columns as f64 * device_pitch + 2.0, site_width);
    let height = snap_up(
        (pmos_rows + nmos_rows) as f64 * row_height + channel_height + 2.6,
        row_height,
    );
    (width, height, pmos_rows, nmos_rows)
}

fn choose_columns(
    target_aspect: f64,
    largest_row: usize,
    pmos_count: usize,
    nmos_count: usize,
    channel_height: f64,
    site_width: f64,
    row_height: f64,
    device_pitch: f64,
) -> usize {
    (1..=largest_row)
        .min_by(|left, right| {
            let (left_width, left_height, _, _) = dimensions(
                *left,
                pmos_count,
                nmos_count,
                channel_height,
                site_width,
                row_height,
                device_pitch,
            );
            let (right_width, right_height, _, _) = dimensions(
                *right,
                pmos_count,
                nmos_count,
                channel_height,
                site_width,
                row_height,
                device_pitch,
            );
            ((left_width / left_height).ln() - target_aspect.ln())
                .abs()
                .total_cmp(&((right_width / right_height).ln() - target_aspect.ln()).abs())
        })
        .unwrap_or(1)
}

pub fn plan(
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    technology: &Technology,
) -> PhysicalPlanningReport {
    let rules = &technology.physical_planning;
    let routing_layers = rules
        .resolved_routing_layers(technology.max_metal_layers)
        .into_iter()
        .filter_map(|(name, resource)| {
            let layer = name.strip_prefix("metal")?.parse().ok()?;
            Some(RoutingLayerPlan {
                layer,
                pitch_um: resource.pitch_um,
                offset_um: resource.offset_um,
                preferred_direction: resource.preferred_direction,
                capacity_adjustment: resource.capacity_adjustment,
                reserved_for_power: resource.reserved_for_power,
            })
        })
        .collect::<Vec<_>>();
    let signal_layers = routing_layers
        .iter()
        .filter(|layer| !layer.reserved_for_power)
        .collect::<Vec<_>>();
    let representative_pitch = signal_layers
        .iter()
        .map(|layer| layer.pitch_um)
        .sum::<f64>()
        / signal_layers.len().max(1) as f64;

    let mut planned_nets = nets
        .iter()
        .map(|net| {
            let class = if matches!(net.role, NetRole::Power | NetRole::Ground) {
                PhysicalNetClass::Power
            } else if net.terminals.len() >= 4
                || matches!(net.role, NetRole::Input | NetRole::Output)
            {
                PhysicalNetClass::Global
            } else {
                PhysicalNetClass::Signal
            };
            PlannedNet {
                net: net.id,
                name: net.name.clone(),
                class,
                priority: match class {
                    PhysicalNetClass::Power => 0,
                    PhysicalNetClass::Global => 100,
                    PhysicalNetClass::Signal => 200,
                },
                terminal_count: net.terminals.len(),
                estimated_demand: net.terminals.len().saturating_sub(1).max(1) as f64,
            }
        })
        .collect::<Vec<_>>();
    planned_nets.sort_by_key(|net| (net.priority, std::cmp::Reverse(net.terminal_count), net.net));

    let pmos_count = devices
        .iter()
        .filter(|device| device.kind == crate::physical_layout::DeviceKind::Pmos)
        .count();
    let nmos_count = devices.len() - pmos_count;
    let largest_row = pmos_count.max(nmos_count).max(1);
    let signal_net_count = planned_nets
        .iter()
        .filter(|net| net.class != PhysicalNetClass::Power)
        .count();
    let channel_tracks = signal_net_count.max(1).div_ceil(signal_layers.len().max(1));
    let channel_height =
        (channel_tracks as f64 * representative_pitch / rules.target_routing_utilization + 0.9)
            .max(rules.row_height_um);
    let device_area = devices
        .iter()
        .map(|device| {
            device_active_width(device, &technology.physical_rules)
                * device_diffusion_height(device, &technology.physical_rules)
        })
        .sum::<f64>()
        .max(rules.placement_site_width_um * rules.row_height_um);
    let widest_active = devices
        .iter()
        .map(|device| device_active_width(device, &technology.physical_rules))
        .fold(technology.physical_rules.diffusion.min_width_um, f64::max);
    let widest_terminal_span = devices
        .iter()
        .map(|device| 2.0 * device_terminal_offset(device, &technology.physical_rules))
        .fold(0.0, f64::max);
    let metal1 = technology
        .physical_rules
        .layer_overrides
        .get("metal1")
        .unwrap_or(&technology.physical_rules.metal);
    let device_pitch = (widest_active + technology.physical_rules.diffusion.min_spacing_um).max(
        widest_terminal_span
            + device_diffusion_landing_size(&technology.physical_rules)
            + metal1.min_spacing_um,
    );
    let topology_aspect = (largest_row as f64
        / (pmos_count.max(1).div_ceil(largest_row) + nmos_count.max(1).div_ceil(largest_row))
            as f64)
        .clamp(0.5, 3.0);
    let strategies = [
        (FloorplanStrategy::Square, 1.0),
        (FloorplanStrategy::Balanced, 4.0 / 3.0),
        (FloorplanStrategy::Topology, topology_aspect),
    ];
    let mut candidates = Vec::new();
    for (id, (strategy, target_aspect)) in strategies.into_iter().enumerate() {
        let columns = choose_columns(
            target_aspect,
            largest_row,
            pmos_count,
            nmos_count,
            channel_height,
            rules.placement_site_width_um,
            rules.row_height_um,
            device_pitch,
        );
        let (mut width, mut height, pmos_rows, nmos_rows) = dimensions(
            columns,
            pmos_count,
            nmos_count,
            channel_height,
            rules.placement_site_width_um,
            rules.row_height_um,
            device_pitch,
        );
        let density_scale = (device_area / (width * height * rules.target_device_density))
            .sqrt()
            .max(1.0);
        width = snap_up(width * density_scale, rules.placement_site_width_um);
        height = snap_up(height * density_scale, rules.row_height_um);
        let mut growth_passes = 0;
        let (mut demand, mut capacity, mut utilization);
        loop {
            demand = planned_nets
                .iter()
                .filter(|net| net.class != PhysicalNetClass::Power)
                .map(|net| net.estimated_demand * (width + height) / 4.0)
                .sum::<f64>();
            capacity = signal_layers
                .iter()
                .map(|layer| {
                    let tracks = match layer.preferred_direction {
                        RoutingDirection::Horizontal => height / layer.pitch_um,
                        RoutingDirection::Vertical => width / layer.pitch_um,
                        RoutingDirection::Any => (width + height) / (2.0 * layer.pitch_um),
                    };
                    let span = match layer.preferred_direction {
                        RoutingDirection::Horizontal => width,
                        RoutingDirection::Vertical => height,
                        RoutingDirection::Any => (width + height) / 2.0,
                    };
                    tracks.floor() * span * layer.capacity_adjustment
                })
                .sum::<f64>()
                .max(representative_pitch);
            utilization = demand / capacity;
            let density = device_area / (width * height);
            if (density <= rules.target_device_density
                && utilization <= rules.target_routing_utilization)
                || growth_passes >= rules.max_floorplan_growth_passes
            {
                break;
            }
            if width / height > target_aspect {
                height = snap_up(
                    height * (1.0 + rules.floorplan_growth_factor),
                    rules.row_height_um,
                );
            } else {
                width = snap_up(
                    width * (1.0 + rules.floorplan_growth_factor),
                    rules.placement_site_width_um,
                );
            }
            growth_passes += 1;
        }
        let device_density = device_area / (width * height);
        let feasible = device_density <= rules.target_device_density + 1e-9
            && utilization <= rules.target_routing_utilization + 1e-9;
        let aspect_penalty = ((width / height).ln() - target_aspect.ln()).abs();
        candidates.push(FloorplanCandidate {
            id,
            strategy,
            width_um: width,
            height_um: height,
            columns,
            pmos_rows,
            nmos_rows,
            growth_passes,
            device_density,
            estimated_routing_utilization: utilization,
            estimated_routing_demand: demand,
            estimated_routing_capacity: capacity,
            pin_access_points: pins.len(),
            feasible,
            score: if feasible {
                width * height * (1.0 + aspect_penalty * 0.05)
            } else {
                1e12 + utilization * 1e6 + width * height
            },
        });
    }
    let selected_candidate = candidates
        .iter()
        .min_by(|left, right| left.score.total_cmp(&right.score))
        .map(|candidate| candidate.id)
        .unwrap_or(0);
    let selected = &candidates[selected_candidate];
    let coarse_bin_size_um = snap_up(
        (representative_pitch * 4.0).max(rules.row_height_um),
        rules.placement_site_width_um,
    );
    let bin_columns = (selected.width_um / coarse_bin_size_um).ceil().max(1.0) as usize;
    let bin_rows = (selected.height_um / coarse_bin_size_um).ceil().max(1.0) as usize;
    let bin_width = selected.width_um / bin_columns as f64;
    let bin_height = selected.height_um / bin_rows as f64;
    let mut routing_bins = Vec::with_capacity(bin_columns * bin_rows);
    for row in 0..bin_rows {
        for column in 0..bin_columns {
            let layer_capacities = routing_layers
                .iter()
                .map(|layer| {
                    let adjusted = |tracks: f64| {
                        (tracks.floor() * layer.capacity_adjustment)
                            .floor()
                            .clamp(0.0, f64::from(u16::MAX)) as u16
                    };
                    RoutingBinLayerCapacity {
                        layer: layer.layer,
                        horizontal_tracks: if matches!(
                            layer.preferred_direction,
                            RoutingDirection::Horizontal | RoutingDirection::Any
                        ) {
                            adjusted(bin_height / layer.pitch_um)
                        } else {
                            0
                        },
                        vertical_tracks: if matches!(
                            layer.preferred_direction,
                            RoutingDirection::Vertical | RoutingDirection::Any
                        ) {
                            adjusted(bin_width / layer.pitch_um)
                        } else {
                            0
                        },
                        reserved_for_power: layer.reserved_for_power,
                    }
                })
                .collect();
            routing_bins.push(RoutingBin {
                id: row * bin_columns + column,
                column,
                row,
                min_x: -selected.width_um / 2.0 + column as f64 * bin_width,
                min_y: -selected.height_um / 2.0 + row as f64 * bin_height,
                max_x: -selected.width_um / 2.0 + (column + 1) as f64 * bin_width,
                max_y: -selected.height_um / 2.0 + (row + 1) as f64 * bin_height,
                layer_capacities,
            });
        }
    }

    PhysicalPlanningReport {
        placement_site_width_um: rules.placement_site_width_um,
        row_height_um: rules.row_height_um,
        target_device_density: rules.target_device_density,
        target_routing_utilization: rules.target_routing_utilization,
        floorplan_growth_factor: rules.floorplan_growth_factor,
        max_floorplan_growth_passes: rules.max_floorplan_growth_passes,
        global_route_max_iterations: rules.global_route_max_iterations,
        global_route_stall_iterations: rules.global_route_stall_iterations,
        detailed_route_max_iterations: rules.detailed_route_max_iterations,
        routing_layers,
        nets: planned_nets,
        candidates,
        selected_candidate,
        coarse_bin_size_um,
        bin_columns,
        bin_rows,
        routing_bins,
    }
}
