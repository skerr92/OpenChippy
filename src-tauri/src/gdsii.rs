use crate::physical_layout::PhysicalShapePurpose;
use crate::physical_layout::{PhysicalBounds, PhysicalLayer, PhysicalLayoutIr};
use crate::technology::{GdsLayerMap, GdsLayerPurpose};
use serde::Serialize;
use std::collections::BTreeMap;

const HEADER: u8 = 0x00;
const BGNLIB: u8 = 0x01;
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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GdsExportReport {
    pub format_version: u32,
    pub library_name: String,
    pub top_cell: String,
    pub database_units_per_micron: u32,
    pub boundary_count: usize,
    pub label_count: usize,
    pub structure_count: usize,
    pub reference_count: usize,
    pub layer_boundary_counts: BTreeMap<String, usize>,
    pub bounds: PhysicalBounds,
    pub byte_count: usize,
    pub structurally_valid: bool,
    pub diagnostics: Vec<String>,
}

pub fn export(
    ir: &PhysicalLayoutIr,
    database_units_per_micron: u32,
    layer_map: &GdsLayerMap,
) -> Result<(Vec<u8>, GdsExportReport), String> {
    if database_units_per_micron == 0 {
        return Err("GDSII database units per micron must be positive".into());
    }
    let library_name = sanitize_name("OPENCHIPPY");
    let top_cell = sanitize_name(&ir.source_project_name);
    let mut output = Vec::new();
    record_i16(&mut output, HEADER, &[600]);
    record_i16(&mut output, BGNLIB, &[0; 12]);
    record_ascii(&mut output, LIBNAME, &library_name);
    record_real8(
        &mut output,
        UNITS,
        &[
            1.0 / f64::from(database_units_per_micron),
            1.0e-6 / f64::from(database_units_per_micron),
        ],
    );
    let mut layer_boundary_counts = BTreeMap::new();
    let mut owned_shapes = std::collections::BTreeSet::new();
    let mut block_cells = Vec::new();
    for (index, block) in ir.physical_blocks.iter().enumerate() {
        let cell_name = sanitize_name(&format!(
            "{}_{}",
            if block.cell_name.is_empty() {
                block.instance_name.as_str()
            } else {
                block.cell_name.as_str()
            },
            index + 1
        ));
        record_i16(&mut output, BGNSTR, &[0; 12]);
        record_ascii(&mut output, STRNAME, &cell_name);
        for shape_index in &block.shape_indices {
            let shape = ir
                .shapes
                .get(*shape_index)
                .ok_or_else(|| format!("physical block references missing shape {shape_index}"))?;
            owned_shapes.insert(*shape_index);
            emit_shape(
                &mut output,
                shape,
                database_units_per_micron,
                layer_map,
                &mut layer_boundary_counts,
            )?;
        }
        record_empty(&mut output, ENDSTR);
        block_cells.push(cell_name);
    }

    record_i16(&mut output, BGNSTR, &[0; 12]);
    record_ascii(&mut output, STRNAME, &top_cell);
    for (shape_index, shape) in ir.shapes.iter().enumerate() {
        if !owned_shapes.contains(&shape_index) {
            emit_shape(
                &mut output,
                shape,
                database_units_per_micron,
                layer_map,
                &mut layer_boundary_counts,
            )?;
        }
    }
    for cell_name in &block_cells {
        record_empty(&mut output, SREF);
        record_ascii(&mut output, SNAME, cell_name);
        record_i32(&mut output, XY, &[0, 0]);
        record_empty(&mut output, ENDEL);
    }

    for pin in &ir.pins {
        let target = ir.shapes.iter().find(|shape| {
            shape.net == Some(pin.net) && matches!(shape.layer, PhysicalLayer::Metal(_))
        });
        let location = target.map(|shape| (shape.x, shape.y)).unwrap_or((
            (ir.bounds.min_x + ir.bounds.max_x) / 2.0,
            (ir.bounds.min_y + ir.bounds.max_y) / 2.0,
        ));
        let label_layer = target
            .and_then(|shape| mapped_purposes(layer_map, shape.layer).ok()?.first())
            .map(|purpose| purpose.layer)
            .unwrap_or(100);
        record_empty(&mut output, TEXT);
        record_i16(
            &mut output,
            LAYER,
            &[to_gds_i16(label_layer, "label layer")?],
        );
        record_i16(
            &mut output,
            TEXTTYPE,
            &[to_gds_i16(layer_map.label_datatype, "label datatype")?],
        );
        record_i32(
            &mut output,
            XY,
            &[
                to_dbu(location.0, database_units_per_micron)?,
                to_dbu(location.1, database_units_per_micron)?,
            ],
        );
        record_ascii(&mut output, STRING, &pin.name);
        record_empty(&mut output, ENDEL);
    }
    record_empty(&mut output, ENDSTR);
    record_empty(&mut output, ENDLIB);

    let mut report = validate(&output)?;
    report.format_version = 1;
    report.library_name = library_name;
    report.top_cell = top_cell;
    report.database_units_per_micron = database_units_per_micron;
    report.bounds = PhysicalBounds {
        min_x: report.bounds.min_x / f64::from(database_units_per_micron),
        min_y: report.bounds.min_y / f64::from(database_units_per_micron),
        max_x: report.bounds.max_x / f64::from(database_units_per_micron),
        max_y: report.bounds.max_y / f64::from(database_units_per_micron),
    };
    report.byte_count = output.len();
    if report.structure_count != ir.physical_blocks.len() + 1 {
        report.structurally_valid = false;
        report.diagnostics.push(format!(
            "export contains {} structures; expected one top cell plus {} physical blocks",
            report.structure_count,
            ir.physical_blocks.len()
        ));
    }
    if report.reference_count != ir.physical_blocks.len() {
        report.structurally_valid = false;
        report.diagnostics.push(format!(
            "top cell contains {} references for {} physical blocks",
            report.reference_count,
            ir.physical_blocks.len()
        ));
    }
    let expected_boundary_count = layer_boundary_counts.values().sum::<usize>();
    if report.boundary_count != expected_boundary_count {
        report.structurally_valid = false;
        report.diagnostics.push(format!(
            "export contains {} boundaries; the process mapping requires {}",
            report.boundary_count, expected_boundary_count
        ));
    }
    if report.layer_boundary_counts != layer_boundary_counts {
        report.structurally_valid = false;
        report
            .diagnostics
            .push("exported per-layer boundary counts do not match Physical IR".into());
    }
    let expected_bounds = shape_bounds(ir, layer_map)?;
    if !bounds_match(
        &report.bounds,
        &expected_bounds,
        1.0 / f64::from(database_units_per_micron),
    ) {
        report.structurally_valid = false;
        report
            .diagnostics
            .push("exported top-cell bounds do not match the Physical IR shape envelope".into());
    }
    Ok((output, report))
}

