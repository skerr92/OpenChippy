use crate::model::{Component, Project, TerminalRef};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    pub component_ids: Vec<Uuid>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub diagnostics: Vec<Diagnostic>,
    pub error_count: usize,
    pub warning_count: usize,
}

type TerminalKey = (Uuid, String);

fn terminals(component: &Component) -> &'static [&'static str] {
    match component.kind.as_str() {
        "nmos" | "pmos" => &["gate", "drain", "source"],
        "resistor" => &["a", "b"],
        "output" => &["in"],
        "junction" | "net_label" => &["node"],
        _ => &["out"],
    }
}

fn key(reference: &TerminalRef) -> TerminalKey {
    (reference.component_id, reference.terminal.clone())
}

pub fn validate(project: &Project) -> ValidationReport {
    let mut diagnostics = Vec::new();
    let mut graph: HashMap<TerminalKey, HashSet<TerminalKey>> = HashMap::new();
    for component in &project.components {
        for terminal in terminals(component) {
            graph.entry((component.id, (*terminal).into())).or_default();
        }
    }
    for wire in &project.wires {
        let from = key(&wire.from);
        let to = wire
            .to
            .as_ref()
            .map(key)
            .unwrap_or_else(|| (wire.id, "free".into()));
        if graph.contains_key(&from) && graph.contains_key(&to) {
            graph.entry(from.clone()).or_default().insert(to.clone());
            graph.entry(to).or_default().insert(from);
        } else if wire.to.is_none() && graph.contains_key(&from) && wire.end.is_some() {
            graph.entry(from.clone()).or_default().insert(to.clone());
            graph.entry(to).or_default().insert(from);
            diagnostics.push(Diagnostic {
                code: "dangling_wire",
                severity: Severity::Warning,
                message: "A wire ends on the grid without a terminal connection.".into(),
                component_ids: vec![wire.from.component_id],
            });
        } else {
            diagnostics.push(Diagnostic {
                code: "disconnected_net",
                severity: Severity::Error,
                message: "A wire references a missing component or terminal.".into(),
                component_ids: wire.to.as_ref().map_or_else(
                    || vec![wire.from.component_id],
                    |to| vec![wire.from.component_id, to.component_id],
                ),
            });
        }
    }

    // Equal net-label names represent the same electrical net even without a drawn wire.
    let mut labels: HashMap<&str, Vec<TerminalKey>> = HashMap::new();
    for component in project
        .components
        .iter()
        .filter(|item| item.kind == "net_label")
    {
        labels
            .entry(&component.name)
            .or_default()
            .push((component.id, "node".into()));
    }
    for terminals in labels.values() {
        if let Some(first) = terminals.first() {
            for terminal in terminals.iter().skip(1) {
                graph
                    .entry(first.clone())
                    .or_default()
                    .insert(terminal.clone());
                graph
                    .entry(terminal.clone())
                    .or_default()
                    .insert(first.clone());
            }
        }
    }

    let mut names: HashMap<&str, Vec<Uuid>> = HashMap::new();
    for component in &project.components {
        names.entry(&component.name).or_default().push(component.id);
    }
    for (name, ids) in names.into_iter().filter(|(_, ids)| ids.len() > 1) {
        diagnostics.push(Diagnostic {
            code: "duplicate_name",
            severity: Severity::Error,
            message: format!("Duplicate component name: {name}."),
            component_ids: ids,
        });
    }

    let vdd: Vec<_> = project
        .components
        .iter()
        .filter(|item| item.kind == "vdd")
        .collect();
    let gnd: Vec<_> = project
        .components
        .iter()
        .filter(|item| item.kind == "gnd")
        .collect();
    if vdd.is_empty() {
        diagnostics.push(Diagnostic {
            code: "missing_power",
            severity: Severity::Error,
            message: "Project has no VDD source.".into(),
            component_ids: vec![],
        });
    }
    if gnd.is_empty() {
        diagnostics.push(Diagnostic {
            code: "missing_power",
            severity: Severity::Error,
            message: "Project has no GND source.".into(),
            component_ids: vec![],
        });
    }

    for component in &project.components {
        for terminal in terminals(component) {
            let terminal_key = (component.id, (*terminal).into());
            let connections = graph.get(&terminal_key).map_or(0, HashSet::len);
            let minimum_connections = if component.kind == "junction" { 2 } else { 1 };
            let junction_like = component.kind == "junction" || component.kind == "net_label";
            if junction_like && connections < minimum_connections {
                diagnostics.push(Diagnostic {
                    code: "disconnected_net",
                    severity: Severity::Warning,
                    message: format!("{} is not connected to a net.", component.name),
                    component_ids: vec![component.id],
                });
            } else if !junction_like && connections == 0 {
                diagnostics.push(Diagnostic {
                    code: "floating_terminal",
                    severity: Severity::Warning,
                    message: format!("{}.{} is floating.", component.name, terminal),
                    component_ids: vec![component.id],
                });
            }
        }
    }

    let gnd_ids: HashSet<_> = gnd.iter().map(|component| component.id).collect();
    for source in vdd {
        let start = (source.id, "out".into());
        let mut queue = VecDeque::from([start.clone()]);
        let mut visited = HashSet::from([start]);
        while let Some(current) = queue.pop_front() {
            if gnd_ids.contains(&current.0) {
                diagnostics.push(Diagnostic {
                    code: "shorted_power_rails",
                    severity: Severity::Error,
                    message: "VDD and GND are connected on the same net.".into(),
                    component_ids: vec![source.id, current.0],
                });
                break;
            }
            for neighbor in graph.get(&current).into_iter().flatten() {
                if visited.insert(neighbor.clone()) {
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }

    let error_count = diagnostics
        .iter()
        .filter(|item| item.severity == Severity::Error)
        .count();
    let warning_count = diagnostics.len() - error_count;
    ValidationReport {
        diagnostics,
        error_count,
        warning_count,
    }
}

pub fn validate_hierarchical(project: &Project) -> ValidationReport {
    fn inspect_scope(
        project: &Project,
        components: &[Component],
        wires: &[crate::model::Wire],
        scope: &str,
        visited: &mut HashSet<Uuid>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for instance in components
            .iter()
            .filter(|component| component.kind == "block")
        {
            let Some(definition_id) = instance.block_definition_id else {
                diagnostics.push(Diagnostic {
                    code: "missing_block_definition",
                    severity: Severity::Error,
                    message: format!(
                        "{scope}/{} has no reusable block definition.",
                        instance.name
                    ),
                    component_ids: vec![instance.id],
                });
                continue;
            };
            let Some(definition) = project.block_definition(definition_id) else {
                diagnostics.push(Diagnostic {
                    code: "missing_block_definition",
                    severity: Severity::Error,
                    message: format!(
                        "{scope}/{} references a missing reusable block.",
                        instance.name
                    ),
                    component_ids: vec![instance.id],
                });
                continue;
            };
            let pins = definition
                .pins
                .iter()
                .map(|pin| pin.name.as_str())
                .collect::<HashSet<_>>();
            for pin in &definition.pins {
                let connected = wires.iter().any(|wire| {
                    (wire.from.component_id == instance.id && wire.from.terminal == pin.name)
                        || wire.to.as_ref().is_some_and(|terminal| {
                            terminal.component_id == instance.id && terminal.terminal == pin.name
                        })
                });
                if !connected {
                    diagnostics.push(Diagnostic {
                        code: "unconnected_block_pin",
                        severity: Severity::Warning,
                        message: format!(
                            "{scope}/{}.{} is not connected.",
                            instance.name, pin.name
                        ),
                        component_ids: vec![instance.id],
                    });
                }
            }
            for terminal in wires
                .iter()
                .flat_map(|wire| std::iter::once(&wire.from).chain(wire.to.as_ref()))
            {
                if terminal.component_id == instance.id
                    && !pins.contains(terminal.terminal.as_str())
                {
                    diagnostics.push(Diagnostic {
                        code: "broken_block_pin",
                        severity: Severity::Error,
                        message: format!(
                            "{scope}/{} has no promoted pin named {}.",
                            instance.name, terminal.terminal
                        ),
                        component_ids: vec![instance.id],
                    });
                }
            }
            if visited.insert(definition.id) {
                inspect_scope(
                    project,
                    &definition.components,
                    &definition.wires,
                    &format!("{scope}/{}", instance.name),
                    visited,
                    diagnostics,
                );
            }
        }
    }

    let mut hierarchy = Vec::new();
    inspect_scope(
        project,
        &project.components,
        &project.wires,
        &project.name,
        &mut HashSet::new(),
        &mut hierarchy,
    );
    let mut report = match project.flattened() {
        Ok(flattened) => validate(&flattened),
        Err(message) => ValidationReport {
            diagnostics: vec![Diagnostic {
                code: "invalid_hierarchy",
                severity: Severity::Error,
                message,
                component_ids: project
                    .components
                    .iter()
                    .filter(|component| component.kind == "block")
                    .map(|component| component.id)
                    .collect(),
            }],
            error_count: 1,
            warning_count: 0,
        },
    };
    hierarchy.append(&mut report.diagnostics);
    let error_count = hierarchy
        .iter()
        .filter(|item| item.severity == Severity::Error)
        .count();
    let warning_count = hierarchy.len() - error_count;
    ValidationReport {
        diagnostics: hierarchy,
        error_count,
        warning_count,
    }
}

#[cfg(test)]
mod tests {
    use super::{validate, validate_hierarchical};
    use crate::model::{Project, TerminalRef};

    #[test]
    fn detects_missing_power_and_floating_terminals() {
        let mut project = Project::default();
        project.add_component("nmos", 0.0, 0.0).unwrap();
        let report = validate(&project);
        assert_eq!(report.error_count, 2);
        assert_eq!(
            report
                .diagnostics
                .iter()
                .filter(|item| item.code == "floating_terminal")
                .count(),
            3
        );
    }

    #[test]
    fn detects_a_power_rail_short() {
        let mut project = Project::default();
        let vdd = project.add_component("vdd", 0.0, -2.0).unwrap();
        let gnd = project.add_component("gnd", 0.0, 2.0).unwrap();
        project
            .connect(
                TerminalRef {
                    component_id: vdd,
                    terminal: "out".into(),
                },
                TerminalRef {
                    component_id: gnd,
                    terminal: "out".into(),
                },
            )
            .unwrap();
        let report = validate(&project);
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.code == "shorted_power_rails"));
    }

    #[test]
    fn dangling_wire_is_a_warning_not_a_corrupt_connection() {
        let mut project = Project::default();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        project
            .connect_to_point(
                TerminalRef {
                    component_id: input,
                    terminal: "out".into(),
                },
                0.0,
                1.0,
            )
            .unwrap();
        let report = validate(&project);
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.code == "dangling_wire" && item.severity == super::Severity::Warning));
        assert!(!report
            .diagnostics
            .iter()
            .any(|item| item.message.contains("missing component")));
    }

    #[test]
    fn hierarchy_reports_missing_definitions_and_broken_pins() {
        let mut source = Project::default();
        source.add_component("input", -2.0, 0.0).unwrap();
        source.add_component("output", 2.0, 0.0).unwrap();
        let definition = source.capture_block("PASS".into()).unwrap();

        let mut parent = Project::default();
        parent.block_definitions = source.block_definitions;
        let instance = parent.place_block(definition, 0.0, 0.0).unwrap();
        parent.wires.push(crate::model::Wire {
            id: uuid::Uuid::new_v4(),
            from: TerminalRef {
                component_id: instance,
                terminal: "REMOVED_PIN".into(),
            },
            to: None,
            end: Some(crate::model::Position {
                x: 2.0,
                y: 0.0,
                z: 0.0,
            }),
            waypoints: Vec::new(),
            route_x: None,
        });
        let report = validate_hierarchical(&parent);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "broken_block_pin"));

        parent.components[0].block_definition_id = Some(uuid::Uuid::new_v4());
        let report = validate_hierarchical(&parent);
        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "missing_block_definition"));
    }
}
