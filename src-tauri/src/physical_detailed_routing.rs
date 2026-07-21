use crate::{
    physical_global_routing::GlobalRoutingReport,
    physical_layout::{PhysicalLayer, PhysicalNet, PhysicalPin, PhysicalShape},
    physical_planning::PhysicalPlanningReport,
    technology::Technology,
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedNetRoute {
    pub net: usize,
    pub name: String,
    pub priority: u16,
    pub polygons: Vec<PhysicalShape>,
    pub pin_access_points: usize,
    pub blocked_pin_access_points: usize,
    pub wire_length_um: f64,
    pub via_count: usize,
    pub repair_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedRoutingIteration {
    pub iteration: u16,
    pub conflict_count: usize,
    pub rerouted_nets: usize,
    pub best_so_far: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedRoutingReport {
    pub routes: Vec<DetailedNetRoute>,
    pub iterations: Vec<DetailedRoutingIteration>,
    pub conflict_count: usize,
    pub blocked_pin_count: usize,
    pub converged: bool,
    pub max_iterations: u16,
    pub total_wire_length_um: f64,
    pub total_via_count: usize,
}

fn snap(value: f64, grid: f64) -> f64 {
    (value / grid).round() * grid
}

fn rectangle(
    start: (f64, f64),
    end: (f64, f64),
    layer: PhysicalLayer,
    width: f64,
    net: usize,
    grid: f64,
) -> PhysicalShape {
    let horizontal = (start.1 - end.1).abs() <= grid / 2.0;
    let left = snap(start.0.min(end.0) - width / 2.0, grid);
    let right = snap(start.0.max(end.0) + width / 2.0, grid);
    let top = snap(start.1.min(end.1) - width / 2.0, grid);
    let bottom = snap(start.1.max(end.1) + width / 2.0, grid);
    PhysicalShape {
        layer,
        x: (left + right) / 2.0,
        y: (top + bottom) / 2.0,
        width: if horizontal { right - left } else { width },
        height: if horizontal { width } else { bottom - top },
        component_id: None,
        net: Some(net),
    }
}

fn edge_key(from: usize, to: usize, layer: u16) -> (usize, usize, u16) {
    (from.min(to), from.max(to), layer)
}

fn build_routes(
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    global: &GlobalRoutingReport,
    planning: &PhysicalPlanningReport,
    technology: &Technology,
    offsets: &HashMap<usize, usize>,
) -> Vec<DetailedNetRoute> {
    let grid = technology.physical_rules.manufacturing_grid_um;
    let mut edge_users = BTreeMap::<(usize, usize, u16), Vec<usize>>::new();
    for route in &global.routes {
        for segment in &route.segments {
            edge_users
                .entry(edge_key(segment.from_bin, segment.to_bin, segment.layer))
                .or_default()
                .push(route.net);
        }
    }
    for users in edge_users.values_mut() {
        users.sort_unstable();
        users.dedup();
    }
    let names = nets
        .iter()
        .map(|net| (net.id, net.name.as_str()))
        .collect::<HashMap<_, _>>();
    let mut routes = Vec::new();
    for global_route in &global.routes {
        let mut polygons = Vec::new();
        let mut wire_length = 0.0;
        let mut previous: Option<(usize, u16, (f64, f64))> = None;
        let repair_offset = offsets.get(&global_route.net).copied().unwrap_or(0);
        for segment in &global_route.segments {
            let from = &planning.routing_bins[segment.from_bin];
            let to = &planning.routing_bins[segment.to_bin];
            let layer_plan = planning
                .routing_layers
                .iter()
                .find(|layer| layer.layer == segment.layer)
                .expect("global route layer belongs to the process plan");
            let width = technology
                .physical_rules
                .layer_overrides
                .get(&format!("metal{}", segment.layer))
                .unwrap_or(&technology.physical_rules.metal)
                .min_width_um;
            let key = edge_key(segment.from_bin, segment.to_bin, segment.layer);
            let users = &edge_users[&key];
            let base_slot = users
                .iter()
                .position(|net| *net == global_route.net)
                .unwrap_or_default();
            let slot = base_slot + repair_offset;
            let centered = slot as f64 - users.len().saturating_sub(1) as f64 / 2.0;
            let delta = centered * layer_plan.pitch_um;
            let mut start = (
                (from.min_x + from.max_x) / 2.0,
                (from.min_y + from.max_y) / 2.0,
            );
            let mut end = ((to.min_x + to.max_x) / 2.0, (to.min_y + to.max_y) / 2.0);
            if from.row == to.row {
                start.1 += delta;
                end.1 += delta;
            } else {
                start.0 += delta;
                end.0 += delta;
            }
            start = (snap(start.0, grid), snap(start.1, grid));
            end = (snap(end.0, grid), snap(end.1, grid));
            wire_length += (end.0 - start.0).abs() + (end.1 - start.1).abs();
            polygons.push(rectangle(
                start,
                end,
                PhysicalLayer::Metal(segment.layer),
                snap(width.max(grid), grid).max(grid),
                global_route.net,
                grid,
            ));
            if let Some((bin, last_layer, point)) = previous {
                if bin == segment.from_bin && last_layer != segment.layer {
                    for lower in last_layer.min(segment.layer)..last_layer.max(segment.layer) {
                        let via = &technology.physical_rules.via;
                        polygons.push(PhysicalShape {
                            layer: PhysicalLayer::Via(lower),
                            x: point.0,
                            y: point.1,
                            width: via.size_um,
                            height: via.size_um,
                            component_id: None,
                            net: Some(global_route.net),
                        });
                    }
                }
            }
            previous = Some((segment.to_bin, segment.layer, end));
        }
        let pin_access_points = pins
            .iter()
            .filter(|pin| pin.net == global_route.net)
            .count()
            + nets
                .iter()
                .find(|net| net.id == global_route.net)
                .map(|net| net.terminals.len())
                .unwrap_or_default();
        // A guide with no bin edge is a local, same-bin connection. Its
        // terminals remain valid access points and require no global track.
        let blocked = 0;
        let via_count = polygons
            .iter()
            .filter(|shape| matches!(shape.layer, PhysicalLayer::Via(_)))
            .count();
        routes.push(DetailedNetRoute {
            net: global_route.net,
            name: names
                .get(&global_route.net)
                .copied()
                .unwrap_or("net")
                .into(),
            priority: global_route.priority,
            polygons,
            pin_access_points,
            blocked_pin_access_points: blocked,
            wire_length_um: wire_length,
            via_count,
            repair_count: repair_offset,
        });
    }
    routes.sort_by_key(|route| (route.priority, route.net));
    routes
}

fn bounds(shape: &PhysicalShape) -> (f64, f64, f64, f64) {
    (
        shape.x - shape.width / 2.0,
        shape.y - shape.height / 2.0,
        shape.x + shape.width / 2.0,
        shape.y + shape.height / 2.0,
    )
}

fn conflicts(routes: &[DetailedNetRoute], spacing: f64) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    for left_route in 0..routes.len() {
        for right_route in (left_route + 1)..routes.len() {
            let conflict = routes[left_route].polygons.iter().any(|left| {
                routes[right_route].polygons.iter().any(|right| {
                    if left.layer != right.layer {
                        return false;
                    }
                    let (ll, lt, lr, lb) = bounds(left);
                    let (rl, rt, rr, rb) = bounds(right);
                    ll < rr + spacing && lr + spacing > rl && lt < rb + spacing && lb + spacing > rt
                })
            });
            if conflict {
                result.push((routes[left_route].net, routes[right_route].net));
            }
        }
    }
    result
}

pub fn route(
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    global: &GlobalRoutingReport,
    planning: &PhysicalPlanningReport,
    technology: &Technology,
) -> DetailedRoutingReport {
    let mut offsets = HashMap::new();
    let mut routes = build_routes(nets, pins, global, planning, technology, &offsets);
    let mut current_conflicts = conflicts(&routes, technology.physical_rules.metal.min_spacing_um);
    let mut best_routes = routes.clone();
    let mut best_count = current_conflicts.len();
    let mut iterations = vec![DetailedRoutingIteration {
        iteration: 0,
        conflict_count: best_count,
        rerouted_nets: routes.len(),
        best_so_far: true,
    }];
    for iteration in 1..=planning.detailed_route_max_iterations {
        if current_conflicts.is_empty() {
            break;
        }
        let mut rerouted = current_conflicts
            .iter()
            .map(|(left, right)| (*left).max(*right))
            .collect::<Vec<_>>();
        rerouted.sort_unstable();
        rerouted.dedup();
        for net in &rerouted {
            *offsets.entry(*net).or_default() += 1;
        }
        routes = build_routes(nets, pins, global, planning, technology, &offsets);
        current_conflicts = conflicts(&routes, technology.physical_rules.metal.min_spacing_um);
        let improved = current_conflicts.len() < best_count;
        if improved {
            best_count = current_conflicts.len();
            best_routes = routes.clone();
        }
        iterations.push(DetailedRoutingIteration {
            iteration,
            conflict_count: current_conflicts.len(),
            rerouted_nets: rerouted.len(),
            best_so_far: improved,
        });
    }
    let blocked_pin_count = best_routes
        .iter()
        .map(|route| route.blocked_pin_access_points)
        .sum();
    DetailedRoutingReport {
        total_wire_length_um: best_routes.iter().map(|route| route.wire_length_um).sum(),
        total_via_count: best_routes.iter().map(|route| route.via_count).sum(),
        routes: best_routes,
        iterations,
        conflict_count: best_count,
        blocked_pin_count,
        converged: best_count == 0 && blocked_pin_count == 0,
        max_iterations: planning.detailed_route_max_iterations,
    }
}
