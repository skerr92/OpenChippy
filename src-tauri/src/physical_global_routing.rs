use crate::{
    physical_layout::{NetRole, PhysicalNet, PhysicalPin},
    physical_placement::PhysicalPlacementReport,
    physical_planning::{PhysicalNetClass, PhysicalPlanningReport, PlannedNet},
};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalRouteSegment {
    pub from_bin: usize,
    pub to_bin: usize,
    pub layer: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalNetRoute {
    pub net: usize,
    pub name: String,
    pub class: PhysicalNetClass,
    pub priority: u16,
    pub segments: Vec<GlobalRouteSegment>,
    pub estimated_length_um: f64,
    pub via_count: usize,
    pub rip_up_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalRoutingIteration {
    pub iteration: u16,
    pub overflow: usize,
    pub rerouted_nets: usize,
    pub best_so_far: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalRoutingReport {
    pub routes: Vec<GlobalNetRoute>,
    pub iterations: Vec<GlobalRoutingIteration>,
    pub total_overflow: usize,
    pub converged: bool,
    pub max_iterations: u16,
    pub stall_limit: u16,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct EdgeKey {
    from: usize,
    to: usize,
    layer: u16,
}

#[derive(Clone)]
struct InternalRoute {
    plan: PlannedNet,
    edges: Vec<EdgeKey>,
    length: f64,
    vias: usize,
    rip_ups: usize,
}

fn edge_capacity(edge: EdgeKey, planning: &PhysicalPlanningReport) -> usize {
    let from = &planning.routing_bins[edge.from];
    let to = &planning.routing_bins[edge.to];
    let horizontal = from.row == to.row;
    let capacity = |bin: &crate::physical_planning::RoutingBin| {
        bin.layer_capacities
            .iter()
            .find(|layer| layer.layer == edge.layer)
            .map(|layer| {
                if horizontal {
                    layer.horizontal_tracks
                } else {
                    layer.vertical_tracks
                }
            })
            .unwrap_or(0)
    };
    usize::from(capacity(from).min(capacity(to)))
}

fn point_bin(x: f64, y: f64, planning: &PhysicalPlanningReport) -> Option<usize> {
    planning
        .routing_bins
        .iter()
        .find(|bin| x >= bin.min_x && x <= bin.max_x && y >= bin.min_y && y <= bin.max_y)
        .map(|bin| bin.id)
}

fn route_points(
    net: &PhysicalNet,
    pins: &[PhysicalPin],
    placement: &PhysicalPlacementReport,
    planning: &PhysicalPlanningReport,
) -> Vec<usize> {
    let selected = &placement.candidates[placement.selected_candidate];
    let positions = selected
        .devices
        .iter()
        .map(|device| (device.component_id, (device.x, device.y)))
        .collect::<HashMap<_, _>>();
    let mut points = net
        .terminals
        .iter()
        .filter_map(|terminal| positions.get(&terminal.component_id).copied())
        .filter_map(|(x, y)| point_bin(x, y, planning))
        .collect::<Vec<_>>();
    for pin in pins.iter().filter(|pin| pin.net == net.id) {
        let bin = match pin.role {
            NetRole::Input => planning.bin_columns.saturating_mul(planning.bin_rows / 2),
            NetRole::Output => {
                planning.bin_columns.saturating_mul(planning.bin_rows / 2)
                    + planning.bin_columns.saturating_sub(1)
            }
            NetRole::Power => planning.bin_columns.saturating_sub(1) / 2,
            NetRole::Ground => {
                planning
                    .bin_columns
                    .saturating_mul(planning.bin_rows.saturating_sub(1))
                    + planning.bin_columns.saturating_sub(1) / 2
            }
            NetRole::Internal => continue,
        };
        if bin < planning.routing_bins.len() {
            points.push(bin);
        }
    }
    points.sort_unstable();
    points.dedup();
    points
}

fn choose_layer(
    from: usize,
    to: usize,
    class: PhysicalNetClass,
    usage: &HashMap<EdgeKey, usize>,
    history: &HashMap<EdgeKey, f64>,
    planning: &PhysicalPlanningReport,
) -> u16 {
    let candidates = planning
        .routing_layers
        .iter()
        .filter(|layer| {
            if class == PhysicalNetClass::Power {
                layer.reserved_for_power
            } else {
                !layer.reserved_for_power
            }
        })
        .collect::<Vec<_>>();
    candidates
        .iter()
        .min_by(|left, right| {
            let cost = |layer: &&crate::physical_planning::RoutingLayerPlan| {
                let edge = EdgeKey {
                    from: from.min(to),
                    to: from.max(to),
                    layer: layer.layer,
                };
                let capacity = edge_capacity(edge, planning);
                let wrong_way = if capacity == 0 { 1000.0 } else { 0.0 };
                wrong_way
                    + usage.get(&edge).copied().unwrap_or(0) as f64 / capacity.max(1) as f64
                    + history.get(&edge).copied().unwrap_or(0.0)
                    + f64::from(layer.layer) * 1e-6
            };
            cost(left).total_cmp(&cost(right))
        })
        .map(|layer| layer.layer)
        .or_else(|| planning.routing_layers.first().map(|layer| layer.layer))
        .unwrap_or(1)
}

fn route_net(
    plan: &PlannedNet,
    net: &PhysicalNet,
    pins: &[PhysicalPin],
    placement: &PhysicalPlacementReport,
    planning: &PhysicalPlanningReport,
    usage: &mut HashMap<EdgeKey, usize>,
    history: &HashMap<EdgeKey, f64>,
    iteration: u16,
    rip_ups: usize,
) -> InternalRoute {
    let points = route_points(net, pins, placement, planning);
    let mut edges = HashSet::new();
    let mut vias = 0;
    let mut last_layer = None;
    let root = points.first().copied();
    for target in points.iter().skip(1).copied() {
        let Some(root) = root else { break };
        let root_bin = &planning.routing_bins[root];
        let target_bin = &planning.routing_bins[target];
        let horizontal_first = (usize::from(iteration) + plan.net) % 2 == 0;
        let mut current_col = root_bin.column;
        let mut current_row = root_bin.row;
        let steps = if horizontal_first {
            [
                (target_bin.column, current_row),
                (target_bin.column, target_bin.row),
            ]
        } else {
            [
                (current_col, target_bin.row),
                (target_bin.column, target_bin.row),
            ]
        };
        for (goal_col, goal_row) in steps {
            while current_col != goal_col || current_row != goal_row {
                let next_col = if current_col < goal_col {
                    current_col + 1
                } else if current_col > goal_col {
                    current_col - 1
                } else {
                    current_col
                };
                let next_row = if current_col == goal_col {
                    if current_row < goal_row {
                        current_row + 1
                    } else {
                        current_row - 1
                    }
                } else {
                    current_row
                };
                let from = current_row * planning.bin_columns + current_col;
                let to = next_row * planning.bin_columns + next_col;
                let layer = choose_layer(from, to, plan.class, usage, history, planning);
                if last_layer.is_some_and(|previous| previous != layer) {
                    vias += 1;
                }
                last_layer = Some(layer);
                let edge = EdgeKey {
                    from: from.min(to),
                    to: from.max(to),
                    layer,
                };
                if edges.insert(edge) {
                    *usage.entry(edge).or_default() += 1;
                }
                current_col = next_col;
                current_row = next_row;
            }
        }
    }
    let length = edges.len() as f64 * planning.coarse_bin_size_um;
    let mut edges = edges.into_iter().collect::<Vec<_>>();
    edges.sort_unstable();
    InternalRoute {
        plan: plan.clone(),
        edges,
        length,
        vias,
        rip_ups,
    }
}

fn overflow_edges(
    usage: &HashMap<EdgeKey, usize>,
    planning: &PhysicalPlanningReport,
) -> (HashMap<EdgeKey, usize>, usize) {
    let overflow = usage
        .iter()
        .filter_map(|(edge, used)| {
            let excess = used.saturating_sub(edge_capacity(*edge, planning));
            (excess > 0).then_some((*edge, excess))
        })
        .collect::<HashMap<_, _>>();
    let total = overflow.values().sum();
    (overflow, total)
}

pub fn route(
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    placement: &PhysicalPlacementReport,
    planning: &PhysicalPlanningReport,
) -> GlobalRoutingReport {
    let net_by_id = nets
        .iter()
        .map(|net| (net.id, net))
        .collect::<HashMap<_, _>>();
    let mut usage = HashMap::new();
    let mut history = HashMap::new();
    let mut routes = HashMap::<usize, InternalRoute>::new();
    for plan in &planning.nets {
        let route = route_net(
            plan,
            net_by_id[&plan.net],
            pins,
            placement,
            planning,
            &mut usage,
            &history,
            0,
            0,
        );
        routes.insert(plan.net, route);
    }
    let (mut overflowing, mut total_overflow) = overflow_edges(&usage, planning);
    let mut iterations = vec![GlobalRoutingIteration {
        iteration: 0,
        overflow: total_overflow,
        rerouted_nets: planning.nets.len(),
        best_so_far: true,
    }];
    let mut best_routes = routes.clone();
    let mut best_overflow = total_overflow;
    let mut stalled = 0;
    for iteration in 1..=planning.global_route_max_iterations {
        if total_overflow == 0 || stalled >= planning.global_route_stall_iterations {
            break;
        }
        for (edge, excess) in &overflowing {
            *history.entry(*edge).or_default() += *excess as f64;
        }
        let conflicted = routes
            .values()
            .filter(|route| {
                route
                    .edges
                    .iter()
                    .any(|edge| overflowing.contains_key(edge))
            })
            .map(|route| route.plan.net)
            .collect::<HashSet<_>>();
        usage.clear();
        for route in routes
            .values()
            .filter(|route| !conflicted.contains(&route.plan.net))
        {
            for edge in &route.edges {
                *usage.entry(*edge).or_default() += 1;
            }
        }
        for plan in planning
            .nets
            .iter()
            .filter(|plan| conflicted.contains(&plan.net))
        {
            let rip_ups = routes
                .get(&plan.net)
                .map(|route| route.rip_ups + 1)
                .unwrap_or(1);
            let rerouted = route_net(
                plan,
                net_by_id[&plan.net],
                pins,
                placement,
                planning,
                &mut usage,
                &history,
                iteration,
                rip_ups,
            );
            routes.insert(plan.net, rerouted);
        }
        let result = overflow_edges(&usage, planning);
        overflowing = result.0;
        total_overflow = result.1;
        let improved = total_overflow < best_overflow;
        if improved {
            best_overflow = total_overflow;
            best_routes = routes.clone();
            stalled = 0;
        } else {
            stalled += 1;
        }
        iterations.push(GlobalRoutingIteration {
            iteration,
            overflow: total_overflow,
            rerouted_nets: conflicted.len(),
            best_so_far: improved,
        });
    }
    let mut output = best_routes
        .into_values()
        .map(|route| GlobalNetRoute {
            net: route.plan.net,
            name: route.plan.name,
            class: route.plan.class,
            priority: route.plan.priority,
            segments: route
                .edges
                .into_iter()
                .map(|edge| GlobalRouteSegment {
                    from_bin: edge.from,
                    to_bin: edge.to,
                    layer: edge.layer,
                })
                .collect(),
            estimated_length_um: route.length,
            via_count: route.vias,
            rip_up_count: route.rip_ups,
        })
        .collect::<Vec<_>>();
    output.sort_by_key(|route| (route.priority, route.net));
    GlobalRoutingReport {
        routes: output,
        iterations,
        total_overflow: best_overflow,
        converged: best_overflow == 0,
        max_iterations: planning.global_route_max_iterations,
        stall_limit: planning.global_route_stall_iterations,
    }
}
