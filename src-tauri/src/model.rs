use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const CURRENT_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub position: Position,
    pub rotation: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRef {
    pub component_id: Uuid,
    pub terminal: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wire {
    pub id: Uuid,
    pub from: TerminalRef,
    pub to: Option<TerminalRef>,
    #[serde(default)]
    pub end: Option<Position>,
    #[serde(default)]
    pub waypoints: Vec<Position>,
    #[serde(default)]
    pub route_x: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub components: Vec<Component>,
    #[serde(default)]
    pub wires: Vec<Wire>,
}

fn terminal_offset(kind: &str, terminal: &str) -> (f64, f64) {
    match (kind, terminal) {
        ("nmos" | "pmos", "gate") => (-1.5, 0.0),
        ("nmos" | "pmos", "drain") => (0.0, -1.4),
        ("nmos" | "pmos", "source") => (0.0, 1.4),
        ("vdd", _) => (0.0, 1.3),
        ("gnd", _) => (0.0, -1.3),
        ("input", _) => (1.5, 0.0),
        ("output" | "net_label", _) => (-1.5, 0.0),
        ("junction", _) => (0.0, 0.0),
        ("resistor", "a") => (-2.4, 0.0),
        ("resistor", _) => (2.4, 0.0),
        _ => (0.0, 0.0),
    }
}

fn point_on_segment(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> bool {
    const TOLERANCE: f64 = 0.2;
    if (start.0 - end.0).abs() < TOLERANCE {
        (point.0 - start.0).abs() <= TOLERANCE
            && point.1 >= start.1.min(end.1) - TOLERANCE
            && point.1 <= start.1.max(end.1) + TOLERANCE
    } else if (start.1 - end.1).abs() < TOLERANCE {
        (point.1 - start.1).abs() <= TOLERANCE
            && point.0 >= start.0.min(end.0) - TOLERANCE
            && point.0 <= start.0.max(end.0) + TOLERANCE
    } else {
        false
    }
}

impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            name: "Untitled chip".into(),
            components: Vec::new(),
            wires: Vec::new(),
        }
    }
}

