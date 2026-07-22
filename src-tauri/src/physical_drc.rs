use crate::{
    physical_canvas::{ObstructionType, PhysicalCanvas},
    physical_layout::{PhysicalLayer, PhysicalLayoutIr, PhysicalShape},
    technology::{CutRule, LayerRule, Technology},
};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

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
        PhysicalLayer::Substrate => PhysicalDrcOrigin::GeometryGeneration,
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
        PhysicalLayer::Nwell => Some(&rules.well),
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
        PhysicalLayer::Nwell | PhysicalLayer::Substrate => ObstructionType::Device,
    }
}

pub fn validate(ir: &PhysicalLayoutIr, technology: &Technology) -> PhysicalDrcReport {
    let mut report = PhysicalDrcReport::default();
    let grid = technology.physical_rules.manufacturing_grid_um;

    for (index, shape) in ir.shapes.iter().enumerate() {
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
            PhysicalLayer::Metal(layer) if layer == 0 || layer > ir.max_metal_layers => {
                report.push(
                    "LAYER.001",
                    format!("Metal {layer} is outside the technology layer range."),
                    shape.layer,
                    vec![index],
                    f64::from(layer),
                    f64::from(ir.max_metal_layers),
                );
            }
            PhysicalLayer::Via(lower) if lower == 0 || lower >= ir.max_metal_layers => {
                report.push(
                    "LAYER.002",
                    format!(
                        "Via {lower}-{} is outside the technology layer range.",
                        lower + 1
                    ),
                    shape.layer,
                    vec![index],
                    f64::from(lower + 1),
                    f64::from(ir.max_metal_layers),
                );
            }
            _ => {}
        }

        if let Some(rule) = layer_rule(shape.layer, technology) {
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
    let mut shape_by_occupied_id = HashMap::with_capacity(ir.shapes.len());
    for (shape_index, shape) in ir.shapes.iter().enumerate() {
        let occupied_id = spatial.index_unchecked(
            shape,
            format!("drc-shape-{shape_index}"),
            obstruction_type(shape.layer),
        );
        shape_by_occupied_id.insert(occupied_id, shape_index);
    }

    for left_index in 0..ir.shapes.len() {
        let left = &ir.shapes[left_index];
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
            let right = &ir.shapes[right_index];
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

    for (index, cut) in ir.shapes.iter().enumerate() {
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
            let enclosed = ir.shapes.iter().any(|shape| {
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
            let enclosed_by_device = ir.shapes.iter().any(|shape| {
                matches!(
                    shape.layer,
                    PhysicalLayer::Ndiff | PhysicalLayer::Pdiff | PhysicalLayer::Poly
                ) && shape.component_id == cut.component_id
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

    for (index, diffusion) in ir.shapes.iter().enumerate() {
        if diffusion.layer == PhysicalLayer::Pdiff
            && !ir.shapes.iter().any(|shape| {
                shape.layer == PhysicalLayer::Nwell
                    && contains(
                        shape,
                        diffusion,
                        technology.physical_rules.well_enclosure_um,
                    )
            })
        {
            report.push(
                "WELL.ENCLOSURE",
                "P diffusion is not enclosed by the N-well.".into(),
                diffusion.layer,
                vec![index],
                0.0,
                technology.physical_rules.well_enclosure_um,
            );
        }
    }

    for (poly_index, poly) in ir.shapes.iter().enumerate() {
        if poly.layer != PhysicalLayer::Poly {
            continue;
        }
        for (diff_index, diffusion) in ir.shapes.iter().enumerate() {
            if !matches!(diffusion.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff)
                || diffusion.component_id != poly.component_id
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

    coalesce_spacing_diagnostics(report, ir)
}

fn coalesce_spacing_diagnostics(
    report: PhysicalDrcReport,
    ir: &PhysicalLayoutIr,
) -> PhysicalDrcReport {
    let mut result = PhysicalDrcReport::default();
    let mut spacing = BTreeMap::<(String, String, usize, usize), PhysicalDrcDiagnostic>::new();
    for diagnostic in report.diagnostics {
        let nets = diagnostic
            .shape_indices
            .iter()
            .filter_map(|index| ir.shapes.get(*index).and_then(|shape| shape.net))
            .collect::<Vec<_>>();
        if matches!(
            diagnostic.rule_id.as_str(),
            "GEOMETRY.MIN_SPACING" | "CUT.MIN_SPACING"
        ) && nets.len() == 2
        {
            let key = (
                diagnostic.rule_id.clone(),
                diagnostic.layer.clone(),
                nets[0].min(nets[1]),
                nets[0].max(nets[1]),
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
            let Some(net_id) = ir.shapes.get(*index).and_then(|shape| shape.net) else {
                return false;
            };
            ir.nets.iter().any(|net| {
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
    use super::validate;
    use crate::{
        physical_layout::{
            PhysicalBounds, PhysicalLayer, PhysicalLayoutIr, PhysicalShape,
            CURRENT_PHYSICAL_IR_VERSION,
        },
        technology::Technology,
    };

    fn layout(shapes: Vec<PhysicalShape>) -> PhysicalLayoutIr {
        PhysicalLayoutIr {
            format_version: CURRENT_PHYSICAL_IR_VERSION,
            source_project_name: "DRC fixture".into(),
            technology_name: "OpenChippy EDU CMOS".into(),
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
            bounds: PhysicalBounds {
                min_x: -2.0,
                min_y: -2.0,
                max_x: 2.0,
                max_y: 2.0,
            },
            shapes,
            physical_blocks: vec![],
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
}
