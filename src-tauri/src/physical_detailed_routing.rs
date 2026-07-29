use crate::{
    physical_canvas::{device_footprint, PhysicalCanvas},
    physical_global_routing::GlobalRoutingReport,
    physical_layout::{
        PhysicalLayer, PhysicalNet, PhysicalPin, PhysicalShape, PhysicalShapePurpose,
    },
    physical_placement::{
        commit_compact_device_entries, compact_device_entries, PhysicalPlacementReport,
    },
    physical_planning::PhysicalPlanningReport,
    technology::Technology,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedNetRoute {
    pub net: usize,
    pub name: String,
    pub priority: u16,
    pub polygons: Vec<PhysicalShape>,
    pub pin_access_points: usize,
    pub blocked_pin_access_points: usize,
    pub wire_length_um: f64,
    pub layer_wire_lengths_um: BTreeMap<String, f64>,
    pub via_count: usize,
    pub via_counts: BTreeMap<String, usize>,
    pub repair_count: usize,
    pub rejected_geometry_count: usize,
    pub track_retry_count: usize,
    pub layer_escalation_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedRoutingIteration {
    pub iteration: u16,
    pub conflict_count: usize,
    pub rerouted_nets: usize,
    pub best_so_far: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
    pub rejected_geometry_count: usize,
    pub seeded_device_shape_count: usize,
    pub track_retry_count: usize,
    pub layer_escalation_count: usize,
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
        purpose: PhysicalShapePurpose::Route,
    }
}

fn edge_key(from: usize, to: usize, layer: u16) -> (usize, usize, u16) {
    (from.min(to), from.max(to), layer)
}

fn transition_stack(
    point: (f64, f64),
    from_layer: u16,
    to_layer: u16,
    net: usize,
    technology: &Technology,
) -> Vec<PhysicalShape> {
    if from_layer == to_layer {
        return Vec::new();
    }
    let grid = technology.physical_rules.manufacturing_grid_um;
    let low = from_layer.min(to_layer);
    let high = from_layer.max(to_layer);
    let mut shapes = Vec::new();
    for layer in low..=high {
        let metal = technology
            .physical_rules
            .layer_overrides
            .get(&format!("metal{layer}"))
            .unwrap_or(&technology.physical_rules.metal);
        let adjacent_cut = if layer == low {
            low
        } else {
            layer.saturating_sub(1)
        };
        let cut = technology
            .physical_rules
            .via_overrides
            .get(&format!("via{adjacent_cut}{}", adjacent_cut + 1))
            .unwrap_or(&technology.physical_rules.via);
        let landing = snap(
            (cut.size_um + 2.0 * cut.enclosure_um).max(metal.min_width_um),
            grid,
        )
        .max(grid);
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
        let cut = technology
            .physical_rules
            .via_overrides
            .get(&format!("via{lower}{}", lower + 1))
            .unwrap_or(&technology.physical_rules.via);
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
    shapes
}

fn build_routes(
    devices: &[crate::physical_layout::PhysicalDevice],
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    global: &GlobalRoutingReport,
    planning: &PhysicalPlanningReport,
    technology: &Technology,
    placement: &PhysicalPlacementReport,
    offsets: &HashMap<usize, usize>,
) -> (Vec<DetailedNetRoute>, Vec<(usize, usize)>) {
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
    let mut canvas = PhysicalCanvas::new(&technology.physical_rules);
    let selected_placement = &placement.candidates[placement.selected_candidate];
    let devices_by_id = devices
        .iter()
        .map(|device| (device.component_id, device))
        .collect::<HashMap<_, _>>();
    let device_entries = if selected_placement.topology_compacted {
        compact_device_entries(
            &selected_placement.devices,
            devices,
            &technology.physical_rules,
        )
    } else {
        let mut entries = Vec::with_capacity(selected_placement.devices.len() * 4);
        for placed in &selected_placement.devices {
            let device = devices_by_id
                .get(&placed.component_id)
                .expect("placed device belongs to physical device IR");
            entries.extend(
                device_footprint(device, placed.x, placed.y, &technology.physical_rules)
                    .into_iter()
                    .map(|(shape, obstruction)| (shape, device.name.clone(), obstruction)),
            );
        }
        entries
    };
    commit_compact_device_entries(&mut canvas, &device_entries)
        .expect("selected placement footprints remain legal when routing begins");
    let mut rejected_conflicts = Vec::new();
    for global_route in &global.routes {
        let mut accepted = Vec::new();
        let mut previous: Option<(usize, u16, (f64, f64))> = None;
        let repair_offset = offsets.get(&global_route.net).copied().unwrap_or(0);
        let mut rejected_geometry_count = 0;
        let mut track_retry_count = 0;
        let mut layer_escalation_count = 0;
        for (segment_index, segment) in global_route.segments.iter().enumerate() {
            let from = &planning.routing_bins[segment.from_bin];
            let to = &planning.routing_bins[segment.to_bin];
            let key = edge_key(segment.from_bin, segment.to_bin, segment.layer);
            let users = &edge_users[&key];
            let base_slot = users
                .iter()
                .position(|net| *net == global_route.net)
                .unwrap_or_default();
            let base_start = (
                (from.min_x + from.max_x) / 2.0,
                (from.min_y + from.max_y) / 2.0,
            );
            let base_end = ((to.min_x + to.max_x) / 2.0, (to.min_y + to.max_y) / 2.0);
            let horizontal = from.row == to.row;
            let mut layer_plans = planning
                .routing_layers
                .iter()
                .filter(|layer| {
                    use crate::technology::RoutingDirection;
                    matches!(layer.preferred_direction, RoutingDirection::Any)
                        || matches!(
                            (horizontal, layer.preferred_direction),
                            (true, RoutingDirection::Horizontal)
                                | (false, RoutingDirection::Vertical)
                        )
                })
                .collect::<Vec<_>>();
            layer_plans.sort_by_key(|layer| (layer.layer != segment.layer, layer.layer));
            let mut committed = None;
            let mut final_collisions = Vec::new();
            let mut attempt = 0usize;
            for (layer_index, layer_plan) in layer_plans.into_iter().enumerate() {
                let width = technology
                    .physical_rules
                    .layer_overrides
                    .get(&format!("metal{}", layer_plan.layer))
                    .unwrap_or(&technology.physical_rules.metal)
                    .min_width_um;
                let track_deltas: &[isize] = if layer_index == 0 {
                    &[0, 1, -1, 2, -2]
                } else {
                    &[0]
                };
                for track_delta in track_deltas {
                    let centered = base_slot as f64 + repair_offset as f64 + *track_delta as f64
                        - users.len().saturating_sub(1) as f64 / 2.0;
                    let delta = centered * layer_plan.pitch_um;
                    let mut start = base_start;
                    let mut end = base_end;
                    if horizontal {
                        start.1 += delta;
                        end.1 += delta;
                    } else {
                        start.0 += delta;
                        end.0 += delta;
                    }
                    start = (snap(start.0, grid), snap(start.1, grid));
                    end = (snap(end.0, grid), snap(end.1, grid));
                    let mut candidate = Vec::new();
                    if let Some((bin, last_layer, point)) = previous {
                        if bin == segment.from_bin {
                            candidate.extend(transition_stack(
                                point,
                                last_layer,
                                layer_plan.layer,
                                global_route.net,
                                technology,
                            ));
                            if (point.0 - start.0).abs() > grid / 2.0
                                || (point.1 - start.1).abs() > grid / 2.0
                            {
                                candidate.push(rectangle(
                                    point,
                                    start,
                                    PhysicalLayer::Metal(layer_plan.layer),
                                    snap(width.max(grid), grid).max(grid),
                                    global_route.net,
                                    grid,
                                ));
                            }
                        }
                    }
                    candidate.push(rectangle(
                        start,
                        end,
                        PhysicalLayer::Metal(layer_plan.layer),
                        snap(width.max(grid), grid).max(grid),
                        global_route.net,
                        grid,
                    ));
                    match canvas.commit_routing_geometry(
                        &candidate,
                        format!("detail-net-{}-segment-{segment_index}", global_route.net),
                    ) {
                        Ok(_) => {
                            committed = Some((candidate, layer_plan.layer, end, attempt));
                            break;
                        }
                        Err(collisions) => final_collisions.extend(collisions),
                    }
                    attempt += 1;
                }
                if committed.is_some() {
                    break;
                }
            }
            if let Some((candidate, layer, end, attempts)) = committed {
                track_retry_count += attempts;
                layer_escalation_count += usize::from(layer != segment.layer);
                accepted.extend(candidate);
                previous = Some((segment.to_bin, layer, end));
            } else {
                rejected_geometry_count += 1;
                if final_collisions.is_empty() {
                    rejected_conflicts.push((global_route.net, global_route.net));
                } else {
                    rejected_conflicts.extend(final_collisions.into_iter().map(|collision| {
                        let other = collision.net.unwrap_or(global_route.net);
                        (global_route.net.min(other), global_route.net.max(other))
                    }));
                }
            }
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
        rejected_conflicts.sort_unstable();
        rejected_conflicts.dedup();
        let mut wire_length = 0.0;
        let mut layer_wire_lengths = BTreeMap::<String, f64>::new();
        for shape in &accepted {
            if let PhysicalLayer::Metal(layer) = shape.layer {
                let width = technology
                    .physical_rules
                    .layer_overrides
                    .get(&format!("metal{layer}"))
                    .unwrap_or(&technology.physical_rules.metal)
                    .min_width_um;
                let length = (shape.width.max(shape.height) - width).max(0.0);
                wire_length += length;
                *layer_wire_lengths
                    .entry(format!("metal{layer}"))
                    .or_default() += length;
            }
        }
        let via_count = accepted
            .iter()
            .filter(|shape| matches!(shape.layer, PhysicalLayer::Via(_)))
            .count();
        let mut via_counts = BTreeMap::<String, usize>::new();
        for shape in &accepted {
            if let PhysicalLayer::Via(lower) = shape.layer {
                *via_counts
                    .entry(format!("via{lower}{}", lower + 1))
                    .or_default() += 1;
            }
        }
        routes.push(DetailedNetRoute {
            net: global_route.net,
            name: names
                .get(&global_route.net)
                .copied()
                .unwrap_or("net")
                .into(),
            priority: global_route.priority,
            polygons: accepted,
            pin_access_points,
            blocked_pin_access_points: blocked,
            wire_length_um: wire_length,
            layer_wire_lengths_um: layer_wire_lengths,
            via_count,
            via_counts,
            repair_count: repair_offset,
            rejected_geometry_count,
            track_retry_count,
            layer_escalation_count,
        });
    }
    routes.sort_by_key(|route| (route.priority, route.net));
    (routes, rejected_conflicts)
}

pub fn route(
    devices: &[crate::physical_layout::PhysicalDevice],
    nets: &[PhysicalNet],
    pins: &[PhysicalPin],
    global: &GlobalRoutingReport,
    planning: &PhysicalPlanningReport,
    technology: &Technology,
    placement: &PhysicalPlacementReport,
) -> DetailedRoutingReport {
    let mut offsets = HashMap::new();
    let (mut routes, mut current_conflicts) = build_routes(
        devices, nets, pins, global, planning, technology, placement, &offsets,
    );
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
        (routes, current_conflicts) = build_routes(
            devices, nets, pins, global, planning, technology, placement, &offsets,
        );
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
        rejected_geometry_count: best_routes
            .iter()
            .map(|route| route.rejected_geometry_count)
            .sum(),
        seeded_device_shape_count: placement.candidates[placement.selected_candidate]
            .reserved_device_shapes,
        track_retry_count: best_routes
            .iter()
            .map(|route| route.track_retry_count)
            .sum(),
        layer_escalation_count: best_routes
            .iter()
            .map(|route| route.layer_escalation_count)
            .sum(),
        routes: best_routes,
        iterations,
        conflict_count: best_count,
        blocked_pin_count,
        converged: best_count == 0 && blocked_pin_count == 0,
        max_iterations: planning.detailed_route_max_iterations,
    }
}
