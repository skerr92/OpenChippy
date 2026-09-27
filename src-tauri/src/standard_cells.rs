use crate::physical_layout::{DeviceKind, NetRole, PhysicalDevice, PhysicalNet};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const EDU_RECIPE: &str = include_str!("../resources/standard_cells/openchippy-edu.yaml");
const GF180_RECIPE: &str = include_str!("../resources/standard_cells/gf180mcu-3v3-5m.yaml");
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellClass {
    Combinational,
    PassGate,
    Sequential,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellDefinition {
    pub name: String,
    pub class: CellClass,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub power_pins: Vec<String>,
    pub function: String,
    pub transistor_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct PhysicalRecipe {
    pub format_version: u32,
    pub library_id: String,
    pub library_name: String,
    pub process_id: String,
    pub deck_revision: String,
    pub source: String,
    pub placement_site_width_um: f64,
    pub row_height_um: f64,
    pub transistor_pitch_um: f64,
    pub cell_padding_um: f64,
    pub pin_layer: u16,
    pub power_layer: u16,
    pub nmos_width_um: f64,
    pub pmos_width_um: f64,
    pub gate_length_um: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedCell {
    pub definition: CellDefinition,
    pub width_um: f64,
    pub height_um: f64,
    pub placement_sites: usize,
    pub pin_layer: u16,
    pub power_layer: u16,
    pub nmos_width_um: f64,
    pub pmos_width_um: f64,
    pub gate_length_um: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySummary {
    pub format_version: u32,
    pub library_name: String,
    pub process_id: String,
    pub deck_revision: String,
    pub source: String,
    pub generated: bool,
    pub cells: Vec<GeneratedCell>,
    #[serde(default)]
    pub inferred_instances: Vec<InferredCell>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferredCell {
    pub instance: String,
    pub cell: String,
    pub device_ids: Vec<Uuid>,
}

fn comb(name: &str, inputs: &[&str], function: &str, transistors: usize) -> CellDefinition {
    CellDefinition {
        name: name.into(),
        class: CellClass::Combinational,
        inputs: inputs.iter().map(|pin| (*pin).into()).collect(),
        outputs: vec!["Y".into()],
        power_pins: vec!["VDD".into(), "GND".into()],
        function: function.into(),
        transistor_count: transistors,
    }
}

fn seq(
    name: &str,
    inputs: &[&str],
    outputs: &[&str],
    function: &str,
    transistors: usize,
) -> CellDefinition {
    CellDefinition {
        name: name.into(),
        class: CellClass::Sequential,
        inputs: inputs.iter().map(|pin| (*pin).into()).collect(),
        outputs: outputs.iter().map(|pin| (*pin).into()).collect(),
        power_pins: vec!["VDD".into(), "GND".into()],
        function: function.into(),
        transistor_count: transistors,
    }
}

fn decoder(name: &str, inputs: &[&str], output_count: usize, transistors: usize) -> CellDefinition {
    CellDefinition {
        name: name.into(),
        class: CellClass::Combinational,
        inputs: inputs.iter().map(|pin| (*pin).into()).collect(),
        outputs: (0..output_count).map(|index| format!("Y{index}")).collect(),
        power_pins: vec!["VDD".into(), "GND".into()],
        function: format!("one-hot {output_count}-way decode while EN=1"),
        transistor_count: transistors,
    }
}

/// Stable logical catalog shared by every technology-specific physical recipe.
pub fn definitions() -> Vec<CellDefinition> {
    vec![
        comb("AND2", &["A", "B"], "Y=A&B", 6),
        comb("NAND2", &["A", "B"], "Y=!(A&B)", 4),
        comb("OR2", &["A", "B"], "Y=A|B", 6),
        comb("NOR2", &["A", "B"], "Y=!(A|B)", 4),
        comb("XOR2", &["A", "B"], "Y=A^B", 12),
        comb("XNOR2", &["A", "B"], "Y=!(A^B)", 14),
        comb("AND3", &["A", "B", "C"], "Y=A&B&C", 8),
        comb("NAND3", &["A", "B", "C"], "Y=!(A&B&C)", 6),
        comb("OR3", &["A", "B", "C"], "Y=A|B|C", 8),
        comb("NOR3", &["A", "B", "C"], "Y=!(A|B|C)", 6),
        comb("XOR3", &["A", "B", "C"], "Y=A^B^C", 24),
        comb("XNOR3", &["A", "B", "C"], "Y=!(A^B^C)", 26),
        comb("INV", &["A"], "Y=!A", 2),
        comb("BUF", &["A"], "Y=A", 4),
        CellDefinition {
            name: "TGATE".into(),
            class: CellClass::PassGate,
            inputs: vec!["A".into(), "EN".into(), "EN_B".into()],
            outputs: vec!["Y".into()],
            power_pins: Vec::new(),
            function: "Y=A when EN=1 and EN_B=0; otherwise Z".into(),
            transistor_count: 2,
        },
        seq(
            "DLATCH",
            &["D", "G"],
            &["Q", "Q_B"],
            "transparent while G=1",
            18,
        ),
        seq(
            "SR_FF",
            &["S", "R", "CLK"],
            &["Q", "Q_B"],
            "clocked set/reset",
            20,
        ),
        seq(
            "JK_FF",
            &["J", "K", "CLK"],
            &["Q", "Q_B"],
            "Q+=J*!Q+!K*Q on rising CLK",
            28,
        ),
        seq(
            "DFF",
            &["D", "CLK"],
            &["Q", "Q_B"],
            "Q+=D on rising CLK",
            24,
        ),
        comb("MUX2", &["A", "B", "S"], "Y=!S*A+S*B", 12),
        comb(
            "MUX4",
            &["A", "B", "C", "D", "S0", "S1"],
            "Y=select(A,B,C,D,S1:S0)",
            28,
        ),
        decoder("DEC1X2", &["A", "EN"], 2, 8),
        decoder("DEC2X4", &["A0", "A1", "EN"], 4, 24),
        decoder("DEC3X8", &["A0", "A1", "A2", "EN"], 8, 54),
    ]
}

pub fn recipe_for_library(library_id: &str) -> Result<PhysicalRecipe, String> {
    let source = match library_id {
        "openchippy-edu" => EDU_RECIPE,
        "gf180mcu-3v3-5m" => GF180_RECIPE,
        _ => {
            return Err(format!(
                "unknown packaged standard-cell library {library_id}"
            ))
        }
    };
    let recipe: PhysicalRecipe = serde_yaml::from_str(source).map_err(|error| error.to_string())?;
    validate_recipe(&recipe)?;
    if recipe.library_id != library_id {
        return Err(format!(
            "standard-cell recipe {library_id} identifies itself as {}",
            recipe.library_id
        ));
    }
    Ok(recipe)
}

pub fn generate_for_library(library_id: &str, process_id: &str) -> Result<LibrarySummary, String> {
    let recipe = recipe_for_library(library_id)?;
    if recipe.process_id != process_id
        && !(recipe.process_id == "openchippy-edu-cmos" && process_id == "openchippy-edu-5m")
    {
        return Err(format!(
            "standard-cell library {library_id} targets {}, not {process_id}",
            recipe.process_id
        ));
    }
    let cells = definitions()
        .into_iter()
        .map(|definition| {
            let device_columns = definition.transistor_count.div_ceil(2);
            let pins =
                definition.inputs.len() + definition.outputs.len() + definition.power_pins.len();
            let access_columns = pins.saturating_sub(4).div_ceil(4);
            let raw_width = 2.0 * recipe.cell_padding_um
                + (device_columns + access_columns) as f64 * recipe.transistor_pitch_um;
            let placement_sites =
                (raw_width / recipe.placement_site_width_um).ceil().max(1.0) as usize;
            GeneratedCell {
                definition,
                width_um: placement_sites as f64 * recipe.placement_site_width_um,
                height_um: recipe.row_height_um,
                placement_sites,
                pin_layer: recipe.pin_layer,
                power_layer: recipe.power_layer,
                nmos_width_um: recipe.nmos_width_um,
                pmos_width_um: recipe.pmos_width_um,
                gate_length_um: recipe.gate_length_um,
            }
        })
        .collect();
    Ok(LibrarySummary {
        format_version: FORMAT_VERSION,
        library_name: recipe.library_name,
        process_id: recipe.process_id,
        deck_revision: recipe.deck_revision,
        source: recipe.source,
        generated: true,
        cells,
        inferred_instances: Vec::new(),
    })
}

pub fn infer_flattened_instances(
    devices: &[PhysicalDevice],
    nets: &[PhysicalNet],
    library: &mut LibrarySummary,
) {
    if !library.generated {
        return;
    }
    let available = library
        .cells
        .iter()
        .map(|cell| cell.definition.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut groups = BTreeMap::<&str, Vec<&PhysicalDevice>>::new();
    for device in devices {
        if let Some(group) = device.standard_cell_group.as_deref() {
            groups.entry(group).or_default().push(device);
        }
    }
    library.inferred_instances = groups
        .into_iter()
        .filter_map(|(instance, mut group)| {
            group.sort_by_key(|device| device.component_id);
            let cell = recognize_cmos_gate(&group, nets)?;
            available.contains(cell).then(|| InferredCell {
                instance: instance.to_string(),
                cell: cell.to_string(),
                device_ids: group.iter().map(|device| device.component_id).collect(),
            })
        })
        .collect();
}

fn recognize_cmos_gate(devices: &[&PhysicalDevice], nets: &[PhysicalNet]) -> Option<&'static str> {
    let pmos = devices
        .iter()
        .copied()
        .filter(|device| device.kind == DeviceKind::Pmos)
        .collect::<Vec<_>>();
    let nmos = devices
        .iter()
        .copied()
        .filter(|device| device.kind == DeviceKind::Nmos)
        .collect::<Vec<_>>();
    let power = nets.iter().find(|net| net.role == NetRole::Power)?.id;
    let ground = nets.iter().find(|net| net.role == NetRole::Ground)?.id;
    let edge = |device: &PhysicalDevice, left: usize, right: usize| {
        (device.drain_net == left && device.source_net == right)
            || (device.drain_net == right && device.source_net == left)
    };
    let gate_pairs = || {
        let p = pmos
            .iter()
            .map(|device| device.gate_net)
            .collect::<BTreeSet<_>>();
        let n = nmos
            .iter()
            .map(|device| device.gate_net)
            .collect::<BTreeSet<_>>();
        (p == n).then_some(p)
    };
    let candidates = nets
        .iter()
        .map(|net| net.id)
        .filter(|net| *net != power && *net != ground)
        .collect::<Vec<_>>();
    if pmos.len() == 1 && nmos.len() == 1 && pmos[0].gate_net == nmos[0].gate_net {
        if candidates
            .iter()
            .any(|output| edge(pmos[0], power, *output) && edge(nmos[0], ground, *output))
        {
            return Some("INV");
        }
    }
    if pmos.len() != 2 || nmos.len() != 2 || gate_pairs()?.len() != 2 {
        return None;
    }
    for output in candidates {
        let mut internal = nets
            .iter()
            .map(|net| net.id)
            .filter(|net| ![power, ground, output].contains(net));
        let p_parallel = pmos.iter().all(|device| edge(device, power, output));
        let n_parallel = nmos.iter().all(|device| edge(device, ground, output));
        let n_series = internal.clone().any(|middle| {
            nmos.iter().any(|device| edge(device, output, middle))
                && nmos.iter().any(|device| edge(device, middle, ground))
        });
        let p_series = internal.any(|middle| {
            pmos.iter().any(|device| edge(device, output, middle))
                && pmos.iter().any(|device| edge(device, middle, power))
        });
        if p_parallel && n_series {
            return Some("NAND2");
        }
        if n_parallel && p_series {
            return Some("NOR2");
        }
    }
    None
}

pub fn empty_summary(process_id: &str) -> LibrarySummary {
    LibrarySummary {
        format_version: FORMAT_VERSION,
        process_id: process_id.into(),
        ..LibrarySummary::default()
    }
}

fn validate_recipe(recipe: &PhysicalRecipe) -> Result<(), String> {
    if recipe.format_version != FORMAT_VERSION {
        return Err(format!(
            "unsupported standard-cell recipe format {}",
            recipe.format_version
        ));
    }
    for (field, value) in [
        ("placement_site_width_um", recipe.placement_site_width_um),
        ("row_height_um", recipe.row_height_um),
        ("transistor_pitch_um", recipe.transistor_pitch_um),
        ("cell_padding_um", recipe.cell_padding_um),
        ("nmos_width_um", recipe.nmos_width_um),
        ("pmos_width_um", recipe.pmos_width_um),
        ("gate_length_um", recipe.gate_length_um),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(format!("{field} must be finite and greater than zero"));
        }
    }
    if recipe.pin_layer == 0 || recipe.power_layer == 0 {
        return Err("standard-cell routing layers start at Metal 1".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        definitions, generate_for_library, infer_flattened_instances, recipe_for_library, CellClass,
    };
    use crate::physical_layout::{DeviceKind, NetRole, PhysicalDevice, PhysicalNet};
    use std::collections::HashSet;
    use uuid::Uuid;

    fn device(kind: DeviceKind, gate: usize, drain: usize, source: usize) -> PhysicalDevice {
        PhysicalDevice {
            component_id: Uuid::new_v4(),
            name: format!("CELL·M{gate}{drain}{source}"),
            physical_group: Some("CELL".into()),
            standard_cell_group: Some("CELL".into()),
            kind,
            gate_net: gate,
            drain_net: drain,
            source_net: source,
            width_um: 1.0,
            length_um: 0.28,
        }
    }

    fn nets() -> Vec<PhysicalNet> {
        [
            NetRole::Power,
            NetRole::Ground,
            NetRole::Input,
            NetRole::Input,
            NetRole::Output,
            NetRole::Internal,
        ]
        .into_iter()
        .enumerate()
        .map(|(id, role)| PhysicalNet {
            id,
            name: format!("N{id}"),
            role,
            terminals: Vec::new(),
        })
        .collect()
    }

    #[test]
    fn catalog_contains_every_requested_cell_once() {
        let cells = definitions();
        assert_eq!(cells.len(), 24);
        let names = cells
            .iter()
            .map(|cell| cell.name.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(names.len(), cells.len());
        for name in [
            "AND2", "NAND2", "OR2", "NOR2", "XOR2", "XNOR2", "AND3", "NAND3", "OR3", "NOR3",
            "XOR3", "XNOR3", "INV", "BUF", "TGATE", "DLATCH", "SR_FF", "JK_FF", "DFF", "MUX2",
            "MUX4", "DEC1X2", "DEC2X4", "DEC3X8",
        ] {
            assert!(names.contains(name), "missing {name}");
        }
        assert_eq!(
            cells
                .iter()
                .filter(|cell| cell.class == CellClass::Sequential)
                .count(),
            4
        );
    }

    #[test]
    fn packaged_recipes_generate_site_aligned_complete_libraries() {
        for (library_id, process) in [
            ("openchippy-edu", "openchippy-edu-cmos"),
            ("openchippy-edu", "openchippy-edu-5m"),
            ("gf180mcu-3v3-5m", "gf180mcu-3v3-5m-compat"),
        ] {
            let recipe = recipe_for_library(library_id).unwrap();
            let library = generate_for_library(library_id, process).unwrap();
            assert_eq!(library.cells.len(), 24);
            for cell in library.cells {
                let sites = cell.width_um / recipe.placement_site_width_um;
                assert!((sites - sites.round()).abs() < 1e-9);
                assert_eq!(cell.height_um, recipe.row_height_um);
            }
        }
    }

    #[test]
    fn flattened_cmos_topology_selects_inv_nand2_and_nor2() {
        let cases = [
            (
                "INV",
                vec![
                    device(DeviceKind::Pmos, 2, 0, 4),
                    device(DeviceKind::Nmos, 2, 4, 1),
                ],
            ),
            (
                "NAND2",
                vec![
                    device(DeviceKind::Pmos, 2, 0, 4),
                    device(DeviceKind::Pmos, 3, 4, 0),
                    device(DeviceKind::Nmos, 2, 4, 5),
                    device(DeviceKind::Nmos, 3, 5, 1),
                ],
            ),
            (
                "NOR2",
                vec![
                    device(DeviceKind::Pmos, 2, 0, 5),
                    device(DeviceKind::Pmos, 3, 5, 4),
                    device(DeviceKind::Nmos, 2, 4, 1),
                    device(DeviceKind::Nmos, 3, 1, 4),
                ],
            ),
        ];
        for (expected, devices) in cases {
            let mut library =
                generate_for_library("gf180mcu-3v3-5m", "gf180mcu-3v3-5m-compat").unwrap();
            infer_flattened_instances(&devices, &nets(), &mut library);
            assert_eq!(library.inferred_instances.len(), 1);
            assert_eq!(library.inferred_instances[0].cell, expected);
        }
    }
}
