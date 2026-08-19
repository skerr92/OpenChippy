use crate::{
    physical_canvas::{ObstructionType, PhysicalCanvas},
    physical_layout::{
        DensitySpatialIndex, PhysicalLayer, PhysicalLayoutIr, PhysicalRowTopology, PhysicalShape,
        PhysicalShapePurpose,
    },
    technology::{CutRule, LayerRule, Technology},
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

const EPSILON: f64 = 1e-7;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum PhysicalDrcSeverity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum PhysicalDrcCategory {
    DeviceOverlap,
    MetalOverlap,
    MinimumSpacing,
    ViaEnclosure,
    PowerCollision,
    RoutingCongestion,
    BoundaryViolation,
    Geometry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum PhysicalDrcOrigin {
    Placement,
    PowerRouting,
    SignalRouting,
    GeometryGeneration,
    Import,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDrcDiagnostic {
    pub rule_id: String,
    pub severity: PhysicalDrcSeverity,
    pub message: String,
    pub layer: String,
    pub shape_indices: Vec<usize>,
    pub measured: f64,
    pub required: f64,
    pub category: PhysicalDrcCategory,
    pub origin: PhysicalDrcOrigin,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDrcReport {
    pub diagnostics: Vec<PhysicalDrcDiagnostic>,
    pub error_count: usize,
    pub warning_count: usize,
    pub by_category: BTreeMap<String, usize>,
    pub by_origin: BTreeMap<String, usize>,
}

impl PhysicalDrcReport {
    fn push(
        &mut self,
        rule_id: &str,
        message: String,
        layer: PhysicalLayer,
        shape_indices: Vec<usize>,
        measured: f64,
        required: f64,
    ) {
        let (category, origin) = classify(rule_id, layer, measured);
        self.diagnostics.push(PhysicalDrcDiagnostic {
            rule_id: rule_id.into(),
            severity: PhysicalDrcSeverity::Error,
            message,
            layer: layer_name(layer),
            shape_indices,
            measured,
            required,
            category,
            origin,
        });
        *self
            .by_category
            .entry(category_key(category).into())
            .or_default() += 1;
        *self.by_origin.entry(origin_key(origin).into()).or_default() += 1;
        self.error_count += 1;
    }
}

fn classify(
    rule_id: &str,
    layer: PhysicalLayer,
    measured: f64,
) -> (PhysicalDrcCategory, PhysicalDrcOrigin) {
    if rule_id.contains("BOUNDARY") {
        return (
            PhysicalDrcCategory::BoundaryViolation,
            PhysicalDrcOrigin::Placement,
        );
    }
    if rule_id.contains("ENCLOSURE")
        && matches!(layer, PhysicalLayer::Contact | PhysicalLayer::Via(_))
    {
        return (
            PhysicalDrcCategory::ViaEnclosure,
            PhysicalDrcOrigin::SignalRouting,
        );
    }
    if rule_id.ends_with("MIN_SPACING") {
        return match layer {
            PhysicalLayer::Metal(_) if measured <= EPSILON => (
                PhysicalDrcCategory::MetalOverlap,
                PhysicalDrcOrigin::SignalRouting,
            ),
            PhysicalLayer::Metal(_) | PhysicalLayer::Via(_) | PhysicalLayer::Contact => (
                PhysicalDrcCategory::MinimumSpacing,
                PhysicalDrcOrigin::SignalRouting,
            ),
            _ if measured <= EPSILON => (
                PhysicalDrcCategory::DeviceOverlap,
                PhysicalDrcOrigin::Placement,
            ),
            _ => (
                PhysicalDrcCategory::MinimumSpacing,
                PhysicalDrcOrigin::Placement,
            ),
        };
    }
    let origin = match layer {
        PhysicalLayer::Nwell
        | PhysicalLayer::Ndiff
        | PhysicalLayer::Pdiff
        | PhysicalLayer::Poly => PhysicalDrcOrigin::Placement,
        PhysicalLayer::Metal(_) | PhysicalLayer::Via(_) | PhysicalLayer::Contact => {
            PhysicalDrcOrigin::SignalRouting
        }
        PhysicalLayer::Substrate | PhysicalLayer::Pwell => PhysicalDrcOrigin::GeometryGeneration,
    };
    (PhysicalDrcCategory::Geometry, origin)
}

fn category_key(category: PhysicalDrcCategory) -> &'static str {
    match category {
        PhysicalDrcCategory::DeviceOverlap => "deviceOverlap",
        PhysicalDrcCategory::MetalOverlap => "metalOverlap",
        PhysicalDrcCategory::MinimumSpacing => "minimumSpacing",
        PhysicalDrcCategory::ViaEnclosure => "viaEnclosure",
        PhysicalDrcCategory::PowerCollision => "powerCollision",
        PhysicalDrcCategory::RoutingCongestion => "routingCongestion",
        PhysicalDrcCategory::BoundaryViolation => "boundaryViolation",
        PhysicalDrcCategory::Geometry => "geometry",
    }
}

fn origin_key(origin: PhysicalDrcOrigin) -> &'static str {
    match origin {
        PhysicalDrcOrigin::Placement => "placement",
        PhysicalDrcOrigin::PowerRouting => "powerRouting",
        PhysicalDrcOrigin::SignalRouting => "signalRouting",
        PhysicalDrcOrigin::GeometryGeneration => "geometryGeneration",
        PhysicalDrcOrigin::Import => "import",
    }
}

fn layer_name(layer: PhysicalLayer) -> String {
    match layer {
        PhysicalLayer::Substrate => "substrate".into(),
        PhysicalLayer::Pwell => "pwell".into(),
        PhysicalLayer::Nwell => "nwell".into(),
        PhysicalLayer::Ndiff => "ndiff".into(),
        PhysicalLayer::Pdiff => "pdiff".into(),
        PhysicalLayer::Poly => "poly".into(),
        PhysicalLayer::Metal(index) => format!("metal{index}"),
        PhysicalLayer::Contact => "contact".into(),
        PhysicalLayer::Via(lower) => format!("via{lower}{}", lower + 1),
    }
}

fn layer_rule<'a>(layer: PhysicalLayer, technology: &'a Technology) -> Option<&'a LayerRule> {
    let rules = &technology.physical_rules;
    match layer {
        PhysicalLayer::Pwell | PhysicalLayer::Nwell => Some(&rules.well),
        PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => Some(&rules.diffusion),
        PhysicalLayer::Poly => Some(&rules.poly),
        PhysicalLayer::Metal(index) => rules
            .layer_overrides
            .get(&format!("metal{index}"))
            .or(Some(&rules.metal)),
        _ => None,
    }
}

fn cut_rule<'a>(layer: PhysicalLayer, technology: &'a Technology) -> Option<&'a CutRule> {
    match layer {
        PhysicalLayer::Contact => Some(&technology.physical_rules.contact),
        PhysicalLayer::Via(lower) => technology
            .physical_rules
            .via_overrides
            .get(&format!("via{lower}{}", lower + 1))
            .or(Some(&technology.physical_rules.via)),
        _ => None,
    }
}

fn density_fill_rule<'a>(
    shape: &PhysicalShape,
    technology: &'a Technology,
) -> Option<(&'a str, &'a crate::technology::DensityFillLayerRule)> {
    if shape.purpose != PhysicalShapePurpose::DummyFill {
        return None;
    }
    let rules = &technology.physical_rules.density_fill.layers;
    let name = match shape.layer {
        PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => "active",
        PhysicalLayer::Poly => "poly",
        PhysicalLayer::Metal(index) if index == technology.max_metal_layers + 1 => "top_metal",
        PhysicalLayer::Metal(index) => {
            return rules
                .get_key_value(&format!("metal{index}"))
                .map(|(name, rule)| (name.as_str(), rule))
        }
        _ => return None,
    };
    rules
        .get_key_value(name)
        .map(|(name, rule)| (name.as_str(), rule))
}

fn density_fill_circuit_obstacle(
    material: &str,
    fill_layer: PhysicalLayer,
    circuit_layer: PhysicalLayer,
) -> bool {
    match material {
        "active" | "poly" => matches!(
            circuit_layer,
            PhysicalLayer::Ndiff
                | PhysicalLayer::Pdiff
                | PhysicalLayer::Poly
                | PhysicalLayer::Nwell
                | PhysicalLayer::Pwell
        ),
        "top_metal" => false,
        _ => fill_layer == circuit_layer,
    }
}

fn edges(shape: &PhysicalShape) -> (f64, f64, f64, f64) {
    (
        shape.x - shape.width / 2.0,
        shape.y - shape.height / 2.0,
        shape.x + shape.width / 2.0,
        shape.y + shape.height / 2.0,
    )
}

fn contains(outer: &PhysicalShape, inner: &PhysicalShape, enclosure: f64) -> bool {
    let (ol, ot, or, ob) = edges(outer);
    let (il, it, ir, ib) = edges(inner);
    ol <= il - enclosure + EPSILON
        && ot <= it - enclosure + EPSILON
        && or >= ir + enclosure - EPSILON
        && ob >= ib + enclosure - EPSILON
}

fn overlaps(left: &PhysicalShape, right: &PhysicalShape) -> bool {
    let (ll, lt, lr, lb) = edges(left);
    let (rl, rt, rr, rb) = edges(right);
    ll < rr - EPSILON && lr > rl + EPSILON && lt < rb - EPSILON && lb > rt + EPSILON
}

fn shapes_touch(left: &PhysicalShape, right: &PhysicalShape) -> bool {
    let (ll, lt, lr, lb) = edges(left);
    let (rl, rt, rr, rb) = edges(right);
    ll <= rr + EPSILON && lr + EPSILON >= rl && lt <= rb + EPSILON && lb + EPSILON >= rt
}