pub fn validate(bytes: &[u8]) -> Result<GdsExportReport, String> {
    let mut offset = 0usize;
    let mut boundary_count = 0usize;
    let mut label_count = 0usize;
    let mut structure_count = 0usize;
    let mut reference_count = 0usize;
    let mut current_element = None;
    let mut current_layer = None;
    let mut current_datatype = None;
    let mut layer_boundary_counts = BTreeMap::new();
    let mut saw_header = false;
    let mut saw_library = false;
    let mut saw_structure = false;
    let mut saw_end_structure = false;
    let mut saw_end_library = false;
    let mut diagnostics = Vec::new();
    let mut parsed_library_name = String::new();
    let mut defined_structures = std::collections::BTreeSet::new();
    let mut referenced_structures = Vec::new();
    let mut current_reference = None;
    let mut database_units_per_micron = 0u32;
    let mut min_x = i32::MAX;
    let mut min_y = i32::MAX;
    let mut max_x = i32::MIN;
    let mut max_y = i32::MIN;

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
            HEADER => saw_header = data_type == 2 && payload.len() == 2,
            LIBNAME => {
                saw_library = data_type == 6 && !payload.is_empty();
                parsed_library_name = ascii_payload(payload);
            }
            UNITS if data_type == 5 && payload.len() == 16 => {
                let user_units_per_dbu = decode_gds_real8(&payload[0..8]);
                if user_units_per_dbu.is_finite() && user_units_per_dbu > 0.0 {
                    database_units_per_micron = (1.0 / user_units_per_dbu)
                        .round()
                        .clamp(0.0, u32::MAX as f64)
                        as u32;
                } else {
                    diagnostics.push("UNITS contains an invalid database-unit value".into());
                }
            }
            BGNSTR => {
                saw_structure = data_type == 2 && payload.len() == 24;
                structure_count += 1;
            }
            STRNAME if data_type == 6 => {
                defined_structures.insert(ascii_payload(payload));
            }
            ENDSTR => saw_end_structure = true,
            ENDLIB => saw_end_library = true,
            BOUNDARY | TEXT | SREF => {
                if current_element.is_some() {
                    diagnostics.push("nested GDSII elements are not valid".into());
                }
                current_element = Some(record_type);
                current_layer = None;
                current_datatype = None;
                current_reference = None;
            }
            SNAME if current_element == Some(SREF) && data_type == 6 => {
                current_reference = Some(ascii_payload(payload));
            }
            LAYER if payload.len() == 2 => {
                current_layer = Some(i16::from_be_bytes([payload[0], payload[1]]));
            }
            DATATYPE | TEXTTYPE if payload.len() == 2 => {
                current_datatype = Some(i16::from_be_bytes([payload[0], payload[1]]));
            }
            XY if current_element == Some(BOUNDARY) => {
                if data_type != 3 || payload.len() != 40 {
                    diagnostics.push("BOUNDARY XY must contain five coordinate pairs".into());
                } else if payload[0..8] != payload[32..40] {
                    diagnostics.push("BOUNDARY polygon is not closed".into());
                } else {
                    for point in payload.chunks_exact(8) {
                        let x = i32::from_be_bytes(point[0..4].try_into().unwrap());
                        let y = i32::from_be_bytes(point[4..8].try_into().unwrap());
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                    }
                }
            }
            ENDEL => match current_element.take() {
                Some(BOUNDARY) => {
                    boundary_count += 1;
                    if let (Some(layer), Some(datatype)) = (current_layer, current_datatype) {
                        let name = gds_pair_name(layer as u16, datatype as u16);
                        *layer_boundary_counts.entry(name).or_insert(0) += 1;
                    } else {
                        diagnostics.push("BOUNDARY is missing LAYER or DATATYPE".into());
                    }
                }
                Some(TEXT) => label_count += 1,
                Some(SREF) => {
                    reference_count += 1;
                    if let Some(name) = current_reference.take() {
                        referenced_structures.push(name);
                    } else {
                        diagnostics.push("SREF is missing SNAME".into());
                    }
                }
                _ => diagnostics.push("ENDEL appears outside a supported element".into()),
            },
            _ => {}
        }
        offset += length;
    }
    for (present, name) in [
        (saw_header, "HEADER"),
        (saw_library, "LIBNAME"),
        (saw_structure, "BGNSTR"),
        (saw_end_structure, "ENDSTR"),
        (saw_end_library, "ENDLIB"),
    ] {
        if !present {
            diagnostics.push(format!("missing or malformed {name} record"));
        }
    }
    let referenced_set = referenced_structures
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for reference in &referenced_structures {
        if !defined_structures.contains(reference) {
            diagnostics.push(format!("SREF references undefined structure {reference}"));
        }
    }
    let top_cells = defined_structures
        .difference(&referenced_set)
        .cloned()
        .collect::<Vec<_>>();
    if top_cells.len() != 1 {
        diagnostics.push(format!(
            "GDSII library must have exactly one unreferenced top cell; found {}",
            top_cells.len()
        ));
    }
    Ok(GdsExportReport {
        format_version: 1,
        library_name: parsed_library_name,
        top_cell: top_cells.first().cloned().unwrap_or_default(),
        database_units_per_micron,
        boundary_count,
        label_count,
        structure_count,
        reference_count,
        layer_boundary_counts,
        bounds: if boundary_count == 0 {
            PhysicalBounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0,
            }
        } else {
            PhysicalBounds {
                min_x: f64::from(min_x),
                min_y: f64::from(min_y),
                max_x: f64::from(max_x),
                max_y: f64::from(max_y),
            }
        },
        byte_count: bytes.len(),
        structurally_valid: diagnostics.is_empty(),
        diagnostics,
    })
}

