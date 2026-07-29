use crate::{
    physical_drc,
    physical_layout::{PhysicalLayer, PhysicalLayoutIr},
    technology::Technology,
};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeLvsReport {
    pub matched: bool,
    pub expected_device_count: usize,
    pub recognized_device_count: usize,
    pub expected_net_count: usize,
    pub open_net_count: usize,
    pub short_count: usize,
    pub missing_devices: Vec<String>,
    pub missing_terminals: Vec<String>,
}

pub fn compare(layout: &PhysicalLayoutIr, technology: &Technology) -> NativeLvsReport {
    let mut missing_devices = Vec::new();
    let mut missing_terminals = Vec::new();
    let mut recognized_device_count = 0usize;
    let geometrically_missing = physical_drc::missing_device_terminal_attachments(layout)
        .into_iter()
        .map(|missing| (missing.component_id, missing.terminal))
        .collect::<std::collections::HashSet<_>>();
    for device in &layout.devices {
        let owned = layout
            .shapes
            .iter()
            .filter(|shape| shape.component_id == Some(device.component_id))
            .collect::<Vec<_>>();
        let has_diffusion = owned
            .iter()
            .any(|shape| matches!(shape.layer, PhysicalLayer::Ndiff | PhysicalLayer::Pdiff))
            || layout
                .row_topology
                .iter()
                .flat_map(|row| &row.islands)
                .any(|island| island.device_ids.contains(&device.component_id));
        let has_poly = owned
            .iter()
            .any(|shape| shape.layer == PhysicalLayer::Poly && shape.net == Some(device.gate_net));
        let terminals = [
            ("gate", device.gate_net),
            ("drain", device.drain_net),
            ("source", device.source_net),
        ];
        let mut complete = has_diffusion && has_poly;
        for (terminal, _net) in terminals {
            let present = !geometrically_missing.contains(&(device.component_id, terminal));
            if !present {
                complete = false;
                missing_terminals.push(format!("{}:{terminal}", device.name));
            }
        }
        if complete {
            recognized_device_count += 1;
        } else {
            missing_devices.push(device.name.clone());
        }
    }

    let drc = physical_drc::validate(layout, technology);
    let open_net_count = drc
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.rule_id.starts_with("CONNECTIVITY."))
        .count();
    let short_count = drc
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.rule_id == "GEOMETRY.MIN_SPACING" && diagnostic.measured <= 1e-7
        })
        .count();
    let matched =
        recognized_device_count == layout.devices.len() && open_net_count == 0 && short_count == 0;
    NativeLvsReport {
        matched,
        expected_device_count: layout.devices.len(),
        recognized_device_count,
        expected_net_count: layout.nets.len(),
        open_net_count,
        short_count,
        missing_devices,
        missing_terminals,
    }
}

#[cfg(test)]
mod tests {
    use super::compare;
    use crate::{
        model::{Project, TerminalRef},
        physical_layout,
        technology::Technology,
    };

    #[test]
    fn generated_inverter_matches_native_device_inventory_and_connectivity() {
        let mut project = Project::default();
        project.name = "LVS inverter".into();
        let vdd = project.add_component("vdd", 0.0, -10.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 10.0).unwrap();
        let input = project.add_component("input", -10.0, 0.0).unwrap();
        let output = project.add_component("output", 10.0, 0.0).unwrap();
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
        let layout = physical_layout::normalize_project(&project).unwrap();
        let report = compare(&layout, &Technology::default());
        assert!(report.matched, "{report:#?}");
        assert_eq!(report.recognized_device_count, 2);
    }
}