fn spacing(left: &PhysicalShape, right: &PhysicalShape) -> f64 {
    let (ll, lt, lr, lb) = edges(left);
    let (rl, rt, rr, rb) = edges(right);
    let dx = (ll - rr).max(rl - lr).max(0.0);
    let dy = (lt - rb).max(rt - lb).max(0.0);
    dx.hypot(dy)
}

fn aligned(value: f64, grid: f64) -> bool {
    ((value / grid) - (value / grid).round()).abs() <= EPSILON
}

fn obstruction_type(layer: PhysicalLayer) -> ObstructionType {
    match layer {
        PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => ObstructionType::Diffusion,
        PhysicalLayer::Poly => ObstructionType::Poly,
        PhysicalLayer::Contact => ObstructionType::Contact,
        PhysicalLayer::Metal(_) => ObstructionType::Metal,
        PhysicalLayer::Via(_) => ObstructionType::Via,
        PhysicalLayer::Pwell | PhysicalLayer::Nwell | PhysicalLayer::Substrate => {
            ObstructionType::Device
        }
    }
}

pub fn validate(ir: &PhysicalLayoutIr, technology: &Technology) -> PhysicalDrcReport {
    let mut report = validate_shape_set(
        &ir.shapes,
        ir.max_metal_layers,
        &ir.nets,
        &ir.row_topology,
        technology,
    );
    validate_device_terminal_attachments(ir, &mut report);
    validate_active_island_geometry(ir, &mut report);
    validate_gate_strap_membership(ir, &mut report);
    validate_terminal_connectivity(&ir.shapes, &mut report);
    validate_logical_terminal_obligations(ir, &mut report);
    validate_route_endpoints(ir, &mut report);
    report
}

fn validate_active_island_geometry(ir: &PhysicalLayoutIr, report: &mut PhysicalDrcReport) {
    for island in ir.row_topology.iter().flat_map(|row| &row.islands) {
        let expected = island
            .geometry
            .iter()
            .chain(island.geometry.is_empty().then_some(&island.bounds))
            .collect::<Vec<_>>();
        let shared = island.device_ids.len() > 1;
        let shape_indices = expected
            .iter()
            .filter_map(|bounds| {
                ir.shapes.iter().enumerate().find_map(|(index, shape)| {
                    (shape.layer == island.layer
                        && shape.purpose == PhysicalShapePurpose::Active
                        && if shared {
                            shape.component_id.is_none()
                        } else {
                            shape.component_id == island.device_ids.first().copied()
                        }
                        && bounds_match_shape(bounds, shape))
                    .then_some(index)
                })
            })
            .collect::<Vec<_>>();
        let complete_geometry = !expected.is_empty() && shape_indices.len() == expected.len();
        let mut connected = complete_geometry;
        if connected {
            let mut reached = HashSet::from([0usize]);
            let mut pending = vec![0usize];
            while let Some(current) = pending.pop() {
                for candidate in 0..expected.len() {
                    if !reached.contains(&candidate)
                        && bounds_touch(expected[current], expected[candidate])
                    {
                        reached.insert(candidate);
                        pending.push(candidate);
                    }
                }
            }
            connected = reached.len() == expected.len();
        }
        let unreached_devices = island
            .device_ids
            .iter()
            .filter(|device_id| {
                !ir.shapes.iter().any(|gate| {
                    gate.component_id == Some(**device_id)
                        && gate.layer == PhysicalLayer::Poly
                        && gate.purpose == PhysicalShapePurpose::Gate
                        && expected.iter().any(|bounds| {
                            let gate_bounds = crate::physical_layout::PhysicalBounds {
                                min_x: gate.x - gate.width / 2.0,
                                min_y: gate.y - gate.height / 2.0,
                                max_x: gate.x + gate.width / 2.0,
                                max_y: gate.y + gate.height / 2.0,
                            };
                            bounds_touch(bounds, &gate_bounds)
                        })
                })
            })
            .count();
        let unreached_accesses = island
            .accesses
            .iter()
            .filter(|access| {
                !expected.iter().any(|bounds| {
                    access.x >= bounds.min_x - EPSILON
                        && access.x <= bounds.max_x + EPSILON
                        && access.y >= bounds.min_y - EPSILON
                        && access.y <= bounds.max_y + EPSILON
                })
            })
            .count();
        if !complete_geometry || !connected || unreached_devices > 0 || unreached_accesses > 0 {
            report.push(
                "CONNECTIVITY.OPEN_ACTIVE_ISLAND",
                format!(
                    "{:?} island has incomplete physical proof: geometry_complete={}, connected={}, unreached_devices={unreached_devices}, unreached_accesses={unreached_accesses}.",
                    island.layer, complete_geometry, connected
                ),
                island.layer,
                shape_indices,
                unreached_devices
                    .saturating_add(unreached_accesses)
                    .max(usize::from(!complete_geometry || !connected))
                    as f64,
                0.0,
            );
        }
    }
}

fn bounds_touch(
    left: &crate::physical_layout::PhysicalBounds,
    right: &crate::physical_layout::PhysicalBounds,
) -> bool {
    left.min_x <= right.max_x + EPSILON
        && left.max_x + EPSILON >= right.min_x
        && left.min_y <= right.max_y + EPSILON
        && left.max_y + EPSILON >= right.min_y
}

/// A gate strap is not connectivity merely because its metadata names a set of
/// devices. Require the persisted rectilinear conductor to exist in the shape
/// set, form one connected union, and physically reach every claimed gate.
fn validate_gate_strap_membership(ir: &PhysicalLayoutIr, report: &mut PhysicalDrcReport) {
    for strap in ir.row_topology.iter().flat_map(|row| &row.gate_straps) {
        let shape_indices = strap
            .geometry
            .iter()
            .filter_map(|bounds| {
                ir.shapes.iter().enumerate().find_map(|(index, shape)| {
                    (shape.layer == PhysicalLayer::Poly
                        && shape.purpose == PhysicalShapePurpose::GateAccess
                        && shape.component_id.is_none()
                        && shape.net == Some(strap.net)
                        && bounds_match_shape(bounds, shape))
                    .then_some(index)
                })
            })
            .collect::<Vec<_>>();
        let complete_geometry =
            !strap.geometry.is_empty() && shape_indices.len() == strap.geometry.len();
        let mut connected = complete_geometry;
        if connected {
            let mut reached = HashSet::from([0usize]);
            let mut pending = vec![0usize];
            while let Some(current) = pending.pop() {
                for candidate in 0..strap.geometry.len() {
                    if !reached.contains(&candidate)
                        && bounds_touch(&strap.geometry[current], &strap.geometry[candidate])
                    {
                        reached.insert(candidate);
                        pending.push(candidate);
                    }
                }
            }
            connected = reached.len() == strap.geometry.len();
        }

        let missing_members = strap
            .device_ids
            .iter()
            .filter(|device_id| {
                let gate = ir.shapes.iter().find(|shape| {
                    shape.component_id == Some(**device_id)
                        && shape.layer == PhysicalLayer::Poly
                        && shape.purpose == PhysicalShapePurpose::Gate
                        && shape.net == Some(strap.net)
                });
                !gate.is_some_and(|gate| {
                    ir.shapes.iter().any(|access| {
                        access.component_id == Some(**device_id)
                            && access.layer == PhysicalLayer::Poly
                            && access.purpose == PhysicalShapePurpose::GateAccess
                            && access.net == Some(strap.net)
                            && shapes_touch(access, gate)
                            && strap.geometry.iter().any(|bounds| {
                                let access_bounds = crate::physical_layout::PhysicalBounds {
                                    min_x: access.x - access.width / 2.0,
                                    min_y: access.y - access.height / 2.0,
                                    max_x: access.x + access.width / 2.0,
                                    max_y: access.y + access.height / 2.0,
                                };
                                bounds_touch(bounds, &access_bounds)
                            })
                    })
                })
            })
            .count();
        if !complete_geometry || !connected || missing_members > 0 {
            report.push(
                "CONNECTIVITY.OPEN_GATE_STRAP",
                format!(
                    "Gate net {} strap has incomplete physical proof: geometry_complete={}, connected={}, unreached_devices={missing_members}.",
                    strap.net, complete_geometry, connected
                ),
                PhysicalLayer::Poly,
                shape_indices,
                missing_members.max(usize::from(!complete_geometry || !connected)) as f64,
                0.0,
            );
        }
    }
}

fn validate_route_endpoints(ir: &PhysicalLayoutIr, report: &mut PhysicalDrcReport) {
    for endpoint in &ir.route_quality.unjustified_route_endpoints {
        let Some(shape) = ir.shapes.get(endpoint.shape_index) else {
            continue;
        };
        report.push(
            "CONNECTIVITY.DANGLING_ROUTE_ENDPOINT",
            format!(
                "Net {} route endpoint at ({:.4}, {:.4}) does not terminate on a conductor, via, device terminal, or pin.",
                endpoint.net, endpoint.x, endpoint.y
            ),
            shape.layer,
            vec![endpoint.shape_index],
            0.0,
            1.0,
        );
    }
}

pub fn validate_imported_shapes(
    shapes: &[PhysicalShape],
    max_metal_layers: u16,
    technology: &Technology,
) -> PhysicalDrcReport {
    validate_shape_set(shapes, max_metal_layers, &[], &[], technology)
}