impl Project {
    pub fn rename(&mut self, name: String) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("circuit name cannot be empty".into());
        }
        self.name = name.into();
        Ok(())
    }

    pub fn add_placeholder(&mut self) {
        let index = self.components.len();
        self.components.push(Component {
            id: Uuid::new_v4(),
            kind: "placeholder".into(),
            name: format!("Component {}", index + 1),
            position: Position {
                x: (index % 3) as f64 * 2.4 - 2.4,
                y: (index / 3) as f64 * 2.0,
                z: 0.0,
            },
            rotation: 0.0,
        });
    }

    pub fn add_resistor(&mut self, x: f64, y: f64) {
        self.add_component("resistor", x, y)
            .expect("resistor is supported");
    }

    pub fn add_component(&mut self, kind: &str, x: f64, y: f64) -> Result<Uuid, String> {
        let prefix = match kind {
            "nmos" => "M",
            "pmos" => "M",
            "vdd" => "VDD",
            "gnd" => "GND",
            "input" => "IN",
            "output" => "OUT",
            "junction" => "J",
            "net_label" => "NET",
            "resistor" => "R",
            _ => return Err(format!("unsupported component kind: {kind}")),
        };
        let number = self
            .components
            .iter()
            .filter(|component| match kind {
                "nmos" | "pmos" => component.kind == "nmos" || component.kind == "pmos",
                _ => component.kind == kind,
            })
            .count()
            + 1;
        let id = Uuid::new_v4();
        self.components.push(Component {
            id,
            kind: kind.into(),
            name: format!("{prefix}{number}"),
            position: Position { x, y, z: 0.0 },
            rotation: 0.0,
        });
        if kind == "junction" {
            self.attach_junction_to_wire(id, x, y);
        }
        Ok(id)
    }

    fn terminal_position(&self, reference: &TerminalRef) -> Option<(f64, f64)> {
        let component = self
            .components
            .iter()
            .find(|component| component.id == reference.component_id)?;
        let (dx, dy) = terminal_offset(&component.kind, &reference.terminal);
        let cosine = component.rotation.cos();
        let sine = component.rotation.sin();
        Some((
            component.position.x + dx * cosine - dy * sine,
            component.position.y + dx * sine + dy * cosine,
        ))
    }

    fn attach_junction_to_wire(&mut self, junction_id: Uuid, x: f64, y: f64) {
        let matching = self.wires.iter().position(|wire| {
            let Some(from) = self.terminal_position(&wire.from) else {
                return false;
            };
            let to = wire
                .to
                .as_ref()
                .and_then(|terminal| self.terminal_position(terminal))
                .or_else(|| wire.end.as_ref().map(|point| (point.x, point.y)));
            let Some(to) = to else {
                return false;
            };
            let route_x = wire.route_x.unwrap_or((from.0 + to.0) / 2.0);
            point_on_segment((x, y), from, (route_x, from.1))
                || point_on_segment((x, y), (route_x, from.1), (route_x, to.1))
                || point_on_segment((x, y), (route_x, to.1), to)
        });
        if let Some(index) = matching {
            let original = self.wires.remove(index);
            let junction = TerminalRef {
                component_id: junction_id,
                terminal: "node".into(),
            };
            self.wires.push(Wire {
                id: Uuid::new_v4(),
                from: original.from,
                to: Some(junction.clone()),
                end: None,
                waypoints: Vec::new(),
                route_x: original.route_x,
            });
            self.wires.push(Wire {
                id: Uuid::new_v4(),
                from: junction,
                to: original.to,
                end: original.end,
                waypoints: original.waypoints,
                route_x: original.route_x,
            });
        }
    }

    pub fn move_component(&mut self, id: Uuid, x: f64, y: f64) -> Result<(), String> {
        let component = self
            .components
            .iter_mut()
            .find(|component| component.id == id)
            .ok_or_else(|| "component not found".to_string())?;
        component.position.x = x;
        component.position.y = y;
        Ok(())
    }

    pub fn rotate_components(&mut self, ids: &[Uuid]) -> Result<(), String> {
        if ids.is_empty() {
            return Err("select at least one component".into());
        }
        for id in ids {
            let component = self
                .components
                .iter_mut()
                .find(|component| component.id == *id)
                .ok_or_else(|| "component not found".to_string())?;
            component.rotation =
                (component.rotation + std::f64::consts::FRAC_PI_2) % std::f64::consts::TAU;
        }
        Ok(())
    }

    pub fn rename_component(&mut self, id: Uuid, name: String) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("component name cannot be empty".into());
        }
        if self
            .components
            .iter()
            .any(|component| component.id != id && component.name == name)
        {
            return Err(format!("component name already exists: {name}"));
        }
        let component = self
            .components
            .iter_mut()
            .find(|component| component.id == id)
            .ok_or_else(|| "component not found".to_string())?;
        component.name = name.into();
        Ok(())
    }

    pub fn delete_components(&mut self, ids: &[Uuid]) -> Result<(), String> {
        if ids.is_empty() {
            return Err("select at least one component".into());
        }
        if ids
            .iter()
            .any(|id| !self.components.iter().any(|component| component.id == *id))
        {
            return Err("component not found".into());
        }
        self.components
            .retain(|component| !ids.contains(&component.id));
        self.wires.retain(|wire| {
            !ids.contains(&wire.from.component_id)
                && wire
                    .to
                    .as_ref()
                    .is_none_or(|terminal| !ids.contains(&terminal.component_id))
        });
        Ok(())
    }

    pub fn connect(&mut self, from: TerminalRef, to: TerminalRef) -> Result<(), String> {
        if from == to {
            return Err("cannot connect a terminal to itself".into());
        }
        for terminal in [&from, &to] {
            if !self
                .components
                .iter()
                .any(|component| component.id == terminal.component_id)
            {
                return Err("wire references a missing component".into());
            }
        }
        if self.wires.iter().any(|wire| {
            (wire.from == from && wire.to.as_ref() == Some(&to))
                || (wire.from == to && wire.to.as_ref() == Some(&from))
        }) {
            return Err("these terminals are already connected".into());
        }
        self.wires.push(Wire {
            id: Uuid::new_v4(),
            from,
            to: Some(to),
            end: None,
            waypoints: Vec::new(),
            route_x: None,
        });
        Ok(())
    }

    pub fn connect_to_point(&mut self, from: TerminalRef, x: f64, y: f64) -> Result<(), String> {
        if !self
            .components
            .iter()
            .any(|component| component.id == from.component_id)
        {
            return Err("wire references a missing component".into());
        }
        self.wires.push(Wire {
            id: Uuid::new_v4(),
            from,
            to: None,
            end: Some(Position { x, y, z: 0.0 }),
            waypoints: vec![Position { x, y, z: 0.0 }],
            route_x: None,
        });
        Ok(())
    }

    pub fn extend_wire(&mut self, id: Uuid, x: f64, y: f64) -> Result<(), String> {
        let wire = self
            .wires
            .iter_mut()
            .find(|wire| wire.id == id)
            .ok_or_else(|| "wire not found".to_string())?;
        if wire.to.is_some() {
            return Err("wire is already connected".into());
        }
        let point = Position { x, y, z: 0.0 };
        if wire
            .waypoints
            .last()
            .is_none_or(|last| last.x != x || last.y != y)
        {
            wire.waypoints.push(point.clone());
        }
        wire.end = Some(point);
        Ok(())
    }

    pub fn finish_wire(&mut self, id: Uuid, to: TerminalRef) -> Result<(), String> {
        if !self
            .components
            .iter()
            .any(|component| component.id == to.component_id)
        {
            return Err("wire references a missing component".into());
        }
        let wire = self
            .wires
            .iter_mut()
            .find(|wire| wire.id == id)
            .ok_or_else(|| "wire not found".to_string())?;
        if wire.to.is_some() {
            return Err("wire is already connected".into());
        }
        if wire.from == to {
            return Err("cannot connect a terminal to itself".into());
        }
        wire.to = Some(to);
        wire.end = None;
        Ok(())
    }

    pub fn move_wire(&mut self, id: Uuid, route_x: f64) -> Result<(), String> {
        let wire = self
            .wires
            .iter_mut()
            .find(|wire| wire.id == id)
            .ok_or_else(|| "wire not found".to_string())?;
        wire.route_x = Some(route_x);
        Ok(())
    }

    pub fn delete_wire(&mut self, id: Uuid) -> Result<(), String> {
        let length = self.wires.len();
        self.wires.retain(|wire| wire.id != id);
        if self.wires.len() == length {
            return Err("wire not found".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Project, TerminalRef};

    #[test]
    fn circuit_name_is_validated_and_persisted_in_the_model() {
        let mut project = Project::default();
        project.rename("CMOS Inverter".into()).unwrap();
        assert_eq!(project.name, "CMOS Inverter");
        assert!(project.rename("  ".into()).is_err());
    }

    #[test]
    fn resistors_receive_sequential_reference_designators() {
        let mut project = Project::default();
        project.add_resistor(2.0, -1.0);
        project.add_resistor(0.0, 0.0);
        assert_eq!(project.components[0].name, "R1");
        assert_eq!(project.components[1].name, "R2");
        assert_eq!(project.components[0].position.x, 2.0);
        assert_eq!(project.components[0].position.y, -1.0);
    }

    #[test]
    fn transistor_terminals_can_be_connected_and_components_moved() {
        let mut project = Project::default();
        let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
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
        project.move_component(nmos, 2.0, 1.0).unwrap();
        let wire = project.wires[0].id;
        project.move_wire(wire, -1.0).unwrap();
        assert_eq!(project.wires.len(), 1);
        assert_eq!(project.wires[0].route_x, Some(-1.0));
        assert_eq!(project.components[0].position.x, 2.0);
    }

    #[test]
    fn deleting_devices_removes_attached_wires() {
        let mut project = Project::default();
        let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
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
        project.rotate_components(&[nmos, input]).unwrap();
        project.delete_components(&[nmos]).unwrap();
        assert_eq!(project.components.len(), 1);
        assert!(project.wires.is_empty());
        assert_eq!(project.components[0].rotation, std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn junctions_branch_and_net_labels_can_be_renamed() {
        let mut project = Project::default();
        let junction = project.add_component("junction", 0.0, 0.0).unwrap();
        let input = project.add_component("input", -3.0, 0.0).unwrap();
        let output = project.add_component("output", 3.0, 0.0).unwrap();
        let label = project.add_component("net_label", 0.0, -2.0).unwrap();
        for (component_id, terminal) in [(input, "out"), (output, "in"), (label, "node")] {
            project
                .connect(
                    TerminalRef {
                        component_id: junction,
                        terminal: "node".into(),
                    },
                    TerminalRef {
                        component_id,
                        terminal: terminal.into(),
                    },
                )
                .unwrap();
        }
        project.rename_component(label, "OUT_NET".into()).unwrap();
        assert_eq!(project.wires.len(), 3);
        assert_eq!(
            project
                .components
                .iter()
                .find(|component| component.id == label)
                .unwrap()
                .name,
            "OUT_NET"
        );
    }

    #[test]
    fn placing_a_junction_on_a_wire_splits_and_attaches_it() {
        let mut project = Project::default();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        let output = project.add_component("output", 2.0, 0.0).unwrap();
        project
            .connect(
                TerminalRef {
                    component_id: input,
                    terminal: "out".into(),
                },
                TerminalRef {
                    component_id: output,
                    terminal: "in".into(),
                },
            )
            .unwrap();
        let junction = project.add_component("junction", 0.0, 0.0).unwrap();
        assert_eq!(project.wires.len(), 2);
        assert!(project.wires.iter().all(|wire| {
            wire.from.component_id == junction
                || wire
                    .to
                    .as_ref()
                    .is_some_and(|terminal| terminal.component_id == junction)
        }));
        let wire = project.wires[0].id;
        project.delete_wire(wire).unwrap();
        assert_eq!(project.wires.len(), 1);
    }

    #[test]
    fn wire_can_end_on_the_grid_and_be_finished_later() {
        let mut project = Project::default();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        let nmos = project.add_component("nmos", 2.0, 0.0).unwrap();
        project
            .connect_to_point(
                TerminalRef {
                    component_id: input,
                    terminal: "out".into(),
                },
                0.0,
                2.0,
            )
            .unwrap();
        let wire = project.wires[0].id;
        assert!(project.wires[0].to.is_none());
        assert_eq!(project.wires[0].end.as_ref().unwrap().x, 0.0);
        project.extend_wire(wire, 1.0, 3.0).unwrap();
        assert_eq!(project.wires[0].waypoints.len(), 2);
        assert_eq!(project.wires[0].end.as_ref().unwrap().y, 3.0);

        project
            .finish_wire(
                wire,
                TerminalRef {
                    component_id: nmos,
                    terminal: "gate".into(),
                },
            )
            .unwrap();
        assert!(project.wires[0].to.is_some());
        assert!(project.wires[0].end.is_none());
    }
}
