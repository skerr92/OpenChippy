use crate::{
    physical_layout::{PhysicalBounds, PhysicalLayer, PhysicalShape, PhysicalShapePurpose},
    technology::GdsLayerMap,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const LIBNAME: u8 = 0x02;
const UNITS: u8 = 0x03;
const ENDLIB: u8 = 0x04;
const BGNSTR: u8 = 0x05;
const STRNAME: u8 = 0x06;
const ENDSTR: u8 = 0x07;
const BOUNDARY: u8 = 0x08;
const SREF: u8 = 0x0a;
const TEXT: u8 = 0x0c;
const LAYER: u8 = 0x0d;
const DATATYPE: u8 = 0x0e;
const XY: u8 = 0x10;
const ENDEL: u8 = 0x11;
const SNAME: u8 = 0x12;
const TEXTTYPE: u8 = 0x16;
const STRING: u8 = 0x19;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalBoundary {
    pub layer: u16,
    pub datatype: u16,
    pub points: Vec<(i32, i32)>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalLabel {
    pub layer: u16,
    pub texttype: u16,
    pub origin: (i32, i32),
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalReference {
    pub cell_name: String,
    pub origin: (i32, i32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalCell {
    pub boundaries: Vec<CanonicalBoundary>,
    pub labels: Vec<CanonicalLabel>,
    pub references: Vec<CanonicalReference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalGds {
    pub format_version: u32,
    pub library_name: String,
    pub top_cell: String,
    pub database_units_per_micron: u32,
    pub cells: BTreeMap<String, CanonicalCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedPhysicalGeometry {
    pub format_version: u32,
    pub source_top_cell: String,
    pub database_units_per_micron: u32,
    pub bounds: PhysicalBounds,
    pub shapes: Vec<PhysicalShape>,
    pub unmapped_layer_pairs: Vec<String>,
}

#[derive(Default)]
struct Element {
    kind: u8,
    layer: Option<u16>,
    datatype: Option<u16>,
    points: Vec<(i32, i32)>,
    text: Option<String>,
    cell_name: Option<String>,
}

pub fn import_canonical(bytes: &[u8]) -> Result<CanonicalGds, String> {
    let mut offset = 0usize;
    let mut library_name = String::new();
    let mut database_units_per_micron = 0u32;
    let mut cells = BTreeMap::<String, CanonicalCell>::new();
    let mut current_cell_name = None::<String>;
    let mut current_cell = None::<CanonicalCell>;
    let mut current_element = None::<Element>;
    let mut saw_end_library = false;

    while offset < bytes.len() {
        if bytes.len() - offset < 4 {
            return Err(format!("truncated GDSII record header at byte {offset}"));
        }
        let length = usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
        if length < 4 || length % 2 != 0 || offset + length > bytes.len() {
            return Err(format!(
                "invalid GDSII record length {length} at byte {offset}"
            ));
        }
        let record_type = bytes[offset + 2];
        let data_type = bytes[offset + 3];
        let payload = &bytes[offset + 4..offset + length];
        match record_type {
            LIBNAME if data_type == 6 => library_name = ascii(payload),
            UNITS if data_type == 5 && payload.len() == 16 => {
                let user_units_per_dbu = decode_real8(&payload[0..8]);
                if !user_units_per_dbu.is_finite() || user_units_per_dbu <= 0.0 {
                    return Err("GDSII UNITS contains an invalid database unit".into());
                }
                database_units_per_micron = (1.0 / user_units_per_dbu)
                    .round()
                    .clamp(0.0, u32::MAX as f64) as u32;
            }
            BGNSTR => {
                if current_cell.is_some() {
                    return Err("nested GDSII structures are not supported".into());
                }
                current_cell = Some(CanonicalCell::default());
                current_cell_name = None;
            }
            STRNAME if data_type == 6 && current_cell.is_some() => {
                current_cell_name = Some(ascii(payload));
            }
            BOUNDARY | TEXT | SREF => {
                if current_cell.is_none() {
                    return Err("GDSII element appears outside a structure".into());
                }
                if current_element.is_some() {
                    return Err("nested GDSII elements are not supported".into());
                }
                current_element = Some(Element {
                    kind: record_type,
                    ..Element::default()
                });
            }
            LAYER if payload.len() == 2 => {
                element_mut(&mut current_element, "LAYER")?.layer =
                    Some(u16::from_be_bytes([payload[0], payload[1]]));
            }
            DATATYPE | TEXTTYPE if payload.len() == 2 => {
                element_mut(&mut current_element, "DATATYPE/TEXTTYPE")?.datatype =
                    Some(u16::from_be_bytes([payload[0], payload[1]]));
            }
            XY if data_type == 3 && payload.len() % 8 == 0 => {
                element_mut(&mut current_element, "XY")?.points = payload
                    .chunks_exact(8)
                    .map(|point| {
                        (
                            i32::from_be_bytes(point[0..4].try_into().unwrap()),
                            i32::from_be_bytes(point[4..8].try_into().unwrap()),
                        )
                    })
                    .collect();
            }
            STRING if data_type == 6 => {
                element_mut(&mut current_element, "STRING")?.text = Some(ascii(payload));
            }
            SNAME if data_type == 6 => {
                element_mut(&mut current_element, "SNAME")?.cell_name = Some(ascii(payload));
            }
            ENDEL => {
                let element = current_element
                    .take()
                    .ok_or_else(|| "ENDEL appears outside an element".to_string())?;
                let cell = current_cell
                    .as_mut()
                    .ok_or_else(|| "ENDEL appears outside a structure".to_string())?;
                finish_element(element, cell)?;
            }
            ENDSTR => {
                if current_element.is_some() {
                    return Err("GDSII structure ended inside an element".into());
                }
                let name = current_cell_name
                    .take()
                    .ok_or_else(|| "GDSII structure is missing STRNAME".to_string())?;
                let mut cell = current_cell
                    .take()
                    .ok_or_else(|| "ENDSTR appears outside a structure".to_string())?;
                canonicalize_cell(&mut cell);
                if cells.insert(name.clone(), cell).is_some() {
                    return Err(format!("duplicate GDSII structure {name}"));
                }
            }
            ENDLIB => saw_end_library = true,
            // Header, timestamps, and other non-geometric metadata are ignored.
            0x00 | 0x01 => {}
            // Explicitly reject geometry/instance forms not yet represented by
            // the MPV-4 canonical importer.
            0x09 | 0x0b | 0x15 | 0x2d => {
                return Err(format!(
                    "unsupported GDSII element record 0x{record_type:02x}"
                ));
            }
            _ => {}
        }
        offset += length;
    }
    if !saw_end_library {
        return Err("GDSII stream is missing ENDLIB".into());
    }
    if database_units_per_micron == 0 {
        return Err("GDSII stream is missing valid UNITS".into());
    }
    let referenced = cells
        .values()
        .flat_map(|cell| {
            cell.references
                .iter()
                .map(|reference| reference.cell_name.clone())
        })
        .collect::<BTreeSet<_>>();
    for name in &referenced {
        if !cells.contains_key(name) {
            return Err(format!(
                "GDSII reference targets undefined structure {name}"
            ));
        }
    }
    let top_cells = cells
        .keys()
        .filter(|name| !referenced.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if top_cells.len() != 1 {
        return Err(format!(
            "GDSII library must have exactly one top cell; found {}",
            top_cells.len()
        ));
    }
    Ok(CanonicalGds {
        format_version: 1,
        library_name,
        top_cell: top_cells[0].clone(),
        database_units_per_micron,
        cells,
    })
}

impl CanonicalGds {
    pub fn to_physical_geometry(
        &self,
        layer_map: &GdsLayerMap,
    ) -> Result<ImportedPhysicalGeometry, String> {
        let mut flattened = Vec::new();
        flatten_cell(
            self,
            &self.top_cell,
            (0, 0),
            &mut BTreeSet::new(),
            &mut flattened,
        )?;
        let reverse = reverse_layer_map(layer_map)?;
        let mut candidates = flattened
            .into_iter()
            .map(|boundary| {
                let pair = (boundary.layer, boundary.datatype);
                let layers = reverse.get(&pair).cloned().unwrap_or_default();
                (boundary, layers)
            })
            .collect::<Vec<_>>();
        let companion_rects = candidates
            .iter()
            .filter_map(|(boundary, layers)| {
                (layers.len() == 1)
                    .then(|| {
                        rectangle_bounds(&boundary.points)
                            .ok()
                            .map(|bounds| (layers[0], bounds))
                    })
                    .flatten()
            })
            .collect::<Vec<_>>();
        let mut unmapped = BTreeSet::new();
        let mut shape_keys = BTreeSet::new();
        let scale = f64::from(self.database_units_per_micron);
        for (boundary, layers) in candidates.drain(..) {
            let (layer, purpose) = match layers.as_slice() {
                [] => {
                    unmapped.insert(format!("{}:{}", boundary.layer, boundary.datatype));
                    continue;
                }
                [mapped] => *mapped,
                choices => {
                    let bounds = rectangle_bounds(&boundary.points)?;
                    let matching = choices
                        .iter()
                        .copied()
                        .filter(|mapped| {
                            companion_rects
                                .iter()
                                .any(|(companion, (left, bottom, right, top))| {
                                    companion.0 == mapped.0
                                        && companion.1 == mapped.1
                                        && *left <= bounds.0
                                        && *bottom <= bounds.1
                                        && *right >= bounds.2
                                        && *top >= bounds.3
                                })
                        })
                        .collect::<BTreeSet<_>>();
                    (matching.len() == 1)
                        .then(|| *matching.first().expect("one matching purpose"))
                        .ok_or_else(|| {
                            format!(
                                "layer {}:{} is ambiguous without a companion process purpose",
                                boundary.layer, boundary.datatype
                            )
                        })?
                }
            };
            let (left, bottom, right, top) = rectangle_bounds(&boundary.points)?;
            shape_keys.insert((layer, purpose, left, bottom, right, top));
        }
        let mut shapes = shape_keys
            .into_iter()
            .map(|(layer, purpose, left, bottom, right, top)| PhysicalShape {
                layer,
                x: f64::from(left + right) / (2.0 * scale),
                y: f64::from(bottom + top) / (2.0 * scale),
                width: f64::from(right - left) / scale,
                height: f64::from(top - bottom) / scale,
                component_id: None,
                net: None,
                purpose,
            })
            .collect::<Vec<_>>();
        infer_connectivity(&mut shapes);
        let bounds = shapes.iter().fold(
            PhysicalBounds {
                min_x: f64::INFINITY,
                min_y: f64::INFINITY,
                max_x: f64::NEG_INFINITY,
                max_y: f64::NEG_INFINITY,
            },
            |bounds, shape| PhysicalBounds {
                min_x: bounds.min_x.min(shape.x - shape.width / 2.0),
                min_y: bounds.min_y.min(shape.y - shape.height / 2.0),
                max_x: bounds.max_x.max(shape.x + shape.width / 2.0),
                max_y: bounds.max_y.max(shape.y + shape.height / 2.0),
            },
        );
        Ok(ImportedPhysicalGeometry {
            format_version: 1,
            source_top_cell: self.top_cell.clone(),
            database_units_per_micron: self.database_units_per_micron,
            bounds,
            shapes,
            unmapped_layer_pairs: unmapped.into_iter().collect(),
        })
    }
}

fn flatten_cell(
    layout: &CanonicalGds,
    name: &str,
    origin: (i32, i32),
    stack: &mut BTreeSet<String>,
    output: &mut Vec<CanonicalBoundary>,
) -> Result<(), String> {
    if !stack.insert(name.to_string()) {
        return Err(format!("cyclic GDSII reference through {name}"));
    }
    let cell = layout
        .cells
        .get(name)
        .ok_or_else(|| format!("missing referenced GDSII cell {name}"))?;
    output.extend(cell.boundaries.iter().cloned().map(|mut boundary| {
        for point in &mut boundary.points {
            point.0 += origin.0;
            point.1 += origin.1;
        }
        boundary
    }));
    for reference in &cell.references {
        flatten_cell(
            layout,
            &reference.cell_name,
            (origin.0 + reference.origin.0, origin.1 + reference.origin.1),
            stack,
            output,
        )?;
    }
    stack.remove(name);
    Ok(())
}

fn reverse_layer_map(
    layer_map: &GdsLayerMap,
) -> Result<BTreeMap<(u16, u16), Vec<(PhysicalLayer, PhysicalShapePurpose)>>, String> {
    let mut reverse = BTreeMap::<(u16, u16), Vec<(PhysicalLayer, PhysicalShapePurpose)>>::new();
    for (name, purposes) in &layer_map.layers {
        let Some(layer) = physical_layer(name) else {
            return Err(format!("unsupported OpenChippy process layer {name}"));
        };
        for purpose in purposes {
            let layers = reverse
                .entry((purpose.layer, purpose.datatype))
                .or_default();
            let mapped = (layer, PhysicalShapePurpose::Unknown);
            if !layers.contains(&mapped) {
                layers.push(mapped);
                layers.sort();
            }
        }
    }
    for (name, purposes) in &layer_map.dummy_layers {
        let layer = match name.as_str() {
            "active" => PhysicalLayer::Ndiff,
            "poly" => PhysicalLayer::Poly,
            "top_metal" => {
                let highest = layer_map
                    .layers
                    .keys()
                    .filter_map(|name| name.strip_prefix("metal")?.parse::<u16>().ok())
                    .max()
                    .unwrap_or(0);
                PhysicalLayer::Metal(highest + 1)
            }
            _ => physical_layer(name)
                .ok_or_else(|| format!("unsupported OpenChippy dummy layer {name}"))?,
        };
        for purpose in purposes {
            let mapped = (layer, PhysicalShapePurpose::DummyFill);
            let layers = reverse
                .entry((purpose.layer, purpose.datatype))
                .or_default();
            if !layers.contains(&mapped) {
                layers.push(mapped);
                layers.sort();
            }
        }
    }
    Ok(reverse)
}

fn physical_layer(name: &str) -> Option<PhysicalLayer> {
    match name {
        "substrate" => Some(PhysicalLayer::Substrate),
        "pwell" => Some(PhysicalLayer::Pwell),
        "nwell" => Some(PhysicalLayer::Nwell),
        "ndiff" => Some(PhysicalLayer::Ndiff),
        "pdiff" => Some(PhysicalLayer::Pdiff),
        "poly" => Some(PhysicalLayer::Poly),
        "contact" => Some(PhysicalLayer::Contact),
        _ if name.starts_with("metal") => name[5..].parse().ok().map(PhysicalLayer::Metal),
        _ if name.starts_with("via") => name[3..]
            .chars()
            .next()
            .and_then(|value| value.to_digit(10))
            .map(|value| PhysicalLayer::Via(value as u16)),
        _ => None,
    }
}

fn rectangle_bounds(points: &[(i32, i32)]) -> Result<(i32, i32, i32, i32), String> {
    if points.len() != 4 {
        return Err(format!(
            "Physical IR import currently requires rectangles; found {} vertices",
            points.len()
        ));
    }
    let left = points.iter().map(|point| point.0).min().unwrap();
    let right = points.iter().map(|point| point.0).max().unwrap();
    let bottom = points.iter().map(|point| point.1).min().unwrap();
    let top = points.iter().map(|point| point.1).max().unwrap();
    let corners = points.iter().copied().collect::<BTreeSet<_>>();
    let expected = [(left, bottom), (left, top), (right, bottom), (right, top)]
        .into_iter()
        .collect::<BTreeSet<_>>();
    if corners != expected || left == right || bottom == top {
        return Err("Physical IR import requires a non-degenerate rectangular boundary".into());
    }
    Ok((left, bottom, right, top))
}

fn infer_connectivity(shapes: &mut [PhysicalShape]) {
    let mut parents = (0..shapes.len()).collect::<Vec<_>>();
    for left in 0..shapes.len() {
        for right in left + 1..shapes.len() {
            if shapes[left].purpose != PhysicalShapePurpose::DummyFill
                && shapes[right].purpose != PhysicalShapePurpose::DummyFill
                && electrically_compatible(shapes[left].layer, shapes[right].layer)
                && rectangles_touch(&shapes[left], &shapes[right])
            {
                union(&mut parents, left, right);
            }
        }
    }
    let mut net_ids = BTreeMap::new();
    for index in 0..shapes.len() {
        if shapes[index].purpose == PhysicalShapePurpose::DummyFill
            || !conductive(shapes[index].layer)
        {
            continue;
        }
        let root = find(&mut parents, index);
        let next = net_ids.len();
        shapes[index].net = Some(*net_ids.entry(root).or_insert(next));
    }
}

fn conductive(layer: PhysicalLayer) -> bool {
    matches!(
        layer,
        PhysicalLayer::Ndiff
            | PhysicalLayer::Pdiff
            | PhysicalLayer::Poly
            | PhysicalLayer::Contact
            | PhysicalLayer::Metal(_)
            | PhysicalLayer::Via(_)
    )
}

fn electrically_compatible(left: PhysicalLayer, right: PhysicalLayer) -> bool {
    if left == right {
        return conductive(left);
    }
    match (left, right) {
        (PhysicalLayer::Contact, PhysicalLayer::Metal(1))
        | (PhysicalLayer::Metal(1), PhysicalLayer::Contact)
        | (
            PhysicalLayer::Contact,
            PhysicalLayer::Ndiff | PhysicalLayer::Pdiff | PhysicalLayer::Poly,
        )
        | (
            PhysicalLayer::Ndiff | PhysicalLayer::Pdiff | PhysicalLayer::Poly,
            PhysicalLayer::Contact,
        ) => true,
        (PhysicalLayer::Via(lower), PhysicalLayer::Metal(metal))
        | (PhysicalLayer::Metal(metal), PhysicalLayer::Via(lower)) => {
            metal == lower || metal == lower + 1
        }
        _ => false,
    }
}

fn rectangles_touch(left: &PhysicalShape, right: &PhysicalShape) -> bool {
    let left_edges = (
        left.x - left.width / 2.0,
        left.y - left.height / 2.0,
        left.x + left.width / 2.0,
        left.y + left.height / 2.0,
    );
    let right_edges = (
        right.x - right.width / 2.0,
        right.y - right.height / 2.0,
        right.x + right.width / 2.0,
        right.y + right.height / 2.0,
    );
    left_edges.0 <= right_edges.2
        && left_edges.2 >= right_edges.0
        && left_edges.1 <= right_edges.3
        && left_edges.3 >= right_edges.1
}

fn find(parents: &mut [usize], index: usize) -> usize {
    if parents[index] != index {
        parents[index] = find(parents, parents[index]);
    }
    parents[index]
}

fn union(parents: &mut [usize], left: usize, right: usize) {
    let left = find(parents, left);
    let right = find(parents, right);
    if left != right {
        parents[right] = left;
    }
}

fn finish_element(element: Element, cell: &mut CanonicalCell) -> Result<(), String> {
    match element.kind {
        BOUNDARY => {
            let layer = element
                .layer
                .ok_or_else(|| "BOUNDARY is missing LAYER".to_string())?;
            let datatype = element
                .datatype
                .ok_or_else(|| "BOUNDARY is missing DATATYPE".to_string())?;
            if element.points.len() < 4 || element.points.first() != element.points.last() {
                return Err("BOUNDARY polygon must contain a closed ring".into());
            }
            cell.boundaries.push(CanonicalBoundary {
                layer,
                datatype,
                points: canonical_ring(&element.points),
            });
        }
        TEXT => {
            if element.points.len() != 1 {
                return Err("TEXT must contain exactly one origin".into());
            }
            cell.labels.push(CanonicalLabel {
                layer: element
                    .layer
                    .ok_or_else(|| "TEXT is missing LAYER".to_string())?,
                texttype: element
                    .datatype
                    .ok_or_else(|| "TEXT is missing TEXTTYPE".to_string())?,
                origin: element.points[0],
                text: element
                    .text
                    .ok_or_else(|| "TEXT is missing STRING".to_string())?,
            });
        }
        SREF => {
            if element.points.len() != 1 {
                return Err("SREF must contain exactly one origin".into());
            }
            cell.references.push(CanonicalReference {
                cell_name: element
                    .cell_name
                    .ok_or_else(|| "SREF is missing SNAME".to_string())?,
                origin: element.points[0],
            });
        }
        _ => return Err("unsupported GDSII element".into()),
    }
    Ok(())
}

fn canonicalize_cell(cell: &mut CanonicalCell) {
    cell.boundaries.sort();
    cell.labels.sort();
    cell.references.sort();
}

fn canonical_ring(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    let open = &points[..points.len() - 1];
    let forward = minimal_rotation(open);
    let reversed_points = open.iter().rev().copied().collect::<Vec<_>>();
    let reversed = minimal_rotation(&reversed_points);
    forward.min(reversed)
}

fn minimal_rotation(points: &[(i32, i32)]) -> Vec<(i32, i32)> {
    (0..points.len())
        .map(|start| {
            points[start..]
                .iter()
                .chain(points[..start].iter())
                .copied()
                .collect::<Vec<_>>()
        })
        .min()
        .unwrap_or_default()
}

fn element_mut<'a>(
    element: &'a mut Option<Element>,
    record: &str,
) -> Result<&'a mut Element, String> {
    element
        .as_mut()
        .ok_or_else(|| format!("{record} appears outside an element"))
}

fn ascii(payload: &[u8]) -> String {
    String::from_utf8_lossy(payload)
        .trim_end_matches('\0')
        .to_string()
}

fn decode_real8(bytes: &[u8]) -> f64 {
    if bytes.len() != 8 || bytes.iter().all(|byte| *byte == 0) {
        return 0.0;
    }
    let sign = if bytes[0] & 0x80 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from(bytes[0] & 0x7f) - 64;
    let mantissa = bytes[1..]
        .iter()
        .fold(0u64, |value, byte| (value << 8) | u64::from(*byte));
    sign * (mantissa as f64 / (1u64 << 56) as f64) * 16f64.powi(exponent)
}

#[cfg(test)]
mod tests {
    use super::{canonical_ring, import_canonical};
    use crate::{
        gdsii,
        model::{Project, TerminalRef},
        physical_drc,
        physical_layout::{self, PhysicalLayer, PhysicalShape, PhysicalShapePurpose},
        technology::Technology,
    };

    #[test]
    fn imports_native_boundaries_labels_references_and_units() {
        let mut project = Project::default();
        project.rename("canonical import".into()).unwrap();
        let vdd = project.add_component("vdd", 0.0, -20.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 20.0).unwrap();
        let input = project.add_component("input", -20.0, 0.0).unwrap();
        let output = project.add_component("output", 20.0, 0.0).unwrap();
        let pmos = project.add_component("pmos", 0.0, -5.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 5.0).unwrap();
        for (left, left_terminal, right, right_terminal) in [
            (vdd, "out", pmos, "source"),
            (gnd, "out", nmos, "source"),
            (input, "out", pmos, "gate"),
            (input, "out", nmos, "gate"),
            (pmos, "drain", nmos, "drain"),
            (pmos, "drain", output, "in"),
        ] {
            project
                .connect(
                    TerminalRef {
                        component_id: left,
                        terminal: left_terminal.into(),
                    },
                    TerminalRef {
                        component_id: right,
                        terminal: right_terminal.into(),
                    },
                )
                .unwrap();
        }
        let technology = Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .unwrap();
        let mut ir = physical_layout::normalize_project(&project).unwrap();
        ir.shapes.push(PhysicalShape {
            layer: PhysicalLayer::Metal(6),
            x: 0.0,
            y: 0.0,
            width: 6.0,
            height: 6.0,
            component_id: None,
            net: None,
            purpose: PhysicalShapePurpose::DummyFill,
        });
        let (bytes, report) = gdsii::export(
            &ir,
            technology.physical_rules.database_units_per_micron,
            &technology.gds_layers,
        )
        .unwrap();
        assert_eq!(report.layer_boundary_counts.get("53:4"), Some(&1));
        let imported = import_canonical(&bytes).unwrap();
        assert_eq!(imported.top_cell, "canonical_import");
        assert_eq!(
            imported.database_units_per_micron,
            technology.physical_rules.database_units_per_micron
        );
        let top = &imported.cells[&imported.top_cell];
        assert_eq!(top.boundaries.len(), report.boundary_count);
        assert_eq!(top.labels.len(), report.label_count);
        let physical = imported
            .to_physical_geometry(&technology.gds_layers)
            .unwrap();
        assert!(physical.unmapped_layer_pairs.is_empty());
        assert!(physical.shapes.iter().any(|shape| {
            shape.layer == PhysicalLayer::Metal(6)
                && shape.purpose == PhysicalShapePurpose::DummyFill
                && shape.net.is_none()
        }));
        let after = physical_drc::validate_imported_shapes(
            &physical.shapes,
            technology.max_metal_layers,
            &technology,
        );
        let repeated_physical = imported
            .to_physical_geometry(&technology.gds_layers)
            .unwrap();
        let repeated_after = physical_drc::validate_imported_shapes(
            &repeated_physical.shapes,
            technology.max_metal_layers,
            &technology,
        );
        assert_eq!(repeated_physical, physical);
        assert_eq!(repeated_after, after);
        assert!(physical
            .shapes
            .iter()
            .any(|shape| shape.layer == physical_layout::PhysicalLayer::Ndiff));
        assert!(physical
            .shapes
            .iter()
            .any(|shape| shape.layer == physical_layout::PhysicalLayer::Pdiff));
    }

    #[test]
    fn canonical_rings_ignore_start_vertex_and_winding() {
        let left = [(0, 0), (4, 0), (4, 2), (0, 2), (0, 0)];
        let right = [(4, 2), (4, 0), (0, 0), (0, 2), (4, 2)];
        assert_eq!(canonical_ring(&left), canonical_ring(&right));
    }
}