pub fn terminal_connectivity_error_count(shapes: &[PhysicalShape]) -> usize {
    let mut report = PhysicalDrcReport::default();
    validate_terminal_connectivity(shapes, &mut report);
    report.error_count
}

/// Prove connectivity from the logical device obligations rather than from the
/// shapes that happened to survive routing. A contact plus a local landing is
/// not evidence that a transistor terminal reaches anything. Conversely, one
/// shared active contact can legitimately satisfy two adjacent source/drain
/// obligations, so the proof retains obligation multiplicity separately from
/// the number of physical anchors.
fn validate_logical_terminal_obligations(ir: &PhysicalLayoutIr, report: &mut PhysicalDrcReport) {
    let routing = ir
        .shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| {
            shape.net.is_some()
                && matches!(
                    shape.layer,
                    PhysicalLayer::Metal(_)
                        | PhysicalLayer::Via(_)
                        | PhysicalLayer::Poly
                        | PhysicalLayer::Contact
                )
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut adjacency = HashMap::<usize, Vec<usize>>::new();
    for (position, left_index) in routing.iter().enumerate() {
        for right_index in routing.iter().skip(position + 1) {
            let left = &ir.shapes[*left_index];
            let right = &ir.shapes[*right_index];
            if left.net != right.net || !shapes_touch(left, right) {
                continue;
            }
            let connected = match (left.layer, right.layer) {
                (PhysicalLayer::Metal(left), PhysicalLayer::Metal(right)) => left == right,
                (PhysicalLayer::Via(lower), PhysicalLayer::Metal(metal))
                | (PhysicalLayer::Metal(metal), PhysicalLayer::Via(lower)) => {
                    metal == lower || metal == lower + 1
                }
                (PhysicalLayer::Poly, PhysicalLayer::Poly)
                | (PhysicalLayer::Contact, PhysicalLayer::Contact) => true,
                (PhysicalLayer::Contact, PhysicalLayer::Metal(1))
                | (PhysicalLayer::Metal(1), PhysicalLayer::Contact)
                | (PhysicalLayer::Contact, PhysicalLayer::Poly)
                | (PhysicalLayer::Poly, PhysicalLayer::Contact) => true,
                _ => false,
            };
            if connected {
                adjacency.entry(*left_index).or_default().push(*right_index);
                adjacency.entry(*right_index).or_default().push(*left_index);
            }
        }
    }

    // Each entry is (physical anchor, number of logical terminals represented
    // by that anchor). Shared diffusion deliberately has multiplicity > 1.
    let mut obligations = BTreeMap::<usize, Vec<(usize, usize)>>::new();
    for device in &ir.devices {
        let gate_anchor = ir.shapes.iter().enumerate().find_map(|(index, contact)| {
            (contact.component_id == Some(device.component_id)
                && contact.layer == PhysicalLayer::Contact
                && contact.net == Some(device.gate_net)
                && ir.shapes.iter().any(|access| {
                    access.component_id == Some(device.component_id)
                        && access.layer == PhysicalLayer::Poly
                        && access.net == Some(device.gate_net)
                        && shapes_touch(access, contact)
                        && ir.shapes.iter().any(|gate| {
                            gate.component_id == Some(device.component_id)
                                && gate.layer == PhysicalLayer::Poly
                                && gate.purpose == PhysicalShapePurpose::Gate
                                && gate.net == Some(device.gate_net)
                                && shapes_touch(gate, access)
                        })
                }))
            .then_some(index)
        });
        if let Some(anchor) = gate_anchor {
            obligations
                .entry(device.gate_net)
                .or_default()
                .push((anchor, 1));
        }
    }
    for access in ir
        .row_topology
        .iter()
        .flat_map(|row| &row.islands)
        .flat_map(|island| &island.accesses)
    {
        if access.shared_contact {
            let anchor = ir.shapes.iter().enumerate().find_map(|(index, shape)| {
                (shape.layer == PhysicalLayer::Contact
                    && shape.purpose == PhysicalShapePurpose::Contact
                    && shape.net == Some(access.net)
                    && shape.component_id.is_none()
                    && (shape.x - access.x).abs() <= EPSILON
                    && (shape.y - access.y).abs() <= EPSILON)
                    .then_some(index)
            });
            if let Some(anchor) = anchor {
                obligations
                    .entry(access.net)
                    .or_default()
                    .push((anchor, access.device_ids.len().max(1)));
            }
        } else {
            // When merging two local contacts into one shared cut is blocked,
            // the topology access still represents both sides of the same
            // continuous active region. Bind each obligation to its nearest
            // component-owned contact on the device row instead of pretending
            // a componentless midpoint contact exists.
            let mut access_anchors = Vec::new();
            for device_id in &access.device_ids {
                let anchor = ir
                    .shapes
                    .iter()
                    .enumerate()
                    .filter(|(_, shape)| {
                        shape.layer == PhysicalLayer::Contact
                            && shape.purpose == PhysicalShapePurpose::Contact
                            && shape.net == Some(access.net)
                            && shape.component_id == Some(*device_id)
                            && (shape.y - access.y).abs() <= EPSILON
                    })
                    .min_by(|(_, left), (_, right)| {
                        (left.x - access.x)
                            .abs()
                            .total_cmp(&(right.x - access.x).abs())
                    })
                    .map(|(index, _)| index);
                if let Some(anchor) = anchor {
                    obligations.entry(access.net).or_default().push((anchor, 1));
                    access_anchors.push(anchor);
                }
            }
            // These contacts land on the same source/drain region between
            // adjacent gates. Model only that topology-proven equivalence;
            // never connect arbitrary contacts merely because their active
            // rectangles belong to the same larger island.
            for left in &access_anchors {
                for right in &access_anchors {
                    if left != right {
                        adjacency.entry(*left).or_default().push(*right);
                    }
                }
            }
        }
    }
    for (index, pin) in ir
        .shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| shape.net.is_some() && shape.purpose == PhysicalShapePurpose::Pin)
    {
        obligations
            .entry(pin.net.expect("filtered physical pin"))
            .or_default()
            .push((index, 1));
    }
    // Power symbols currently synthesize distributed rails rather than
    // perimeter Pin shapes. Make one deterministic rail the external supply
    // obligation so a lone source terminal is accepted only when it actually
    // reaches the power fabric.
    for net in ir.nets.iter().filter(|net| {
        matches!(
            net.role,
            crate::physical_layout::NetRole::Power | crate::physical_layout::NetRole::Ground
        )
    }) {
        let rail = ir
            .shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| {
                shape.net == Some(net.id)
                    && shape.component_id.is_none()
                    && matches!(shape.layer, PhysicalLayer::Metal(_))
                    && shape.purpose == PhysicalShapePurpose::PowerRail
            })
            .max_by(|(_, left), (_, right)| {
                left.width
                    .max(left.height)
                    .total_cmp(&right.width.max(right.height))
            })
            .map(|(index, _)| index);
        if let Some(rail) = rail {
            obligations.entry(net.id).or_default().push((rail, 1));
        }
    }

    for net in &ir.nets {
        let expected_device_terminals = ir
            .devices
            .iter()
            .map(|device| {
                usize::from(device.gate_net == net.id)
                    + usize::from(device.drain_net == net.id)
                    + usize::from(device.source_net == net.id)
            })
            .sum::<usize>();
        let internal_active_terminals = ir
            .row_topology
            .iter()
            .flat_map(|row| &row.islands)
            .flat_map(|island| &island.accesses)
            .find(|access| topology_internal_access(ir, access) && access.net == net.id)
            .map_or(0, |access| access.device_ids.len());
        // Only persisted pin geometry is an electrical obligation. VDD/GND
        // schematic source symbols currently become distributed power fabric,
        // not perimeter pin shapes; counting their logical symbols here would
        // manufacture a false missing-terminal diagnostic.
        let expected_pins = ir
            .shapes
            .iter()
            .filter(|shape| shape.net == Some(net.id) && shape.purpose == PhysicalShapePurpose::Pin)
            .count();
        let expected_power_fabric = usize::from(matches!(
            net.role,
            crate::physical_layout::NetRole::Power | crate::physical_layout::NetRole::Ground
        ));
        let expected = expected_device_terminals.saturating_sub(internal_active_terminals)
            + expected_pins
            + expected_power_fabric;
        if expected == 0 {
            continue;
        }
        let anchors = obligations.get(&net.id).cloned().unwrap_or_default();
        let represented = anchors
            .iter()
            .map(|(_, multiplicity)| *multiplicity)
            .sum::<usize>();
        if represented < expected {
            report.push(
                "CONNECTIVITY.MISSING_TERMINAL_OBLIGATION",
                format!(
                    "Net {} represents {represented} of {expected} required device/pin terminals.",
                    net.name
                ),
                PhysicalLayer::Contact,
                anchors.iter().map(|(index, _)| *index).collect(),
                (expected - represented) as f64,
                0.0,
            );
            continue;
        }
        if expected < 2 {
            report.push(
                "CONNECTIVITY.SINGLE_TERMINAL_NET",
                format!(
                    "Net {} has only one logical terminal; a local contact or landing cannot prove useful connectivity.",
                    net.name
                ),
                PhysicalLayer::Contact,
                anchors.iter().map(|(index, _)| *index).collect(),
                1.0,
                2.0,
            );
            continue;
        }

        let root = anchors.first().map(|(index, _)| *index);
        let mut reached = HashSet::new();
        let mut pending = root.into_iter().collect::<Vec<_>>();
        while let Some(index) = pending.pop() {
            if !reached.insert(index) {
                continue;
            }
            pending.extend(adjacency.get(&index).into_iter().flatten().copied());
        }
        let unreached = anchors
            .iter()
            .filter(|(index, _)| !reached.contains(index))
            .map(|(_, multiplicity)| *multiplicity)
            .sum::<usize>();
        if unreached > 0 {
            let anchor_summary = anchors
                .iter()
                .filter_map(|(index, multiplicity)| {
                    ir.shapes.get(*index).map(|shape| {
                        format!(
                            "#{index}:{:?}/{}@({:.4},{:.4})x{multiplicity}",
                            shape.layer,
                            shape.purpose.as_str(),
                            shape.x,
                            shape.y
                        )
                    })
                })
                .collect::<Vec<_>>()
                .join(", ");
            report.push(
                "CONNECTIVITY.OPEN_TERMINAL_OBLIGATION",
                format!(
                    "Net {} leaves {unreached} of {expected} required device/pin terminals outside the connected conductor component. Anchors: {anchor_summary}.",
                    net.name,
                ),
                PhysicalLayer::Metal(1),
                anchors.iter().map(|(index, _)| *index).collect(),
                unreached as f64,
                0.0,
            );
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct MissingDeviceTerminalAttachment {
    pub component_id: uuid::Uuid,
    pub terminal: &'static str,
    pub shape_indices: Vec<usize>,
}

pub(crate) fn missing_device_terminal_attachments(
    ir: &PhysicalLayoutIr,
) -> Vec<MissingDeviceTerminalAttachment> {
    let mut missing = Vec::new();
    for device in &ir.devices {
        let gate_poly = ir.shapes.iter().enumerate().find(|(_, shape)| {
            shape.component_id == Some(device.component_id)
                && shape.layer == PhysicalLayer::Poly
                && shape.purpose == PhysicalShapePurpose::Gate
                && shape.net == Some(device.gate_net)
        });
        let gate_contact = gate_poly.and_then(|(_, poly)| {
            ir.shapes.iter().enumerate().find(|(_, shape)| {
                shape.component_id == Some(device.component_id)
                    && shape.layer == PhysicalLayer::Contact
                    && shape.net == Some(device.gate_net)
                    && ir.shapes.iter().any(|access| {
                        access.component_id == Some(device.component_id)
                            && access.layer == PhysicalLayer::Poly
                            && access.net == Some(device.gate_net)
                            && shapes_touch(access, poly)
                            && shapes_touch(access, shape)
                    })
                    && ir.shapes.iter().any(|landing| {
                        landing.layer == PhysicalLayer::Metal(1)
                            && landing.net == shape.net
                            && shapes_touch(landing, shape)
                    })
            })
        });
        if gate_contact.is_none() {
            missing.push(MissingDeviceTerminalAttachment {
                component_id: device.component_id,
                terminal: "gate",
                shape_indices: gate_poly.map(|(index, _)| vec![index]).unwrap_or_default(),
            });
        }

        let island = ir
            .row_topology
            .iter()
            .flat_map(|row| &row.islands)
            .find(|island| island.device_ids.contains(&device.component_id));
        let Some(island) = island else {
            for terminal in ["drain", "source"] {
                missing.push(MissingDeviceTerminalAttachment {
                    component_id: device.component_id,
                    terminal,
                    shape_indices: Vec::new(),
                });
            }
            continue;
        };
        let active = ir
            .shapes
            .iter()
            .enumerate()
            .filter(|(_, shape)| {
                shape.purpose == PhysicalShapePurpose::Active
                    && (shape.component_id == Some(device.component_id)
                        || shared_active_owns(shape, device.component_id, &ir.row_topology))
            })
            .collect::<Vec<_>>();
        for (terminal, expected_net) in [("drain", device.drain_net), ("source", device.source_net)]
        {
            let access = island
                .accesses
                .iter()
                .filter(|access| {
                    access.net == expected_net && access.device_ids.contains(&device.component_id)
                })
                .find(|access| {
                    access.net == expected_net && access.device_ids.contains(&device.component_id)
                });
            if access.is_some_and(|access| topology_internal_access(ir, access)) {
                continue;
            }
            let contact = access.and_then(|access| {
                ir.shapes.iter().enumerate().find(|(_, shape)| {
                    shape.layer == PhysicalLayer::Contact
                        && shape.purpose == PhysicalShapePurpose::Contact
                        && shape.net == Some(expected_net)
                        && (if access.shared_contact {
                            shape.component_id.is_none()
                                && (shape.x - access.x).abs() <= EPSILON
                                && (shape.y - access.y).abs() <= EPSILON
                        } else {
                            shape.component_id == Some(device.component_id)
                        })
                        && active
                            .iter()
                            .any(|(_, active_shape)| shapes_touch(shape, active_shape))
                        && ir.shapes.iter().any(|landing| {
                            landing.layer == PhysicalLayer::Metal(1)
                                && landing.net == shape.net
                                && shapes_touch(landing, shape)
                        })
                })
            });
            if contact.is_none() {
                missing.push(MissingDeviceTerminalAttachment {
                    component_id: device.component_id,
                    terminal,
                    shape_indices: active.iter().map(|(index, _)| *index).collect(),
                });
            }
        }
    }
    missing
}

/// A repeated source/drain boundary wholly contained by one persisted active
/// island is electrically closed in diffusion and must not be manufactured as
/// a contact/M1 pin. This exemption is intentionally narrow: every logical
/// terminal on the net must be one of the access members and no boundary pin
/// may expose the net outside the island.
fn topology_internal_access(
    ir: &PhysicalLayoutIr,
    access: &crate::physical_layout::PhysicalTerminalAccess,
) -> bool {
    if access.device_ids.len() < 2
        || access.shared_contact
        || ir.shapes.iter().any(|shape| {
            shape.net == Some(access.net) && shape.purpose == PhysicalShapePurpose::Pin
        })
    {
        return false;
    }
    let terminal_devices = ir
        .devices
        .iter()
        .flat_map(|device| {
            [device.gate_net, device.drain_net, device.source_net]
                .into_iter()
                .filter(move |net| *net == access.net)
                .map(move |_| device.component_id)
        })
        .collect::<Vec<_>>();
    terminal_devices.len() == access.device_ids.len()
        && terminal_devices
            .iter()
            .all(|device_id| access.device_ids.contains(device_id))
}

fn validate_device_terminal_attachments(ir: &PhysicalLayoutIr, report: &mut PhysicalDrcReport) {
    let device_name = ir
        .devices
        .iter()
        .map(|device| (device.component_id, device.name.as_str()))
        .collect::<HashMap<_, _>>();
    for missing in missing_device_terminal_attachments(ir) {
        report.push(
            "CONNECTIVITY.UNLANDED_DEVICE_TERMINAL",
            format!(
                "{} {} terminal is not continuously attached to its device geometry and Metal 1.",
                device_name
                    .get(&missing.component_id)
                    .copied()
                    .unwrap_or("Unknown device"),
                missing.terminal
            ),
            PhysicalLayer::Contact,
            missing.shape_indices,
            0.0,
            1.0,
        );
    }
}

fn validate_shape_set(
    shapes: &[PhysicalShape],
    max_metal_layers: u16,
    nets: &[crate::physical_layout::PhysicalNet],
    row_topology: &[PhysicalRowTopology],
    technology: &Technology,
) -> PhysicalDrcReport {
    let mut report = PhysicalDrcReport::default();
    let grid = technology.physical_rules.manufacturing_grid_um;

    for (index, shape) in shapes.iter().enumerate() {
        let (left, top, right, bottom) = edges(shape);
        if ![left, top, right, bottom]
            .into_iter()
            .all(|value| aligned(value, grid))
        {
            report.push(
                "GRID.001",
                format!(
                    "{} shape is not aligned to the {grid:.4} µm manufacturing grid.",
                    layer_name(shape.layer)
                ),
                shape.layer,
                vec![index],
                [left, top, right, bottom]
                    .into_iter()
                    .map(|value| ((value / grid) - (value / grid).round()).abs() * grid)
                    .fold(0.0, f64::max),
                0.0,
            );
        }

        match shape.layer {
            PhysicalLayer::Metal(layer)
                if layer == 0
                    || (layer > max_metal_layers
                        && !(shape.purpose == PhysicalShapePurpose::DummyFill
                            && layer == max_metal_layers + 1
                            && technology
                                .physical_rules
                                .density_fill
                                .layers
                                .contains_key("top_metal"))) =>
            {
                report.push(
                    "LAYER.001",
                    format!("Metal {layer} is outside the technology layer range."),
                    shape.layer,
                    vec![index],
                    f64::from(layer),
                    f64::from(max_metal_layers),
                );
            }
            PhysicalLayer::Via(lower) if lower == 0 || lower >= max_metal_layers => {
                report.push(
                    "LAYER.002",
                    format!(
                        "Via {lower}-{} is outside the technology layer range.",
                        lower + 1
                    ),
                    shape.layer,
                    vec![index],
                    f64::from(lower + 1),
                    f64::from(max_metal_layers),
                );
            }
            _ => {}
        }

        let composite_route_fill = shape.purpose == PhysicalShapePurpose::RouteFill;
        if composite_route_fill {
            let supports = shapes
                .iter()
                .enumerate()
                .filter(|(other_index, other)| {
                    *other_index != index
                        && other.layer == shape.layer
                        && other.net == shape.net
                        && other.purpose != PhysicalShapePurpose::RouteFill
                        && shapes_touch(shape, other)
                })
                .count();
            if supports < 2 {
                report.push(
                    "GEOMETRY.ROUTE_FILL_SUPPORT",
                    format!(
                        "{} composite fill does not join two conductor shapes.",
                        layer_name(shape.layer)
                    ),
                    shape.layer,
                    vec![index],
                    supports as f64,
                    2.0,
                );
            }
        }
        if shape.purpose == PhysicalShapePurpose::DummyFill {
            if shape.net.is_some() || shape.component_id.is_some() {
                report.push(
                    "DENSITY.FILL_IDENTITY",
                    "Dummy fill must not carry electrical net or component identity.".into(),
                    shape.layer,
                    vec![index],
                    1.0,
                    0.0,
                );
            }
        } else if let Some(rule) =
            layer_rule(shape.layer, technology).filter(|_| !composite_route_fill)
        {
            let width = shape.width.min(shape.height);
            if width + EPSILON < rule.min_width_um {
                report.push(
                    "GEOMETRY.MIN_WIDTH",
                    format!("{} width is below its minimum.", layer_name(shape.layer)),
                    shape.layer,
                    vec![index],
                    width,
                    rule.min_width_um,
                );
            }
            let area = shape.width * shape.height;
            if area + EPSILON < rule.min_area_um2 {
                report.push(
                    "GEOMETRY.MIN_AREA",
                    format!("{} area is below its minimum.", layer_name(shape.layer)),
                    shape.layer,
                    vec![index],
                    area,
                    rule.min_area_um2,
                );
            }
        }
        if let Some(rule) = cut_rule(shape.layer, technology) {
            let size = shape.width.min(shape.height);
            if size + EPSILON < rule.size_um {
                report.push(
                    "CUT.MIN_SIZE",
                    format!("{} cut is below its minimum size.", layer_name(shape.layer)),
                    shape.layer,
                    vec![index],
                    size,
                    rule.size_um,
                );
            }
        }
    }

    let mut spatial = PhysicalCanvas::new(&technology.physical_rules);
    let mut shape_by_occupied_id = HashMap::with_capacity(shapes.len());
    for (shape_index, shape) in shapes.iter().enumerate() {
        // Density fill has its own process-aware spacing/coverage validation
        // and is skipped by the pairwise checks below. Avoid rasterizing
        // large, electrically inert preview tiles into the fine routing grid.
        if shape.purpose == PhysicalShapePurpose::DummyFill {
            continue;
        }
        let occupied_id = spatial.index_unchecked(
            shape,
            format!("drc-shape-{shape_index}"),
            obstruction_type(shape.layer),
        );
        shape_by_occupied_id.insert(occupied_id, shape_index);
    }

    for left_index in 0..shapes.len() {
        let left = &shapes[left_index];
        if left.purpose == PhysicalShapePurpose::DummyFill {
            continue;
        }
        let required = layer_rule(left.layer, technology)
            .map(|rule| rule.min_spacing_um)
            .or_else(|| cut_rule(left.layer, technology).map(|rule| rule.min_spacing_um));
        let Some(required) = required else {
            continue;
        };
        for neighbor in spatial.query_neighbors(left) {
            let right_index = shape_by_occupied_id[&neighbor.id];
            if right_index <= left_index {
                continue;
            }
            let right = &shapes[right_index];
            if right.purpose == PhysicalShapePurpose::DummyFill {
                continue;
            }
            // Rectilinear active islands are serialized as non-overlapping,
            // edge-abutting rectangles. Their union is one manufactured
            // diffusion polygon, so internal decomposition edges are not
            // diffusion-spacing boundaries.
            if same_active_island(left, right, row_topology) {
                continue;
            }
            // Process-owned perimeter rings and their fixed, unassigned pads
            // are decomposed into touching rectangles without pretending they
            // belong to a routed schematic net. Their touching edges form the
            // intended manufactured perimeter conductor.
            if left.net.is_none()
                && right.net.is_none()
                && left.component_id.is_none()
                && right.component_id.is_none()
                && matches!(
                    left.purpose,
                    PhysicalShapePurpose::PowerRail | PhysicalShapePurpose::Pin
                )
                && matches!(
                    right.purpose,
                    PhysicalShapePurpose::PowerRail | PhysicalShapePurpose::Pin
                )
            {
                continue;
            }
            // Continuous same-net layers may merge into one conductor. Cuts
            // remain discrete manufactured features, so contact/via spacing
            // applies even when both cuts belong to the same electrical net.
            if layer_rule(left.layer, technology).is_some()
                && left.net.is_some()
                && left.net == right.net
            {
                continue;
            }
            let measured = spacing(left, right);
            if measured + EPSILON < required {
                let cut_spacing = cut_rule(left.layer, technology).is_some();
                report.push(
                    if cut_spacing {
                        "CUT.MIN_SPACING"
                    } else {
                        "GEOMETRY.MIN_SPACING"
                    },
                    if cut_spacing {
                        format!("{} cuts are too close.", layer_name(left.layer))
                    } else {
                        format!("{} shapes are too close.", layer_name(left.layer))
                    },
                    left.layer,
                    vec![left_index, right_index],
                    measured,
                    required,
                );
            }
        }
    }

    let maximum_density_clearance = technology
        .physical_rules
        .density_fill
        .layers
        .values()
        .map(|rule| rule.fill_spacing_um.max(rule.circuit_spacing_um))
        .fold(0.0, f64::max);
    let mut density_spatial = DensitySpatialIndex::new(maximum_density_clearance.max(32.0));
    for (index, shape) in shapes.iter().enumerate() {
        density_spatial.insert(index, shape);
    }
    for (fill_index, fill) in shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| shape.purpose == PhysicalShapePurpose::DummyFill)
    {
        let Some((material, rule)) = density_fill_rule(fill, technology) else {
            report.push(
                "DENSITY.UNMAPPED_FILL",
                "Dummy fill has no matching process density rule.".into(),
                fill.layer,
                vec![fill_index],
                0.0,
                1.0,
            );
            continue;
        };
        let query_clearance = rule.fill_spacing_um.max(rule.circuit_spacing_um);
        for other_index in density_spatial.query(fill, query_clearance) {
            let other = &shapes[other_index];
            if other_index == fill_index {
                continue;
            }
            let required = if other.purpose == PhysicalShapePurpose::DummyFill {
                density_fill_rule(other, technology)
                    .filter(|(other_material, _)| *other_material == material)
                    .map(|_| rule.fill_spacing_um)
            } else if density_fill_circuit_obstacle(material, fill.layer, other.layer) {
                Some(rule.circuit_spacing_um)
            } else {
                None
            };
            let Some(required) = required else {
                continue;
            };
            if other.purpose == PhysicalShapePurpose::DummyFill && other_index < fill_index {
                continue;
            }
            let measured = spacing(fill, other);
            if measured + EPSILON < required {
                report.push(
                    "DENSITY.FILL_SPACING",
                    format!("{material} dummy fill violates its process clearance."),
                    fill.layer,
                    vec![fill_index, other_index],
                    measured,
                    required,
                );
            }
        }
        if let Some(support) = &rule.support_layer {
            let supported = shapes.iter().any(|shape| {
                shape.purpose == PhysicalShapePurpose::DummyFill
                    && density_fill_rule(shape, technology).is_some_and(|(name, _)| name == support)
                    && contains(shape, fill, 0.0)
            });
            if !supported {
                report.push(
                    "DENSITY.FILL_SUPPORT",
                    format!("{material} dummy fill is not enclosed by {support} dummy fill."),
                    fill.layer,
                    vec![fill_index],
                    0.0,
                    1.0,
                );
            }
        }
    }

    for (index, cut) in shapes.iter().enumerate() {
        let Some(rule) = cut_rule(cut.layer, technology) else {
            continue;
        };
        let required_layers = match cut.layer {
            PhysicalLayer::Contact => vec![PhysicalLayer::Metal(1)],
            PhysicalLayer::Via(lower) => {
                vec![PhysicalLayer::Metal(lower), PhysicalLayer::Metal(lower + 1)]
            }
            _ => unreachable!(),
        };
        for required_layer in required_layers {
            let enclosed = shapes.iter().any(|shape| {
                shape.layer == required_layer
                    && shape.net == cut.net
                    && contains(shape, cut, rule.enclosure_um)
            });
            if !enclosed {
                report.push(
                    "CUT.ENCLOSURE",
                    format!(
                        "{} is not enclosed by {}.",
                        layer_name(cut.layer),
                        layer_name(required_layer)
                    ),
                    cut.layer,
                    vec![index],
                    0.0,
                    rule.enclosure_um,
                );
            }
        }
        if cut.layer == PhysicalLayer::Contact {
            let enclosed_by_device = shapes.iter().any(|shape| {
                matches!(
                    shape.layer,
                    PhysicalLayer::Ndiff | PhysicalLayer::Pdiff | PhysicalLayer::Poly
                ) && (shape.component_id == cut.component_id
                    || (shape.purpose == PhysicalShapePurpose::Active
                        && cut.component_id.is_some_and(|component_id| {
                            shared_active_owns(shape, component_id, row_topology)
                        })))
                    && contains(shape, cut, rule.enclosure_um)
            });
            if !enclosed_by_device {
                report.push(
                    "CONTACT.DEVICE_ENCLOSURE",
                    "Contact is not enclosed by diffusion or poly.".into(),
                    cut.layer,
                    vec![index],
                    0.0,
                    rule.enclosure_um,
                );
            }
        }
    }

    let power_nets = nets
        .iter()
        .filter(|net| net.role == crate::physical_layout::NetRole::Power)
        .map(|net| net.id)
        .collect::<HashSet<_>>();
    let ground_nets = nets
        .iter()
        .filter(|net| net.role == crate::physical_layout::NetRole::Ground)
        .map(|net| net.id)
        .collect::<HashSet<_>>();
    for (index, diffusion) in shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| shape.purpose != PhysicalShapePurpose::DummyFill)
    {
        let tap_well = match (diffusion.layer, diffusion.net) {
            (PhysicalLayer::Ndiff, Some(net)) if power_nets.contains(&net) => {
                Some((PhysicalLayer::Nwell, "N tap", "N"))
            }
            (PhysicalLayer::Pdiff, Some(net)) if ground_nets.contains(&net) => {
                Some((PhysicalLayer::Pwell, "P tap", "P"))
            }
            _ => None,
        };
        let required_well = tap_well.or_else(|| match diffusion.layer {
            PhysicalLayer::Pdiff => Some((PhysicalLayer::Nwell, "P", "N")),
            PhysicalLayer::Ndiff => Some((PhysicalLayer::Pwell, "N", "P")),
            _ => None,
        });
        let Some((well_layer, diffusion_name, well_name)) = required_well else {
            continue;
        };
        if !shapes.iter().any(|shape| {
            shape.layer == well_layer
                && contains(
                    shape,
                    diffusion,
                    technology.physical_rules.well_enclosure_um,
                )
        }) {
            report.push(
                "WELL.ENCLOSURE",
                format!("{diffusion_name} diffusion is not enclosed by the {well_name}-well."),
                diffusion.layer,
                vec![index],
                0.0,
                technology.physical_rules.well_enclosure_um,
            );
        }
    }

    for (poly_index, poly) in shapes.iter().enumerate() {
        if poly.layer != PhysicalLayer::Poly || poly.purpose != PhysicalShapePurpose::Gate {
            continue;
        }
        for (diff_index, diffusion) in shapes.iter().enumerate() {
            if !matches!(diffusion.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff)
                || !(diffusion.component_id == poly.component_id
                    || poly.component_id.is_some_and(|component_id| {
                        shared_active_owns(diffusion, component_id, row_topology)
                    }))
                || !overlaps(poly, diffusion)
            {
                continue;
            }
            let measured = (poly.height - diffusion.height) / 2.0;
            if measured + EPSILON < technology.physical_rules.gate_extension_um {
                report.push(
                    "POLY.GATE_EXTENSION",
                    "Poly does not extend far enough beyond diffusion.".into(),
                    poly.layer,
                    vec![poly_index, diff_index],
                    measured.max(0.0),
                    technology.physical_rules.gate_extension_um,
                );
            }
        }
    }

    coalesce_spacing_diagnostics(report, shapes, nets)
}

