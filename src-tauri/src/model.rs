use crate::technology::Technology;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    #[serde(default)]
    pub device_geometry: Option<DeviceGeometry>,
    #[serde(default)]
    pub block_definition_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceGeometry {
    pub width_um: f64,
    pub length_um: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCharacteristics {
    pub width_um: f64,
    pub length_um: f64,
    pub effective_on_resistance_ohms: f64,
    pub gate_capacitance_ff: f64,
    pub diffusion_capacitance_ff: f64,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockPinRole {
    Input,
    Output,
    Power,
    Ground,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockPin {
    pub name: String,
    pub role: BlockPinRole,
    pub component_id: Uuid,
    pub terminal: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockDefinition {
    pub id: Uuid,
    pub name: String,
    pub components: Vec<Component>,
    pub wires: Vec<Wire>,
    pub pins: Vec<BlockPin>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaveformRadix {
    Binary,
    Hex,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveformGroup {
    pub id: Uuid,
    pub name: String,
    pub signals: Vec<String>,
    pub radix: WaveformRadix,
    pub collapsed: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub format_version: u32,
    pub name: String,
    pub components: Vec<Component>,
    #[serde(default)]
    pub wires: Vec<Wire>,
    #[serde(default)]
    pub technology: Technology,
    #[serde(default)]
    pub block_definitions: Vec<BlockDefinition>,
    #[serde(default)]
    pub waveform_groups: Vec<WaveformGroup>,
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

fn derived_uuid(instance: Uuid, source: Uuid, discriminator: u8) -> Uuid {
    let mut bytes = *source.as_bytes();
    for (index, byte) in instance.as_bytes().iter().enumerate() {
        bytes[index] ^= byte;
    }
    bytes[0] ^= discriminator;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

impl Default for Project {
    fn default() -> Self {
        Self {
            format_version: CURRENT_FORMAT_VERSION,
            name: "Untitled chip".into(),
            components: Vec::new(),
            wires: Vec::new(),
            technology: Technology::default(),
            block_definitions: Vec::new(),
            waveform_groups: Vec::new(),
        }
    }
}

impl Project {
    pub fn set_technology(&mut self, technology: Technology) {
        self.technology = technology;
    }

    pub fn reset_technology(&mut self) {
        self.technology = Technology::default();
    }

    pub fn rename(&mut self, name: String) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("circuit name cannot be empty".into());
        }
        self.name = name.into();
        Ok(())
    }

    pub fn set_waveform_groups(&mut self, groups: Vec<WaveformGroup>) -> Result<(), String> {
        let available = self
            .components
            .iter()
            .filter(|component| matches!(component.kind.as_str(), "input" | "output"))
            .map(|component| component.name.as_str())
            .collect::<HashSet<_>>();
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        let mut claimed_signals = HashSet::new();
        for group in &groups {
            let name = group.name.trim();
            if name.is_empty() {
                return Err("waveform group name cannot be empty".into());
            }
            if !ids.insert(group.id) || !names.insert(name.to_ascii_lowercase()) {
                return Err("waveform group IDs and names must be unique".into());
            }
            if group.signals.len() < 2 {
                return Err(format!("waveform group {name} needs at least two signals"));
            }
            let mut members = HashSet::new();
            for signal in &group.signals {
                if !available.contains(signal.as_str()) {
                    return Err(format!(
                        "waveform group {name} references missing signal {signal}"
                    ));
                }
                if !members.insert(signal.as_str()) {
                    return Err(format!("waveform group {name} repeats signal {signal}"));
                }
                if !claimed_signals.insert(signal.as_str()) {
                    return Err(format!(
                        "signal {signal} belongs to more than one waveform group"
                    ));
                }
            }
        }
        self.waveform_groups = groups
            .into_iter()
            .map(|mut group| {
                group.name = group.name.trim().into();
                group
            })
            .collect();
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
            device_geometry: None,
            block_definition_id: None,
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
        let device_geometry = match kind {
            "nmos" => Some(DeviceGeometry {
                width_um: self.technology.nmos.reference_width_um,
                length_um: self.technology.nmos.reference_length_um,
            }),
            "pmos" => Some(DeviceGeometry {
                width_um: self.technology.pmos.reference_width_um,
                length_um: self.technology.pmos.reference_length_um,
            }),
            _ => None,
        };
        self.components.push(Component {
            id,
            kind: kind.into(),
            name: format!("{prefix}{number}"),
            position: Position { x, y, z: 0.0 },
            rotation: 0.0,
            device_geometry,
            block_definition_id: None,
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
        let old_name = component.name.clone();
        let waveform_signal = matches!(component.kind.as_str(), "input" | "output");
        component.name = name.into();
        if waveform_signal {
            for group in &mut self.waveform_groups {
                for signal in &mut group.signals {
                    if *signal == old_name {
                        *signal = name.into();
                    }
                }
            }
        }
        Ok(())
    }

    pub fn set_device_geometry(
        &mut self,
        id: Uuid,
        width_um: f64,
        length_um: f64,
    ) -> Result<(), String> {
        if !width_um.is_finite() || width_um <= 0.0 {
            return Err("transistor width must be a finite positive value".into());
        }
        if !length_um.is_finite() || length_um <= 0.0 {
            return Err("transistor length must be a finite positive value".into());
        }
        let component = self
            .components
            .iter_mut()
            .find(|component| component.id == id)
            .ok_or_else(|| "component not found".to_string())?;
        if component.kind != "nmos" && component.kind != "pmos" {
            return Err("device geometry is only available for NMOS and PMOS transistors".into());
        }
        component.device_geometry = Some(DeviceGeometry {
            width_um,
            length_um,
        });
        Ok(())
    }

    pub fn device_characteristics(&self, id: Uuid) -> Result<DeviceCharacteristics, String> {
        let component = self
            .components
            .iter()
            .find(|component| component.id == id)
            .ok_or_else(|| "component not found".to_string())?;
        let technology = match component.kind.as_str() {
            "nmos" => &self.technology.nmos,
            "pmos" => &self.technology.pmos,
            _ => return Err("device characteristics are only available for transistors".into()),
        };
        let geometry = component.device_geometry.clone().unwrap_or(DeviceGeometry {
            width_um: technology.reference_width_um,
            length_um: technology.reference_length_um,
        });
        Ok(DeviceCharacteristics {
            width_um: geometry.width_um,
            length_um: geometry.length_um,
            effective_on_resistance_ohms: technology.nominal_on_resistance_ohms
                * (geometry.length_um / technology.reference_length_um)
                * (technology.reference_width_um / geometry.width_um),
            gate_capacitance_ff: technology.gate_capacitance_ff_per_um
                * geometry.width_um
                * (geometry.length_um / technology.reference_length_um),
            diffusion_capacitance_ff: technology.diffusion_capacitance_ff_per_um
                * geometry.width_um,
        })
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
        let removed_signals = self
            .components
            .iter()
            .filter(|component| {
                ids.contains(&component.id) && matches!(component.kind.as_str(), "input" | "output")
            })
            .map(|component| component.name.clone())
            .collect::<HashSet<_>>();
        self.components
            .retain(|component| !ids.contains(&component.id));
        self.wires.retain(|wire| {
            !ids.contains(&wire.from.component_id)
                && wire
                    .to
                    .as_ref()
                    .is_none_or(|terminal| !ids.contains(&terminal.component_id))
        });
        for group in &mut self.waveform_groups {
            group
                .signals
                .retain(|signal| !removed_signals.contains(signal));
        }
        self.waveform_groups
            .retain(|group| group.signals.len() >= 2);
        Ok(())
    }

    pub fn connect(&mut self, from: TerminalRef, to: TerminalRef) -> Result<(), String> {
        if from == to {
            return Err("cannot connect a terminal to itself".into());
        }
        for terminal in [&from, &to] {
            let component = self
                .components
                .iter()
                .find(|component| component.id == terminal.component_id)
                .ok_or_else(|| "wire references a missing component".to_string())?;
            if !self
                .component_terminals(component)
                .contains(&terminal.terminal)
            {
                return Err(format!(
                    "{} has no terminal named {}",
                    component.name, terminal.terminal
                ));
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
        let component = self
            .components
            .iter()
            .find(|component| component.id == to.component_id)
            .ok_or_else(|| "wire references a missing component".to_string())?;
        if !self.component_terminals(component).contains(&to.terminal) {
            return Err(format!(
                "{} has no terminal named {}",
                component.name, to.terminal
            ));
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

    pub fn capture_block(&mut self, name: String) -> Result<Uuid, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("block name cannot be empty".into());
        }
        if self
            .block_definitions
            .iter()
            .any(|definition| definition.name == name)
        {
            return Err(format!("block definition already exists: {name}"));
        }
        let pins = self
            .components
            .iter()
            .filter_map(|component| {
                let (role, terminal) = match component.kind.as_str() {
                    "input" => (BlockPinRole::Input, "out"),
                    "output" => (BlockPinRole::Output, "in"),
                    "vdd" => (BlockPinRole::Power, "out"),
                    "gnd" => (BlockPinRole::Ground, "out"),
                    _ => return None,
                };
                Some(BlockPin {
                    name: component.name.clone(),
                    role,
                    component_id: component.id,
                    terminal: terminal.into(),
                })
            })
            .collect::<Vec<_>>();
        if !pins.iter().any(|pin| pin.role == BlockPinRole::Input)
            || !pins.iter().any(|pin| pin.role == BlockPinRole::Output)
        {
            return Err("a reusable block requires at least one input and output pin".into());
        }
        let id = Uuid::new_v4();
        let definition = BlockDefinition {
            id,
            name: name.into(),
            components: self.components.clone(),
            wires: self.wires.clone(),
            pins,
        };
        let mut candidate = self.clone();
        candidate.block_definitions.push(definition.clone());
        candidate.validate_block_graph()?;
        self.block_definitions.push(definition);
        Ok(id)
    }

    pub fn update_block_from_current(&mut self, definition_id: Uuid) -> Result<(), String> {
        let existing = self
            .block_definition(definition_id)
            .cloned()
            .ok_or_else(|| "block definition not found".to_string())?;
        let mut snapshot = self.clone();
        snapshot
            .block_definitions
            .iter_mut()
            .find(|definition| definition.id == definition_id)
            .expect("definition exists")
            .name = format!("__updating_{definition_id}");
        let replacement_id = snapshot.capture_block(existing.name.clone())?;
        let mut replacement = snapshot
            .block_definitions
            .into_iter()
            .find(|definition| definition.id == replacement_id)
            .expect("captured definition exists");
        let interface = |definition: &BlockDefinition| {
            definition
                .pins
                .iter()
                .map(|pin| (pin.name.clone(), pin.role))
                .collect::<Vec<_>>()
        };
        if interface(&existing) != interface(&replacement) {
            return Err(
                "block pin names and roles must remain compatible with existing instances".into(),
            );
        }
        replacement.id = definition_id;
        let index = self
            .block_definitions
            .iter()
            .position(|definition| definition.id == definition_id)
            .expect("definition exists");
        let mut candidate = self.clone();
        candidate.block_definitions[index] = replacement.clone();
        candidate.validate_block_graph()?;
        self.block_definitions[index] = replacement;
        Ok(())
    }

    pub fn place_block(&mut self, definition_id: Uuid, x: f64, y: f64) -> Result<Uuid, String> {
        let definition = self
            .block_definitions
            .iter()
            .find(|definition| definition.id == definition_id)
            .ok_or_else(|| "block definition not found".to_string())?;
        let number = self
            .components
            .iter()
            .filter(|component| component.block_definition_id == Some(definition_id))
            .count()
            + 1;
        let id = Uuid::new_v4();
        self.components.push(Component {
            id,
            kind: "block".into(),
            name: format!("X{}{}", definition.name, number),
            position: Position { x, y, z: 0.0 },
            rotation: 0.0,
            device_geometry: None,
            block_definition_id: Some(definition_id),
        });
        Ok(id)
    }

    pub fn block_definition(&self, id: Uuid) -> Option<&BlockDefinition> {
        self.block_definitions
            .iter()
            .find(|definition| definition.id == id)
    }

    pub fn component_terminals(&self, component: &Component) -> Vec<String> {
        if component.kind == "block" {
            return component
                .block_definition_id
                .and_then(|id| self.block_definition(id))
                .map(|definition| definition.pins.iter().map(|pin| pin.name.clone()).collect())
                .unwrap_or_default();
        }
        match component.kind.as_str() {
            "nmos" | "pmos" => vec!["gate".into(), "drain".into(), "source".into()],
            "resistor" => vec!["a".into(), "b".into()],
            "output" => vec!["in".into()],
            "junction" | "net_label" => vec!["node".into()],
            _ => vec!["out".into()],
        }
    }

    pub fn validate_block_graph(&self) -> Result<(), String> {
        fn visit(
            project: &Project,
            definition_id: Uuid,
            visiting: &mut Vec<Uuid>,
            visited: &mut HashSet<Uuid>,
        ) -> Result<(), String> {
            if let Some(index) = visiting.iter().position(|id| *id == definition_id) {
                let mut names = visiting[index..]
                    .iter()
                    .filter_map(|id| project.block_definition(*id))
                    .map(|definition| definition.name.clone())
                    .collect::<Vec<_>>();
                names.push(
                    project
                        .block_definition(definition_id)
                        .map(|definition| definition.name.clone())
                        .unwrap_or_else(|| definition_id.to_string()),
                );
                return Err(format!(
                    "recursive block definition cycle: {}",
                    names.join(" -> ")
                ));
            }
            if visited.contains(&definition_id) {
                return Ok(());
            }
            let definition = project
                .block_definition(definition_id)
                .ok_or_else(|| format!("missing block definition {definition_id}"))?;
            visiting.push(definition_id);
            for instance in definition
                .components
                .iter()
                .filter(|component| component.kind == "block")
            {
                let child_id = instance.block_definition_id.ok_or_else(|| {
                    format!(
                        "{} in {} has no block definition",
                        instance.name, definition.name
                    )
                })?;
                if project.block_definition(child_id).is_none() {
                    return Err(format!(
                        "{} in {} references missing block definition {}",
                        instance.name, definition.name, child_id
                    ));
                }
                visit(project, child_id, visiting, visited)?;
            }
            visiting.pop();
            visited.insert(definition_id);
            Ok(())
        }

        let mut visited = HashSet::new();
        for definition in &self.block_definitions {
            visit(self, definition.id, &mut Vec::new(), &mut visited)?;
        }
        Ok(())
    }

    pub fn flattened(&self) -> Result<Project, String> {
        self.validate_block_graph()?;
        let mut flattened = Project {
            format_version: self.format_version,
            name: self.name.clone(),
            components: self
                .components
                .iter()
                .filter(|component| component.kind != "block")
                .cloned()
                .collect(),
            wires: Vec::new(),
            technology: self.technology.clone(),
            block_definitions: Vec::new(),
            waveform_groups: self.waveform_groups.clone(),
        };
        let mut instance_terminals: HashMap<(Uuid, String), TerminalRef> = HashMap::new();
        for instance in self
            .components
            .iter()
            .filter(|component| component.kind == "block")
        {
            let definition_id = instance
                .block_definition_id
                .ok_or_else(|| format!("{} has no block definition", instance.name))?;
            let definition = self.block_definition(definition_id).ok_or_else(|| {
                format!("{} references a missing block definition", instance.name)
            })?;
            let pins = self.expand_block_definition(
                definition,
                instance.id,
                &instance.name,
                &instance.position,
                &mut flattened,
            )?;
            for (pin, terminal) in pins {
                instance_terminals.insert((instance.id, pin), terminal);
            }
        }
        for source in &self.wires {
            let mut wire = source.clone();
            if let Some(replacement) =
                instance_terminals.get(&(wire.from.component_id, wire.from.terminal.clone()))
            {
                wire.from = replacement.clone();
            }
            if let Some(to) = &mut wire.to {
                if let Some(replacement) =
                    instance_terminals.get(&(to.component_id, to.terminal.clone()))
                {
                    *to = replacement.clone();
                }
            }
            flattened.wires.push(wire);
        }
        Ok(flattened)
    }

    fn expand_block_definition(
        &self,
        definition: &BlockDefinition,
        namespace: Uuid,
        prefix: &str,
        offset: &Position,
        flattened: &mut Project,
    ) -> Result<HashMap<String, TerminalRef>, String> {
        let mut component_ids = HashMap::new();
        let mut child_terminals = HashMap::new();
        let boundary_components = definition
            .pins
            .iter()
            .map(|pin| pin.component_id)
            .collect::<HashSet<_>>();

        for source in &definition.components {
            if source.kind == "block" {
                let child_id = source.block_definition_id.ok_or_else(|| {
                    format!(
                        "{} in {} has no block definition",
                        source.name, definition.name
                    )
                })?;
                let child = self.block_definition(child_id).ok_or_else(|| {
                    format!(
                        "{} in {} references a missing block definition",
                        source.name, definition.name
                    )
                })?;
                let child_namespace = derived_uuid(namespace, source.id, 0x42);
                let child_prefix = format!("{prefix}·{}", source.name);
                let child_offset = Position {
                    x: offset.x + source.position.x,
                    y: offset.y + source.position.y,
                    z: offset.z + source.position.z,
                };
                for (pin, terminal) in self.expand_block_definition(
                    child,
                    child_namespace,
                    &child_prefix,
                    &child_offset,
                    flattened,
                )? {
                    child_terminals.insert((source.id, pin), terminal);
                }
                continue;
            }

            let id = derived_uuid(namespace, source.id, 0x43);
            component_ids.insert(source.id, id);
            let mut component = source.clone();
            component.id = id;
            component.name = format!("{prefix}·{}", source.name);
            component.position.x += offset.x;
            component.position.y += offset.y;
            component.position.z += offset.z;
            if boundary_components.contains(&source.id) {
                component.kind = "junction".into();
                component.block_definition_id = None;
            }
            flattened.components.push(component);
        }

        let map_terminal = |terminal: &TerminalRef| -> Result<TerminalRef, String> {
            if let Some(replacement) =
                child_terminals.get(&(terminal.component_id, terminal.terminal.clone()))
            {
                return Ok(replacement.clone());
            }
            let component_id = component_ids
                .get(&terminal.component_id)
                .copied()
                .ok_or_else(|| {
                    format!(
                        "{} contains a wire to missing component {}",
                        definition.name, terminal.component_id
                    )
                })?;
            Ok(TerminalRef {
                component_id,
                terminal: if boundary_components.contains(&terminal.component_id) {
                    "node".into()
                } else {
                    terminal.terminal.clone()
                },
            })
        };

        for source in &definition.wires {
            let mut wire = source.clone();
            wire.id = derived_uuid(namespace, source.id, 0x57);
            wire.from = map_terminal(&source.from)?;
            wire.to = source.to.as_ref().map(&map_terminal).transpose()?;
            if let Some(end) = &mut wire.end {
                end.x += offset.x;
                end.y += offset.y;
                end.z += offset.z;
            }
            for waypoint in &mut wire.waypoints {
                waypoint.x += offset.x;
                waypoint.y += offset.y;
                waypoint.z += offset.z;
            }
            if let Some(route_x) = &mut wire.route_x {
                *route_x += offset.x;
            }
            flattened.wires.push(wire);
        }

        definition
            .pins
            .iter()
            .map(|pin| {
                let component_id =
                    component_ids
                        .get(&pin.component_id)
                        .copied()
                        .ok_or_else(|| {
                            format!(
                                "{} pin {} references a missing boundary component",
                                definition.name, pin.name
                            )
                        })?;
                Ok((
                    pin.name.clone(),
                    TerminalRef {
                        component_id,
                        terminal: "node".into(),
                    },
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Component, Position, Project, TerminalRef, WaveformGroup, WaveformRadix};
    use crate::technology::Technology;
    use uuid::Uuid;

    #[test]
    fn circuit_name_is_validated_and_persisted_in_the_model() {
        let mut project = Project::default();
        project.rename("CMOS Inverter".into()).unwrap();
        assert_eq!(project.name, "CMOS Inverter");
        assert!(project.rename("  ".into()).is_err());
    }

    #[test]
    fn waveform_groups_validate_order_and_round_trip() {
        let mut project = Project::default();
        let a = project.add_component("input", 0.0, 0.0).unwrap();
        let b = project.add_component("input", 0.0, 2.0).unwrap();
        let names = project
            .components
            .iter()
            .filter(|component| component.id == a || component.id == b)
            .map(|component| component.name.clone())
            .collect::<Vec<_>>();
        let group = WaveformGroup {
            id: Uuid::new_v4(),
            name: "DATA".into(),
            signals: names.clone(),
            radix: WaveformRadix::Hex,
            collapsed: true,
        };
        project.set_waveform_groups(vec![group.clone()]).unwrap();
        let mut restored: Project =
            serde_json::from_str(&serde_json::to_string(&project).unwrap()).unwrap();
        assert_eq!(restored.waveform_groups, vec![group.clone()]);
        assert_eq!(restored.waveform_groups[0].signals, names);
        restored.rename_component(a, "D7".into()).unwrap();
        assert_eq!(restored.waveform_groups[0].signals[0], "D7");
        restored.delete_components(&[b]).unwrap();
        assert!(restored.waveform_groups.is_empty());

        let mut invalid = vec![group];
        invalid[0].signals = vec!["MISSING".into(), "ALSO_MISSING".into()];
        assert!(project.set_waveform_groups(invalid).is_err());
    }

    #[test]
    fn older_projects_receive_the_builtin_technology() {
        let project = Project::default();
        let mut serialized = serde_json::to_value(project).unwrap();
        serialized.as_object_mut().unwrap().remove("technology");
        let loaded: Project = serde_json::from_value(serialized).unwrap();
        assert_eq!(loaded.technology, crate::technology::Technology::default());
    }

    #[test]
    fn older_transistors_use_reference_geometry() {
        let mut project = Project::default();
        let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
        let mut serialized = serde_json::to_value(project).unwrap();
        serialized["components"][0]
            .as_object_mut()
            .unwrap()
            .remove("deviceGeometry");
        let loaded: Project = serde_json::from_value(serialized).unwrap();
        let characteristics = loaded.device_characteristics(nmos).unwrap();
        assert_eq!(
            characteristics.width_um,
            loaded.technology.nmos.reference_width_um
        );
        assert_eq!(
            characteristics.length_um,
            loaded.technology.nmos.reference_length_um
        );
    }

    #[test]
    fn device_geometry_scales_resistance_and_capacitance() {
        let mut project = Project::default();
        let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
        let nominal = project.device_characteristics(nmos).unwrap();

        project.set_device_geometry(nmos, 2.0, 1.0).unwrap();
        let wider = project.device_characteristics(nmos).unwrap();
        assert!(wider.effective_on_resistance_ohms < nominal.effective_on_resistance_ohms);
        assert!(wider.gate_capacitance_ff > nominal.gate_capacitance_ff);
        assert!(wider.diffusion_capacitance_ff > nominal.diffusion_capacitance_ff);

        project.set_device_geometry(nmos, 1.0, 2.0).unwrap();
        let longer = project.device_characteristics(nmos).unwrap();
        assert!(longer.effective_on_resistance_ohms > nominal.effective_on_resistance_ohms);
        assert!(longer.gate_capacitance_ff > nominal.gate_capacitance_ff);
    }

    #[test]
    fn device_geometry_round_trips_with_the_project() {
        let mut project = Project::default();
        let pmos = project.add_component("pmos", 0.0, 0.0).unwrap();
        project.set_device_geometry(pmos, 3.2, 0.8).unwrap();
        let serialized = serde_json::to_string(&project).unwrap();
        let loaded: Project = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            loaded.device_characteristics(pmos).unwrap(),
            project.device_characteristics(pmos).unwrap()
        );
    }

    #[test]
    fn changing_technology_preserves_schematic_topology() {
        let mut project = Project::default();
        let input = project.add_component("input", -4.0, 0.0).unwrap();
        let nmos = project.add_component("nmos", 0.0, 0.0).unwrap();
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
        let components = project.components.clone();
        let wires = project.wires.clone();
        let mut technology = Technology::default();
        technology.name = "Topology-safe technology".into();

        project.set_technology(technology);

        assert_eq!(project.components, components);
        assert_eq!(project.wires, wires);
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

    #[test]
    fn nested_blocks_flatten_recursively_and_preserve_promoted_pins() {
        let mut leaf = Project::default();
        let leaf_input = leaf.add_component("input", -2.0, 0.0).unwrap();
        let leaf_output = leaf.add_component("output", 2.0, 0.0).unwrap();
        leaf.connect(
            TerminalRef {
                component_id: leaf_input,
                terminal: "out".into(),
            },
            TerminalRef {
                component_id: leaf_output,
                terminal: "in".into(),
            },
        )
        .unwrap();
        let leaf_id = leaf.capture_block("PASS".into()).unwrap();

        let mut composite = Project::default();
        composite.block_definitions = leaf.block_definitions;
        let input = composite.add_component("input", -4.0, 0.0).unwrap();
        let output = composite.add_component("output", 4.0, 0.0).unwrap();
        let child = composite.place_block(leaf_id, 0.0, 0.0).unwrap();
        composite
            .connect(
                TerminalRef {
                    component_id: input,
                    terminal: "out".into(),
                },
                TerminalRef {
                    component_id: child,
                    terminal: "IN1".into(),
                },
            )
            .unwrap();
        composite
            .connect(
                TerminalRef {
                    component_id: child,
                    terminal: "OUT1".into(),
                },
                TerminalRef {
                    component_id: output,
                    terminal: "in".into(),
                },
            )
            .unwrap();
        let composite_id = composite.capture_block("DOUBLE_PASS".into()).unwrap();

        let mut parent = Project::default();
        parent.block_definitions = composite.block_definitions;
        let instance = parent.place_block(composite_id, 10.0, 20.0).unwrap();
        let source = parent.add_component("input", 0.0, 20.0).unwrap();
        parent
            .connect(
                TerminalRef {
                    component_id: source,
                    terminal: "out".into(),
                },
                TerminalRef {
                    component_id: instance,
                    terminal: "IN1".into(),
                },
            )
            .unwrap();

        let flattened = parent.flattened().unwrap();
        assert!(flattened
            .components
            .iter()
            .all(|component| component.kind != "block"));
        assert!(flattened
            .components
            .iter()
            .any(|component| component.name.contains("DOUBLE_PASS")
                && component.name.contains("PASS")));
        let parent_wire = flattened
            .wires
            .iter()
            .find(|wire| wire.id == parent.wires[0].id)
            .unwrap();
        assert_ne!(parent_wire.to.as_ref().unwrap().component_id, instance);
        assert_eq!(parent_wire.to.as_ref().unwrap().terminal, "node");
    }

    #[test]
    fn block_graph_rejects_indirect_recursion() {
        let mut project = Project::default();
        project.add_component("input", -2.0, 0.0).unwrap();
        project.add_component("output", 2.0, 0.0).unwrap();
        let first = project.capture_block("FIRST".into()).unwrap();
        let second = project.capture_block("SECOND".into()).unwrap();
        let first_definition = project
            .block_definitions
            .iter_mut()
            .find(|definition| definition.id == first)
            .unwrap();
        first_definition.components.push(Component {
            id: Uuid::new_v4(),
            kind: "block".into(),
            name: "XSECOND1".into(),
            position: Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: 0.0,
            device_geometry: None,
            block_definition_id: Some(second),
        });
        let second_definition = project
            .block_definitions
            .iter_mut()
            .find(|definition| definition.id == second)
            .unwrap();
        second_definition.components.push(Component {
            id: Uuid::new_v4(),
            kind: "block".into(),
            name: "XFIRST1".into(),
            position: Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: 0.0,
            device_geometry: None,
            block_definition_id: Some(first),
        });

        let error = project.validate_block_graph().unwrap_err();
        assert!(error.contains("FIRST -> SECOND -> FIRST"));
    }
}