fn bounds_match(left: &PhysicalBounds, right: &PhysicalBounds, tolerance: f64) -> bool {
    (left.min_x - right.min_x).abs() <= tolerance
        && (left.min_y - right.min_y).abs() <= tolerance
        && (left.max_x - right.max_x).abs() <= tolerance
        && (left.max_y - right.max_y).abs() <= tolerance
}

fn shape_bounds(ir: &PhysicalLayoutIr, layer_map: &GdsLayerMap) -> Result<PhysicalBounds, String> {
    let bounds = ir
        .shapes
        .iter()
        .filter_map(|shape| {
            mapped_purposes_for_shape(layer_map, shape)
                .ok()
                .filter(|purposes| !purposes.is_empty())
                .map(|purposes| {
                    (
                        shape,
                        purposes
                            .iter()
                            .map(|purpose| purpose.enclosure_um)
                            .fold(0.0, f64::max),
                    )
                })
        })
        .fold(
            PhysicalBounds {
                min_x: f64::INFINITY,
                min_y: f64::INFINITY,
                max_x: f64::NEG_INFINITY,
                max_y: f64::NEG_INFINITY,
            },
            |bounds, (shape, enclosure)| PhysicalBounds {
                min_x: bounds.min_x.min(shape.x - shape.width / 2.0 - enclosure),
                min_y: bounds.min_y.min(shape.y - shape.height / 2.0 - enclosure),
                max_x: bounds.max_x.max(shape.x + shape.width / 2.0 + enclosure),
                max_y: bounds.max_y.max(shape.y + shape.height / 2.0 + enclosure),
            },
        );
    if bounds.min_x.is_finite() {
        Ok(bounds)
    } else {
        Ok(PhysicalBounds {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        })
    }
}