fn shared_active_owns(
    shape: &PhysicalShape,
    component_id: uuid::Uuid,
    rows: &[PhysicalRowTopology],
) -> bool {
    shape.purpose == PhysicalShapePurpose::Active
        && shape.component_id.is_none()
        && rows.iter().flat_map(|row| &row.islands).any(|island| {
            island.layer == shape.layer
                && island.device_ids.contains(&component_id)
                && island
                    .geometry
                    .iter()
                    .chain(island.geometry.is_empty().then_some(&island.bounds))
                    .any(|bounds| bounds_match_shape(bounds, shape))
        })
}

fn bounds_match_shape(
    bounds: &crate::physical_layout::PhysicalBounds,
    shape: &PhysicalShape,
) -> bool {
    (bounds.min_x - (shape.x - shape.width / 2.0)).abs() <= EPSILON
        && (bounds.max_x - (shape.x + shape.width / 2.0)).abs() <= EPSILON
        && (bounds.min_y - (shape.y - shape.height / 2.0)).abs() <= EPSILON
        && (bounds.max_y - (shape.y + shape.height / 2.0)).abs() <= EPSILON
}

fn same_active_island(
    left: &PhysicalShape,
    right: &PhysicalShape,
    rows: &[PhysicalRowTopology],
) -> bool {
    left.purpose == PhysicalShapePurpose::Active
        && right.purpose == PhysicalShapePurpose::Active
        && left.layer == right.layer
        && rows.iter().flat_map(|row| &row.islands).any(|island| {
            island.layer == left.layer
                && island
                    .geometry
                    .iter()
                    .chain(island.geometry.is_empty().then_some(&island.bounds))
                    .any(|bounds| bounds_match_shape(bounds, left))
                && island
                    .geometry
                    .iter()
                    .chain(island.geometry.is_empty().then_some(&island.bounds))
                    .any(|bounds| bounds_match_shape(bounds, right))
        })
}

