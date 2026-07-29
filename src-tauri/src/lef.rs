use crate::physical_layout::{NetRole, PhysicalLayer, PhysicalLayoutIr, PhysicalShape};
use std::collections::HashSet;

fn identifier(value: &str) -> String {
    let mut result = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if result.is_empty() || result.starts_with(|character: char| character.is_ascii_digit()) {
        result.insert(0, '_');
    }
    result
}

fn layer_name(layer: PhysicalLayer) -> Option<String> {
    match layer {
        PhysicalLayer::Metal(index) => Some(format!("Metal{index}")),
        _ => None,
    }
}

fn rect(shape: &PhysicalShape, origin_x: f64, origin_y: f64) -> String {
    format!(
        "      RECT {:.6} {:.6} {:.6} {:.6} ;\n",
        shape.x - shape.width / 2.0 - origin_x,
        shape.y - shape.height / 2.0 - origin_y,
        shape.x + shape.width / 2.0 - origin_x,
        shape.y + shape.height / 2.0 - origin_y,
    )
}

/// Emits the first OpenChippy abstract macro view. Pin rectangles come from the
/// accepted physical pin landings, while non-pin metal is represented as OBS.
pub fn export(layout: &PhysicalLayoutIr) -> Result<String, String> {
    let width = layout.bounds.max_x - layout.bounds.min_x;
    let height = layout.bounds.max_y - layout.bounds.min_y;
    if width <= 0.0 || height <= 0.0 {
        return Err("LEF export requires positive Physical IR bounds".into());
    }
    let macro_name = identifier(&layout.source_project_name);
    let site_name = format!("{}_SITE", macro_name.to_ascii_uppercase());
    let site_width = layout.planning.placement_site_width_um;
    let row_height = layout.planning.row_height_um;
    let mut output = format!(
        "VERSION 5.8 ;\nBUSBITCHARS \"[]\" ;\nDIVIDERCHAR \"/\" ;\n\nSITE {site_name}\n  CLASS CORE ;\n  SYMMETRY Y ;\n  SIZE {site_width:.6} BY {row_height:.6} ;\nEND {site_name}\n\nMACRO {macro_name}\n  CLASS BLOCK ;\n  ORIGIN 0 0 ;\n  FOREIGN {macro_name} 0 0 ;\n  SIZE {width:.6} BY {height:.6} ;\n  SYMMETRY X Y R90 ;\n  SITE {site_name} ;\n"
    );
    let pin_components = layout
        .pins
        .iter()
        .map(|pin| pin.component_id)
        .collect::<HashSet<_>>();
    for pin in &layout.pins {
        let direction = match pin.role {
            NetRole::Input => "INPUT",
            NetRole::Output => "OUTPUT",
            NetRole::Power | NetRole::Ground => "INOUT",
            NetRole::Internal => continue,
        };
        let use_name = match pin.role {
            NetRole::Power => "POWER",
            NetRole::Ground => "GROUND",
            _ => "SIGNAL",
        };
        let mut pin_shapes = layout
            .shapes
            .iter()
            .filter(|shape| {
                shape.component_id == Some(pin.component_id)
                    && shape.net == Some(pin.net)
                    && matches!(shape.layer, PhysicalLayer::Metal(_))
            })
            .collect::<Vec<_>>();
        if pin_shapes.is_empty() {
            if let Some(fallback) = layout
                .shapes
                .iter()
                .filter(|shape| {
                    shape.net == Some(pin.net) && matches!(shape.layer, PhysicalLayer::Metal(_))
                })
                .min_by(|left, right| {
                    (left.width * left.height).total_cmp(&(right.width * right.height))
                })
            {
                pin_shapes.push(fallback);
            }
        }
        if pin_shapes.is_empty() {
            return Err(format!(
                "LEF export could not find a metal landing for pin {}",
                pin.name
            ));
        }
        output.push_str(&format!(
            "  PIN {}\n    DIRECTION {direction} ;\n    USE {use_name} ;\n    PORT\n",
            identifier(&pin.name)
        ));
        let mut current_layer = None;
        for shape in pin_shapes {
            let layer = layer_name(shape.layer).expect("pin shape is metal");
            if current_layer.as_deref() != Some(layer.as_str()) {
                output.push_str(&format!("      LAYER {layer} ;\n"));
                current_layer = Some(layer);
            }
            output.push_str(&rect(shape, layout.bounds.min_x, layout.bounds.min_y));
        }
        output.push_str("    END\n");
        output.push_str(&format!("  END {}\n", identifier(&pin.name)));
    }

    output.push_str("  OBS\n");
    let mut current_layer = None;
    for shape in layout.shapes.iter().filter(|shape| {
        matches!(shape.layer, PhysicalLayer::Metal(_))
            && !shape
                .component_id
                .is_some_and(|id| pin_components.contains(&id))
    }) {
        let layer = layer_name(shape.layer).expect("obstruction shape is metal");
        if current_layer.as_deref() != Some(layer.as_str()) {
            output.push_str(&format!("    LAYER {layer} ;\n"));
            current_layer = Some(layer);
        }
        output.push_str(&rect(shape, layout.bounds.min_x, layout.bounds.min_y));
    }
    output.push_str("  END\n");
    output.push_str(&format!("END {macro_name}\n\nEND LIBRARY\n"));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::export;
    use crate::{
        model::{Project, TerminalRef},
        physical_layout,
    };

    #[test]
    fn inverter_lef_has_size_pins_and_obstructions() {
        let mut project = Project::default();
        project.name = "LEF inverter".into();
        let input = project.add_component("input", -10.0, 0.0).unwrap();
        let output = project.add_component("output", 10.0, 0.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 4.0).unwrap();
        project
            .connect(
                TerminalRef {
                    component_id: input,
                    terminal: "out".into(),
                },
                TerminalRef {
                    component_id: nmos,
                    terminal: "gate".into(),
                },
            )
            .unwrap();
        project
            .connect(
                TerminalRef {
                    component_id: nmos,
                    terminal: "drain".into(),
                },
                TerminalRef {
                    component_id: output,
                    terminal: "in".into(),
                },
            )
            .unwrap();
        let layout = physical_layout::normalize_project(&project).unwrap();
        let lef = export(&layout).unwrap();
        assert!(lef.contains("MACRO LEF_inverter"));
        assert!(lef.contains("SITE LEF_INVERTER_SITE"));
        assert!(lef.contains("  SITE LEF_INVERTER_SITE ;"));
        assert!(lef.contains("PIN IN1"));
        assert!(lef.contains("PIN OUT1"));
        assert!(lef.contains("  OBS"));
        assert!(lef.ends_with("END LIBRARY\n"));
    }
}