fn emit_shape(
    output: &mut Vec<u8>,
    shape: &crate::physical_layout::PhysicalShape,
    database_units_per_micron: u32,
    layer_map: &GdsLayerMap,
    counts: &mut BTreeMap<String, usize>,
) -> Result<(), String> {
    for purpose in mapped_purposes_for_shape(layer_map, shape)? {
        let left = to_dbu(
            shape.x - shape.width / 2.0 - purpose.enclosure_um,
            database_units_per_micron,
        )?;
        let right = to_dbu(
            shape.x + shape.width / 2.0 + purpose.enclosure_um,
            database_units_per_micron,
        )?;
        let bottom = to_dbu(
            shape.y - shape.height / 2.0 - purpose.enclosure_um,
            database_units_per_micron,
        )?;
        let top = to_dbu(
            shape.y + shape.height / 2.0 + purpose.enclosure_um,
            database_units_per_micron,
        )?;
        record_empty(output, BOUNDARY);
        record_i16(output, LAYER, &[to_gds_i16(purpose.layer, "layer")?]);
        record_i16(
            output,
            DATATYPE,
            &[to_gds_i16(purpose.datatype, "datatype")?],
        );
        record_i32(
            output,
            XY,
            &[
                left, bottom, right, bottom, right, top, left, top, left, bottom,
            ],
        );
        record_empty(output, ENDEL);
        *counts
            .entry(gds_pair_name(purpose.layer, purpose.datatype))
            .or_insert(0) += 1;
    }
    Ok(())
}