fn validate_terminal_connectivity(shapes: &[PhysicalShape], report: &mut PhysicalDrcReport) {
    let routing = shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| {
            shape.net.is_some()
                && matches!(
                    shape.layer,
                    PhysicalLayer::Metal(_)
                        | PhysicalLayer::Via(_)
                        | PhysicalLayer::Poly
                        | PhysicalLayer::Contact
                )
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut adjacency = HashMap::<usize, Vec<usize>>::new();
    for (position, left_index) in routing.iter().enumerate() {
        for right_index in routing.iter().skip(position + 1) {
            let left = &shapes[*left_index];
            let right = &shapes[*right_index];
            if left.net != right.net || !shapes_touch(left, right) {
                continue;
            }
            let connected = match (left.layer, right.layer) {
                (PhysicalLayer::Metal(left), PhysicalLayer::Metal(right)) => left == right,
                (PhysicalLayer::Via(lower), PhysicalLayer::Metal(metal))
                | (PhysicalLayer::Metal(metal), PhysicalLayer::Via(lower)) => {
                    metal == lower || metal == lower + 1
                }
                (PhysicalLayer::Poly, PhysicalLayer::Poly)
                | (PhysicalLayer::Contact, PhysicalLayer::Contact) => true,
                (PhysicalLayer::Contact, PhysicalLayer::Metal(1))
                | (PhysicalLayer::Metal(1), PhysicalLayer::Contact)
                | (PhysicalLayer::Contact, PhysicalLayer::Poly)
                | (PhysicalLayer::Poly, PhysicalLayer::Contact) => true,
                _ => false,
            };
            if connected {
                adjacency.entry(*left_index).or_default().push(*right_index);
                adjacency.entry(*right_index).or_default().push(*left_index);
            }
        }
    }

    let mut by_net = BTreeMap::<usize, Vec<(usize, Vec<usize>)>>::new();
    for (access_index, access) in shapes.iter().enumerate().filter(|(_, shape)| {
        shape.net.is_some()
            && shape.layer == PhysicalLayer::Contact
            && (shape.component_id.is_some() || shape.purpose == PhysicalShapePurpose::Contact)
    }) {
        let landing = routing
            .iter()
            .filter(|index| {
                let shape = &shapes[**index];
                shape.layer == PhysicalLayer::Metal(1)
                    && shape.net == access.net
                    && shapes_touch(shape, access)
            })
            .copied()
            .collect::<Vec<_>>();
        if landing.is_empty() {
            report.push(
                "CONNECTIVITY.UNROUTED_TERMINAL",
                "Transistor terminal has no Metal 1 access landing.".into(),
                PhysicalLayer::Metal(1),
                vec![access_index],
                0.0,
                1.0,
            );
        } else {
            by_net
                .entry(access.net.expect("filtered terminal net"))
                .or_default()
                .push((access_index, landing));
        }
    }
    for (pin_index, pin) in shapes
        .iter()
        .enumerate()
        .filter(|(_, shape)| shape.net.is_some() && shape.purpose == PhysicalShapePurpose::Pin)
    {
        by_net
            .entry(pin.net.expect("filtered pin net"))
            .or_default()
            .push((pin_index, vec![pin_index]));
    }

    for (net, accesses) in by_net {
        if accesses.len() < 2 {
            continue;
        }
        let mut component_by_routing = HashMap::<usize, usize>::new();
        let mut component = 0usize;
        for index in routing
            .iter()
            .filter(|index| shapes[**index].net == Some(net))
        {
            if component_by_routing.contains_key(index) {
                continue;
            }
            let mut pending = vec![*index];
            while let Some(candidate) = pending.pop() {
                if component_by_routing.insert(candidate, component).is_some() {
                    continue;
                }
                pending.extend(adjacency.get(&candidate).into_iter().flatten().copied());
            }
            component += 1;
        }
        let access_components = accesses
            .iter()
            .filter_map(|(access, _)| component_by_routing.get(access).copied())
            .collect::<std::collections::BTreeSet<_>>();
        if access_components.len() > 1 {
            report.push(
                "CONNECTIVITY.OPEN_NET",
                format!(
                    "Net {net} device terminals and physical pins occupy {} disconnected routing islands.",
                    access_components.len()
                ),
                PhysicalLayer::Metal(1),
                accesses.iter().map(|(index, _)| *index).collect(),
                access_components.len() as f64,
                1.0,
            );
        }
    }
}

fn coalesce_spacing_diagnostics(
    report: PhysicalDrcReport,
    shapes: &[PhysicalShape],
    physical_nets: &[crate::physical_layout::PhysicalNet],
) -> PhysicalDrcReport {
    let mut result = PhysicalDrcReport::default();
    let mut spacing = BTreeMap::<(String, String, usize, usize), PhysicalDrcDiagnostic>::new();
    for diagnostic in report.diagnostics {
        let diagnostic_nets = diagnostic
            .shape_indices
            .iter()
            .filter_map(|index| shapes.get(*index).and_then(|shape| shape.net))
            .collect::<Vec<_>>();
        if matches!(
            diagnostic.rule_id.as_str(),
            "GEOMETRY.MIN_SPACING" | "CUT.MIN_SPACING"
        ) && diagnostic_nets.len() == 2
        {
            let key = (
                diagnostic.rule_id.clone(),
                diagnostic.layer.clone(),
                diagnostic_nets[0].min(diagnostic_nets[1]),
                diagnostic_nets[0].max(diagnostic_nets[1]),
            );
            if let Some(existing) = spacing.get_mut(&key) {
                existing.measured = existing.measured.min(diagnostic.measured);
                existing.shape_indices.extend(diagnostic.shape_indices);
                existing.shape_indices.sort_unstable();
                existing.shape_indices.dedup();
            } else {
                spacing.insert(key, diagnostic);
            }
        } else {
            result.diagnostics.push(diagnostic);
        }
    }
    result.diagnostics.extend(spacing.into_values());
    result.diagnostics.sort_by(|left, right| {
        (&left.rule_id, &left.layer, &left.shape_indices).cmp(&(
            &right.rule_id,
            &right.layer,
            &right.shape_indices,
        ))
    });
    for diagnostic in &mut result.diagnostics {
        let power_related = diagnostic.shape_indices.iter().any(|index| {
            let Some(net_id) = shapes.get(*index).and_then(|shape| shape.net) else {
                return false;
            };
            physical_nets.iter().any(|net| {
                net.id == net_id
                    && matches!(
                        net.role,
                        crate::physical_layout::NetRole::Power
                            | crate::physical_layout::NetRole::Ground
                    )
            })
        });
        if power_related && matches!(diagnostic.origin, PhysicalDrcOrigin::SignalRouting) {
            diagnostic.origin = PhysicalDrcOrigin::PowerRouting;
            if matches!(diagnostic.category, PhysicalDrcCategory::MetalOverlap) {
                diagnostic.category = PhysicalDrcCategory::PowerCollision;
            }
        }
        *result
            .by_category
            .entry(category_key(diagnostic.category).into())
            .or_default() += 1;
        *result
            .by_origin
            .entry(origin_key(diagnostic.origin).into())
            .or_default() += 1;
    }
    result.error_count = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == PhysicalDrcSeverity::Error)
        .count();
    result.warning_count = result.diagnostics.len() - result.error_count;
    result
}

#[cfg(test)]
mod tests {
    use super::{validate, validate_logical_terminal_obligations, PhysicalDrcReport};
    use crate::{
        physical_layout::{
            DeviceKind, NetRole, PhysicalActiveIsland, PhysicalBounds, PhysicalDevice,
            PhysicalLayer, PhysicalLayoutIr, PhysicalNet, PhysicalRowTopology, PhysicalShape,
            PhysicalShapePurpose, PhysicalTerminal, PhysicalTerminalAccess,
            CURRENT_PHYSICAL_IR_VERSION,
        },
        technology::Technology,
    };

    fn layout(shapes: Vec<PhysicalShape>) -> PhysicalLayoutIr {
        PhysicalLayoutIr {
            format_version: CURRENT_PHYSICAL_IR_VERSION,
            source_project_name: "DRC fixture".into(),
            technology_name: "OpenChippy EDU CMOS".into(),
            technology_fingerprint: Technology::default().fingerprint().unwrap(),
            max_metal_layers: 5,
            devices: vec![],
            nets: vec![],
            pins: vec![],
            planning: Default::default(),
            placement: Default::default(),
            global_routing: Default::default(),
            detailed_routing: Default::default(),
            timing: Default::default(),
            tapeout: crate::physical_layout::PhysicalTapeoutReport {
                name: "fixture".into(),
                width_um: 10.0,
                height_um: 10.0,
                edge_margin_um: 0.0,
                usable_width_um: 10.0,
                usable_height_um: 10.0,
                geometry_width_um: 0.0,
                geometry_height_um: 0.0,
                area_utilization: 0.0,
                fits: true,
                shapes_outside_floorplan: 0,
                shapes_outside_tapeout: 0,
            },
            density: Default::default(),
            bounds: PhysicalBounds {
                min_x: -2.0,
                min_y: -2.0,
                max_x: 2.0,
                max_y: 2.0,
            },
            shapes,
            row_topology: vec![],
            route_quality: Default::default(),
            orphan_routing_shapes_removed: 0,
            physical_blocks: vec![],
            standard_cell_library: Default::default(),
        }
    }