fn mapped_purposes(
    layer_map: &GdsLayerMap,
    layer: PhysicalLayer,
) -> Result<&[GdsLayerPurpose], String> {
    let name = match layer {
        PhysicalLayer::Substrate => "substrate".into(),
        PhysicalLayer::Pwell => "pwell".into(),
        PhysicalLayer::Nwell => "nwell".into(),
        PhysicalLayer::Ndiff => "ndiff".into(),
        PhysicalLayer::Pdiff => "pdiff".into(),
        PhysicalLayer::Poly => "poly".into(),
        PhysicalLayer::Contact => "contact".into(),
        PhysicalLayer::Metal(index) => format!("metal{index}"),
        PhysicalLayer::Via(lower) => format!("via{lower}{}", lower + 1),
    };
    layer_map
        .layers
        .get(&name)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("active process deck does not map Physical IR layer {name}"))
}

fn mapped_purposes_for_shape<'a>(
    layer_map: &'a GdsLayerMap,
    shape: &crate::physical_layout::PhysicalShape,
) -> Result<&'a [GdsLayerPurpose], String> {
    if shape.purpose != PhysicalShapePurpose::DummyFill {
        return mapped_purposes(layer_map, shape.layer);
    }
    let name = match shape.layer {
        PhysicalLayer::Ndiff | PhysicalLayer::Pdiff => "active".into(),
        PhysicalLayer::Poly => "poly".into(),
        PhysicalLayer::Metal(index) => {
            let metal = format!("metal{index}");
            if layer_map.dummy_layers.contains_key(&metal) {
                metal
            } else {
                "top_metal".into()
            }
        }
        layer => {
            return Err(format!(
                "density fill cannot use Physical IR layer {layer:?}"
            ))
        }
    };
    layer_map
        .dummy_layers
        .get(&name)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("active process deck does not map dummy material {name}"))
}

fn gds_pair_name(layer: u16, datatype: u16) -> String {
    format!("{layer}:{datatype}")
}

fn ascii_payload(payload: &[u8]) -> String {
    String::from_utf8_lossy(payload)
        .trim_end_matches('\0')
        .to_string()
}

fn to_gds_i16(value: u16, field: &str) -> Result<i16, String> {
    i16::try_from(value).map_err(|_| format!("GDS {field} {value} exceeds the stream range"))
}

fn sanitize_name(name: &str) -> String {
    let value = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '$' | '?') {
                character
            } else {
                '_'
            }
        })
        .take(32)
        .collect::<String>();
    if value.is_empty() {
        "TOP".into()
    } else {
        value
    }
}

fn to_dbu(value_um: f64, dbu_per_um: u32) -> Result<i32, String> {
    let value = value_um * f64::from(dbu_per_um);
    if !value.is_finite()
        || value.round() < f64::from(i32::MIN)
        || value.round() > f64::from(i32::MAX)
    {
        return Err(format!(
            "coordinate {value_um} µm is outside the GDSII coordinate range"
        ));
    }
    Ok(value.round() as i32)
}

fn record_empty(output: &mut Vec<u8>, record_type: u8) {
    record(output, record_type, 0, &[]);
}

fn record_i16(output: &mut Vec<u8>, record_type: u8, values: &[i16]) {
    let payload = values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect::<Vec<_>>();
    record(output, record_type, 2, &payload);
}

fn record_i32(output: &mut Vec<u8>, record_type: u8, values: &[i32]) {
    let payload = values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect::<Vec<_>>();
    record(output, record_type, 3, &payload);
}

fn record_real8(output: &mut Vec<u8>, record_type: u8, values: &[f64]) {
    let payload = values
        .iter()
        .flat_map(|value| gds_real8(*value))
        .collect::<Vec<_>>();
    record(output, record_type, 5, &payload);
}

fn record_ascii(output: &mut Vec<u8>, record_type: u8, value: &str) {
    let mut payload = value.as_bytes().to_vec();
    if payload.len() % 2 != 0 {
        payload.push(0);
    }
    record(output, record_type, 6, &payload);
}

fn record(output: &mut Vec<u8>, record_type: u8, data_type: u8, payload: &[u8]) {
    let length = u16::try_from(payload.len() + 4).expect("GDSII record exceeds 65535 bytes");
    output.extend_from_slice(&length.to_be_bytes());
    output.push(record_type);
    output.push(data_type);
    output.extend_from_slice(payload);
}

fn gds_real8(value: f64) -> [u8; 8] {
    if value == 0.0 {
        return [0; 8];
    }
    let sign = if value < 0.0 { 0x80 } else { 0 };
    let mut fraction = value.abs();
    let mut exponent = 64i32;
    while fraction >= 1.0 {
        fraction /= 16.0;
        exponent += 1;
    }
    while fraction < 1.0 / 16.0 {
        fraction *= 16.0;
        exponent -= 1;
    }
    let mantissa = (fraction * (1u64 << 56) as f64).round() as u64;
    let mut result = [0u8; 8];
    result[0] = sign | u8::try_from(exponent).unwrap_or(0);
    for (index, byte) in result[1..].iter_mut().enumerate() {
        *byte = (mantissa >> (48 - index * 8)) as u8;
    }
    result
}