    fn shape(layer: PhysicalLayer, x: f64, width: f64, net: Option<usize>) -> PhysicalShape {
        PhysicalShape {
            layer,
            x,
            y: 0.0,
            width,
            height: width,
            component_id: None,
            net,
            purpose: Default::default(),
        }
    }

    #[test]
    fn reports_grid_width_area_and_spacing_violations() {
        let ir = layout(vec![
            shape(PhysicalLayer::Metal(1), 0.005, 0.1, Some(1)),
            shape(PhysicalLayer::Metal(1), 0.125, 0.1, Some(2)),
        ]);
        let report = validate(&ir, &Technology::default());
        let ids = report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.rule_id.as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"GRID.001"));
        assert!(ids.contains(&"GEOMETRY.MIN_WIDTH"));
        assert!(ids.contains(&"GEOMETRY.MIN_AREA"));
        assert!(ids.contains(&"GEOMETRY.MIN_SPACING"));
        assert_eq!(
            report.by_category.values().sum::<usize>(),
            report.diagnostics.len()
        );
        assert_eq!(
            report.by_origin.values().sum::<usize>(),
            report.diagnostics.len()
        );
    }

    #[test]
    fn reports_via_size_and_both_missing_enclosures() {
        let ir = layout(vec![shape(PhysicalLayer::Via(1), 0.0, 0.1, Some(1))]);
        let report = validate(&ir, &Technology::default());
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "CUT.MIN_SIZE"));
        assert_eq!(
            report
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.rule_id == "CUT.ENCLOSURE")
                .count(),
            2
        );
    }

    #[test]
    fn same_net_metal_merges_but_same_net_vias_keep_cut_spacing() {
        let metal = layout(vec![
            shape(PhysicalLayer::Metal(2), 0.0, 0.2, Some(7)),
            shape(PhysicalLayer::Metal(2), 0.1, 0.2, Some(7)),
        ]);
        assert!(!validate(&metal, &Technology::default())
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "GEOMETRY.MIN_SPACING"));

        let vias = layout(vec![
            shape(PhysicalLayer::Via(1), 0.0, 0.24, Some(7)),
            shape(PhysicalLayer::Via(1), 0.25, 0.24, Some(7)),
        ]);
        let spacing = validate(&vias, &Technology::default())
            .diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.rule_id == "CUT.MIN_SPACING")
            .expect("same-net via cuts still require process spacing");
        assert_eq!(spacing.layer, "via12");
        assert!((spacing.measured - 0.01).abs() < 1e-7);
        assert_eq!(spacing.required, 0.08);
    }

    #[test]
    fn composite_route_fill_requires_two_same_net_supports() {
        let left = shape(PhysicalLayer::Metal(1), -0.16, 0.30, Some(7));
        let right = shape(PhysicalLayer::Metal(1), 0.16, 0.30, Some(7));
        let mut fill = shape(PhysicalLayer::Metal(1), 0.0, 0.02, Some(7));
        fill.height = 0.10;
        fill.purpose = PhysicalShapePurpose::RouteFill;

        let valid = validate(
            &layout(vec![left.clone(), right, fill.clone()]),
            &Technology::default(),
        );
        assert!(!valid.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.rule_id.as_str(),
                "GEOMETRY.MIN_WIDTH" | "GEOMETRY.MIN_AREA" | "GEOMETRY.ROUTE_FILL_SUPPORT"
            ) && diagnostic.shape_indices.contains(&2)
        }));

        let unsupported = validate(&layout(vec![left, fill]), &Technology::default());
        assert!(unsupported
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "GEOMETRY.ROUTE_FILL_SUPPORT"));
    }

    #[test]
    fn reports_unlanded_and_disconnected_transistor_terminals() {
        let component = uuid::Uuid::new_v4();
        let mut left_contact = shape(PhysicalLayer::Contact, -1.0, 0.22, Some(7));
        left_contact.component_id = Some(component);
        let mut right_contact = shape(PhysicalLayer::Contact, 1.0, 0.22, Some(7));
        right_contact.component_id = Some(component);
        let mut left_landing = shape(PhysicalLayer::Metal(1), -1.0, 0.30, Some(7));
        left_landing.component_id = None;
        let mut ir = layout(vec![left_contact, right_contact, left_landing]);
        let report = validate(&ir, &Technology::default());
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "CONNECTIVITY.UNROUTED_TERMINAL"));

        let mut right_landing = shape(PhysicalLayer::Metal(1), 1.0, 0.30, Some(7));
        right_landing.component_id = None;
        ir.shapes.push(right_landing);
        let report = validate(&ir, &Technology::default());
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "CONNECTIVITY.OPEN_NET"));
    }

    #[test]
    fn disconnected_physical_pin_is_reported_as_an_open_net() {
        let component_id = uuid::Uuid::new_v4();

        let mut contact = shape(PhysicalLayer::Contact, -1.0, 0.22, Some(7));
        contact.component_id = Some(component_id);

        let landing = shape(PhysicalLayer::Metal(1), -1.0, 0.30, Some(7));

        let mut pin = shape(PhysicalLayer::Metal(1), 1.0, 0.30, Some(7));
        pin.purpose = PhysicalShapePurpose::Pin;

        let report = validate(&layout(vec![contact, landing, pin]), &Technology::default());

        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "CONNECTIVITY.OPEN_NET"));
    }

    #[test]
    fn a_contact_and_local_landing_do_not_close_a_single_terminal_net() {
        let component_id = uuid::Uuid::new_v4();
        let mut gate = shape(PhysicalLayer::Poly, 0.0, 0.18, Some(7));
        gate.height = 1.0;
        gate.component_id = Some(component_id);
        gate.purpose = PhysicalShapePurpose::Gate;
        let mut access = shape(PhysicalLayer::Poly, 0.0, 0.22, Some(7));
        access.component_id = Some(component_id);
        access.purpose = PhysicalShapePurpose::GateAccess;
        let mut contact = shape(PhysicalLayer::Contact, 0.0, 0.22, Some(7));
        contact.component_id = Some(component_id);
        contact.purpose = PhysicalShapePurpose::Contact;
        let mut landing = shape(PhysicalLayer::Metal(1), 0.0, 0.30, Some(7));
        landing.purpose = PhysicalShapePurpose::DeviceLanding;

        let mut ir = layout(vec![gate, access, contact, landing]);
        ir.devices.push(PhysicalDevice {
            component_id,
            name: "M1".into(),
            physical_group: None,
            standard_cell_group: None,
            kind: DeviceKind::Nmos,
            gate_net: 7,
            drain_net: 8,
            source_net: 9,
            width_um: 1.0,
            length_um: 0.18,
        });
        ir.nets.push(PhysicalNet {
            id: 7,
            name: "floating_gate".into(),
            role: NetRole::Internal,
            terminals: vec![PhysicalTerminal {
                component_id,
                component_name: "M1".into(),
                terminal: "gate".into(),
            }],
        });

        let mut report = PhysicalDrcReport::default();
        validate_logical_terminal_obligations(&ir, &mut report);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "CONNECTIVITY.SINGLE_TERMINAL_NET"));
    }

    #[test]
    fn one_shared_diffusion_access_can_close_two_terminal_obligations() {
        let left = uuid::Uuid::new_v4();
        let right = uuid::Uuid::new_v4();
        let mut contact = shape(PhysicalLayer::Contact, 0.0, 0.22, Some(7));
        contact.purpose = PhysicalShapePurpose::Contact;
        let mut ir = layout(vec![contact]);
        ir.devices = [left, right]
            .into_iter()
            .map(|component_id| PhysicalDevice {
                component_id,
                name: format!("M{component_id}"),
                physical_group: None,
                standard_cell_group: None,
                kind: DeviceKind::Nmos,
                gate_net: 8,
                drain_net: 7,
                source_net: 9,
                width_um: 1.0,
                length_um: 0.18,
            })
            .collect();
        ir.nets.push(PhysicalNet {
            id: 7,
            name: "shared_active".into(),
            role: NetRole::Internal,
            terminals: vec![],
        });
        ir.row_topology.push(PhysicalRowTopology {
            kind: DeviceKind::Nmos,
            y: 0.0,
            ordered_devices: vec![left, right],
            islands: vec![PhysicalActiveIsland {
                layer: PhysicalLayer::Ndiff,
                device_ids: vec![left, right],
                terminal_nets: vec![9, 7, 9],
                accesses: vec![PhysicalTerminalAccess {
                    net: 7,
                    device_ids: vec![left, right],
                    x: 0.0,
                    y: 0.0,
                    shared_contact: true,
                }],
                bounds: PhysicalBounds {
                    min_x: -1.0,
                    min_y: -0.5,
                    max_x: 1.0,
                    max_y: 0.5,
                },
                geometry: vec![],
            }],
            gate_straps: vec![],
        });

        let mut report = PhysicalDrcReport::default();
        validate_logical_terminal_obligations(&ir, &mut report);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    }

    #[test]
    fn topology_internal_diffusion_node_requires_no_contact_or_metal_pin() {
        let left = uuid::Uuid::new_v4();
        let right = uuid::Uuid::new_v4();
        let mut ir = layout(vec![]);
        ir.devices = [left, right]
            .into_iter()
            .map(|component_id| PhysicalDevice {
                component_id,
                name: format!("M{component_id}"),
                physical_group: None,
                standard_cell_group: Some("CELL".into()),
                kind: DeviceKind::Nmos,
                gate_net: 8,
                drain_net: 7,
                source_net: 9,
                width_um: 1.0,
                length_um: 0.18,
            })
            .collect();
        ir.nets.push(PhysicalNet {
            id: 7,
            name: "internal_shared_active".into(),
            role: NetRole::Internal,
            terminals: vec![],
        });
        ir.row_topology.push(PhysicalRowTopology {
            kind: DeviceKind::Nmos,
            y: 0.0,
            ordered_devices: vec![left, right],
            islands: vec![PhysicalActiveIsland {
                layer: PhysicalLayer::Ndiff,
                device_ids: vec![left, right],
                terminal_nets: vec![9, 7, 9],
                accesses: vec![PhysicalTerminalAccess {
                    net: 7,
                    device_ids: vec![left, right],
                    x: 0.0,
                    y: 0.0,
                    shared_contact: false,
                }],
                bounds: PhysicalBounds {
                    min_x: -1.0,
                    min_y: -0.5,
                    max_x: 1.0,
                    max_y: 0.5,
                },
                geometry: vec![],
            }],
            gate_straps: vec![],
        });

        let mut report = PhysicalDrcReport::default();
        validate_logical_terminal_obligations(&ir, &mut report);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    }

    #[test]
    fn two_local_contacts_on_one_topology_access_are_diffusion_equivalent() {
        let left = uuid::Uuid::new_v4();
        let right = uuid::Uuid::new_v4();
        let mut left_contact = shape(PhysicalLayer::Contact, -0.5, 0.22, Some(7));
        left_contact.component_id = Some(left);
        left_contact.purpose = PhysicalShapePurpose::Contact;
        let mut right_contact = shape(PhysicalLayer::Contact, 0.5, 0.22, Some(7));
        right_contact.component_id = Some(right);
        right_contact.purpose = PhysicalShapePurpose::Contact;
        let mut ir = layout(vec![left_contact, right_contact]);
        ir.devices = [left, right]
            .into_iter()
            .map(|component_id| PhysicalDevice {
                component_id,
                name: format!("M{component_id}"),
                physical_group: None,
                standard_cell_group: None,
                kind: DeviceKind::Nmos,
                gate_net: 8,
                drain_net: 7,
                source_net: 9,
                width_um: 1.0,
                length_um: 0.18,
            })
            .collect();
        ir.nets.push(PhysicalNet {
            id: 7,
            name: "shared_active_with_local_cuts".into(),
            role: NetRole::Internal,
            terminals: vec![],
        });
        ir.row_topology.push(PhysicalRowTopology {
            kind: DeviceKind::Nmos,
            y: 0.0,
            ordered_devices: vec![left, right],
            islands: vec![PhysicalActiveIsland {
                layer: PhysicalLayer::Ndiff,
                device_ids: vec![left, right],
                terminal_nets: vec![9, 7, 9],
                accesses: vec![PhysicalTerminalAccess {
                    net: 7,
                    device_ids: vec![left, right],
                    x: 0.0,
                    y: 0.0,
                    shared_contact: false,
                }],
                bounds: PhysicalBounds {
                    min_x: -1.0,
                    min_y: -0.5,
                    max_x: 1.0,
                    max_y: 0.5,
                },
                geometry: vec![],
            }],
            gate_straps: vec![],
        });

        let mut report = PhysicalDrcReport::default();
        validate_logical_terminal_obligations(&ir, &mut report);
        assert!(report.diagnostics.is_empty(), "{:#?}", report.diagnostics);
    }

    #[test]
    fn missing_rectilinear_active_piece_is_not_hidden_by_island_metadata() {
        let component_id = uuid::Uuid::new_v4();
        let left = PhysicalBounds {
            min_x: -1.0,
            min_y: -0.5,
            max_x: 0.0,
            max_y: 0.5,
        };
        let right = PhysicalBounds {
            min_x: 0.0,
            min_y: -1.0,
            max_x: 1.0,
            max_y: 1.0,
        };
        let mut active = shape(PhysicalLayer::Ndiff, -0.5, 1.0, None);
        active.height = 1.0;
        active.component_id = Some(component_id);
        active.purpose = PhysicalShapePurpose::Active;
        let mut gate = shape(PhysicalLayer::Poly, -0.5, 0.18, Some(7));
        gate.height = 1.4;
        gate.component_id = Some(component_id);
        gate.purpose = PhysicalShapePurpose::Gate;
        let mut ir = layout(vec![active, gate]);
        ir.devices.push(PhysicalDevice {
            component_id,
            name: "M1".into(),
            physical_group: None,
            standard_cell_group: None,
            kind: DeviceKind::Nmos,
            gate_net: 7,
            drain_net: 8,
            source_net: 9,
            width_um: 1.0,
            length_um: 0.18,
        });
        ir.row_topology.push(PhysicalRowTopology {
            kind: DeviceKind::Nmos,
            y: 0.0,
            ordered_devices: vec![component_id],
            islands: vec![PhysicalActiveIsland {
                layer: PhysicalLayer::Ndiff,
                device_ids: vec![component_id],
                terminal_nets: vec![8, 9],
                accesses: vec![PhysicalTerminalAccess {
                    net: 8,
                    device_ids: vec![component_id],
                    x: -0.5,
                    y: 0.0,
                    shared_contact: false,
                }],
                bounds: PhysicalBounds {
                    min_x: -1.0,
                    min_y: -1.0,
                    max_x: 1.0,
                    max_y: 1.0,
                },
                geometry: vec![left, right],
            }],
            gate_straps: vec![],
        });

        let report = validate(&ir, &Technology::default());
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "CONNECTIVITY.OPEN_ACTIVE_ISLAND"));
    }
}