fn decode_gds_real8(bytes: &[u8]) -> f64 {
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
    use super::{export, validate};
    use crate::{
        model::{Project, TerminalRef},
        physical_layout,
        technology::Technology,
    };

    fn inverter() -> Project {
        let mut project = Project::default();
        project.rename("GDS inverter".into()).unwrap();
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
        project
    }

    fn hierarchical_inverter_pair() -> Project {
        let mut leaf = inverter();
        let definition = leaf.capture_block("INV".into()).unwrap();
        let mut parent = Project::default();
        parent.block_definitions = leaf.block_definitions;
        let vdd = parent.add_component("vdd", 0.0, -10.0).unwrap();
        let gnd = parent.add_component("gnd", 0.0, 10.0).unwrap();
        let input = parent.add_component("input", -10.0, 0.0).unwrap();
        let output = parent.add_component("output", 10.0, 0.0).unwrap();
        let first = parent.place_block(definition, -5.0, 0.0).unwrap();
        let second = parent.place_block(definition, 5.0, 0.0).unwrap();
        for instance in [first, second] {
            for (source, source_pin, target_pin) in [(vdd, "out", "VDD1"), (gnd, "out", "GND1")] {
                parent
                    .connect(
                        TerminalRef {
                            component_id: source,
                            terminal: source_pin.into(),
                        },
                        TerminalRef {
                            component_id: instance,
                            terminal: target_pin.into(),
                        },
                    )
                    .unwrap();
            }
        }
        for (source, source_pin, target, target_pin) in [
            (input, "out", first, "IN1"),
            (first, "OUT1", second, "IN1"),
            (second, "OUT1", output, "in"),
        ] {
            parent
                .connect(
                    TerminalRef {
                        component_id: source,
                        terminal: source_pin.into(),
                    },
                    TerminalRef {
                        component_id: target,
                        terminal: target_pin.into(),
                    },
                )
                .unwrap();
        }
        parent
    }

    #[test]
    fn exports_structurally_valid_gds_with_matching_geometry_counts() {
        let project = inverter();
        let ir = physical_layout::normalize_project(&project).unwrap();
        let technology = Technology::default();
        let dbu = technology.physical_rules.database_units_per_micron;
        let (bytes, report) = export(&ir, dbu, &technology.gds_layers).unwrap();
        assert!(report.structurally_valid, "{:?}", report.diagnostics);
        assert_eq!(report.boundary_count, ir.shapes.len());
        assert_eq!(report.label_count, ir.pins.len());
        assert!(report.bounds.min_x <= ir.bounds.min_x);
        assert!(report.bounds.max_x >= ir.bounds.max_x);
        assert_eq!(report.byte_count, bytes.len());
        assert!(bytes.starts_with(&[0, 6, 0, 2, 2, 88]));
        let parsed = validate(&bytes).unwrap();
        assert!(parsed.structurally_valid);
        assert_eq!(parsed.database_units_per_micron, dbu);
    }

    #[test]
    fn shape_envelope_not_planning_bounds_is_the_export_contract() {
        let project = inverter();
        let mut ir = physical_layout::normalize_project(&project).unwrap();
        ir.bounds.min_x += 10.0;
        ir.bounds.max_x -= 10.0;
        let technology = Technology::default();
        let dbu = technology.physical_rules.database_units_per_micron;
        let (_, report) = export(&ir, dbu, &technology.gds_layers).unwrap();
        assert!(report.structurally_valid, "{:?}", report.diagnostics);
        assert!(report.bounds.min_x < ir.bounds.min_x);
        assert!(report.bounds.max_x > ir.bounds.max_x);
    }

    #[test]
    fn gf180_export_uses_process_owned_layers_and_expands_diffusion_implants() {
        let mut technology = Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .unwrap();
        // This fixture exercises process-owned GDS mappings and implant
        // expansion on a deliberately tiny inverter. Density closure is
        // covered by the full-floorplan GF180 regressions in physical_layout.
        technology.physical_rules.density_fill.layers.clear();
        let mut project = inverter();
        project.technology = technology.clone();
        let ir = physical_layout::normalize_project(&project).unwrap();
        let (_, report) = export(
            &ir,
            technology.physical_rules.database_units_per_micron,
            &technology.gds_layers,
        )
        .unwrap();
        assert!(report.structurally_valid, "{:?}", report.diagnostics);
        assert!(!report.layer_boundary_counts.contains_key("1:0"));
        assert!(report.layer_boundary_counts.contains_key("22:0"));
        assert!(report.layer_boundary_counts.contains_key("31:0"));
        assert!(report.layer_boundary_counts.contains_key("32:0"));
        assert_eq!(technology.gds_layers.layers["metal5"][0].layer, 81);
        assert_eq!(technology.gds_layers.layers["ndiff"][1].enclosure_um, 0.35);
        assert_eq!(technology.gds_layers.layers["pdiff"][1].enclosure_um, 0.35);
        assert_eq!(
            technology.physical_rules.layer_overrides["metal5"].min_width_um,
            0.44
        );
        assert_eq!(
            technology.physical_rules.layer_overrides["metal5"].min_area_um2,
            0.5625
        );
    }

    #[test]
    fn reusable_physical_blocks_export_as_referenced_gds_structures() {
        let project = hierarchical_inverter_pair();
        let ir = physical_layout::normalize_project(&project).unwrap();
        assert_eq!(ir.physical_blocks.len(), 2);
        assert!(ir
            .physical_blocks
            .iter()
            .all(|block| block.cell_name == "INV"));
        let technology = Technology::default();
        let (_, report) = export(
            &ir,
            technology.physical_rules.database_units_per_micron,
            &technology.gds_layers,
        )
        .unwrap();
        assert!(report.structurally_valid, "{:?}", report.diagnostics);
        assert_eq!(report.structure_count, 3);
        assert_eq!(report.reference_count, 2);
    }

    #[test]
    fn validator_rejects_truncated_record_streams() {
        let error = validate(&[0, 10, 0, 2, 0]).unwrap_err();
        assert!(error.contains("record length"));
    }
}
