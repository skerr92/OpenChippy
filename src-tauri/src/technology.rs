use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CURRENT_TECHNOLOGY_FORMAT_VERSION: u32 = 1;
pub const DEFAULT_MAX_METAL_LAYERS: u16 = 5;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Technology {
    pub format_version: u32,
    pub name: String,
    pub supply_voltage: f64,
    #[serde(default = "default_max_metal_layers")]
    pub max_metal_layers: u16,
    pub nmos: MosTechnology,
    pub pmos: MosTechnology,
    #[serde(default)]
    pub physical_rules: PhysicalRuleDeck,
    #[serde(default)]
    pub physical_planning: PhysicalPlanningRules,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct MosTechnology {
    #[serde(alias = "vt")]
    pub threshold_voltage: f64,
    #[serde(alias = "ron")]
    pub nominal_on_resistance_ohms: f64,
    #[serde(alias = "reference_width")]
    pub reference_width_um: f64,
    #[serde(alias = "reference_length")]
    pub reference_length_um: f64,
    pub gate_capacitance_ff_per_um: f64,
    pub diffusion_capacitance_ff_per_um: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct PhysicalRuleDeck {
    pub format_version: u32,
    pub database_units_per_micron: u32,
    pub manufacturing_grid_um: f64,
    pub diffusion: LayerRule,
    pub poly: LayerRule,
    pub well: LayerRule,
    pub metal: LayerRule,
    pub contact: CutRule,
    pub via: CutRule,
    pub gate_extension_um: f64,
    pub well_enclosure_um: f64,
    #[serde(default)]
    pub layer_overrides: BTreeMap<String, LayerRule>,
    #[serde(default)]
    pub via_overrides: BTreeMap<String, CutRule>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct LayerRule {
    pub min_width_um: f64,
    pub min_spacing_um: f64,
    pub min_area_um2: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CutRule {
    pub size_um: f64,
    pub min_spacing_um: f64,
    pub enclosure_um: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct PhysicalPlanningRules {
    pub placement_site_width_um: f64,
    pub row_height_um: f64,
    pub target_device_density: f64,
    pub target_routing_utilization: f64,
    pub floorplan_growth_factor: f64,
    pub max_floorplan_growth_passes: u16,
    pub global_route_max_iterations: u16,
    pub global_route_stall_iterations: u16,
    pub detailed_route_max_iterations: u16,
    #[serde(default)]
    pub routing_layers: BTreeMap<String, RoutingLayerResource>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct RoutingLayerResource {
    pub pitch_um: f64,
    pub offset_um: f64,
    pub preferred_direction: RoutingDirection,
    pub capacity_adjustment: f64,
    #[serde(default)]
    pub reserved_for_power: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingDirection {
    Horizontal,
    Vertical,
    Any,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TechnologyDiagnostic {
    pub code: &'static str,
    pub field: String,
    pub message: String,
}

impl Default for Technology {
    fn default() -> Self {
        Self {
            format_version: CURRENT_TECHNOLOGY_FORMAT_VERSION,
            name: "OpenChippy EDU CMOS".into(),
            supply_voltage: 1.8,
            max_metal_layers: DEFAULT_MAX_METAL_LAYERS,
            nmos: MosTechnology {
                threshold_voltage: 0.45,
                nominal_on_resistance_ohms: 12_000.0,
                reference_width_um: 1.0,
                reference_length_um: 1.0,
                gate_capacitance_ff_per_um: 2.0,
                diffusion_capacitance_ff_per_um: 1.0,
            },
            pmos: MosTechnology {
                threshold_voltage: -0.45,
                nominal_on_resistance_ohms: 22_000.0,
                reference_width_um: 1.0,
                reference_length_um: 1.0,
                gate_capacitance_ff_per_um: 2.2,
                diffusion_capacitance_ff_per_um: 1.2,
            },
            physical_rules: PhysicalRuleDeck::default(),
            physical_planning: PhysicalPlanningRules::educational(DEFAULT_MAX_METAL_LAYERS),
        }
    }
}

impl Default for PhysicalRuleDeck {
    fn default() -> Self {
        Self {
            format_version: 1,
            database_units_per_micron: 1_000,
            manufacturing_grid_um: 0.01,
            diffusion: LayerRule {
                min_width_um: 0.3,
                min_spacing_um: 0.3,
                min_area_um2: 0.09,
            },
            poly: LayerRule {
                min_width_um: 0.2,
                min_spacing_um: 0.2,
                min_area_um2: 0.04,
            },
            well: LayerRule {
                min_width_um: 0.6,
                min_spacing_um: 0.6,
                min_area_um2: 0.36,
            },
            metal: LayerRule {
                min_width_um: 0.2,
                min_spacing_um: 0.04,
                min_area_um2: 0.04,
            },
            contact: CutRule {
                size_um: 0.22,
                min_spacing_um: 0.08,
                enclosure_um: 0.03,
            },
            via: CutRule {
                size_um: 0.24,
                min_spacing_um: 0.08,
                enclosure_um: 0.02,
            },
            gate_extension_um: 0.2,
            well_enclosure_um: 0.3,
            layer_overrides: BTreeMap::new(),
            via_overrides: BTreeMap::new(),
        }
    }
}

impl Default for PhysicalPlanningRules {
    fn default() -> Self {
        Self {
            placement_site_width_um: 0.5,
            row_height_um: 1.7,
            target_device_density: 0.65,
            target_routing_utilization: 0.75,
            floorplan_growth_factor: 0.08,
            max_floorplan_growth_passes: 3,
            global_route_max_iterations: 30,
            global_route_stall_iterations: 3,
            detailed_route_max_iterations: 10,
            routing_layers: BTreeMap::new(),
        }
    }
}

impl PhysicalPlanningRules {
    pub fn educational(max_metal_layers: u16) -> Self {
        let mut planning = Self::default();
        for layer in 1..=max_metal_layers {
            planning.routing_layers.insert(
                format!("metal{layer}"),
                RoutingLayerResource {
                    pitch_um: 0.32,
                    offset_um: 0.16,
                    preferred_direction: if layer % 2 == 1 {
                        RoutingDirection::Horizontal
                    } else {
                        RoutingDirection::Vertical
                    },
                    capacity_adjustment: 0.75,
                    reserved_for_power: layer == 1,
                },
            );
        }
        planning
    }

    pub fn resolved_routing_layers(
        &self,
        max_metal_layers: u16,
    ) -> BTreeMap<String, RoutingLayerResource> {
        if self.routing_layers.is_empty() {
            Self::educational(max_metal_layers).routing_layers
        } else {
            self.routing_layers
                .iter()
                .filter(|(name, _)| {
                    name.strip_prefix("metal")
                        .and_then(|index| index.parse::<u16>().ok())
                        .is_some_and(|index| index <= max_metal_layers)
                })
                .map(|(name, resource)| (name.clone(), resource.clone()))
                .collect()
        }
    }
}

impl Technology {
    pub fn validate(&self) -> Result<(), Vec<TechnologyDiagnostic>> {
        let mut diagnostics = Vec::new();
        if self.format_version != CURRENT_TECHNOLOGY_FORMAT_VERSION {
            diagnostics.push(TechnologyDiagnostic {
                code: "unsupported_format_version",
                field: "format_version".into(),
                message: format!(
                    "Technology format {} is unsupported; expected {}.",
                    self.format_version, CURRENT_TECHNOLOGY_FORMAT_VERSION
                ),
            });
        }
        if self.name.trim().is_empty() {
            diagnostics.push(TechnologyDiagnostic {
                code: "missing_technology_name",
                field: "name".into(),
                message: "Technology name cannot be blank.".into(),
            });
        }
        if !self.supply_voltage.is_finite() || self.supply_voltage <= 0.0 {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_supply_voltage",
                field: "supply_voltage".into(),
                message: "Supply voltage must be a finite positive value.".into(),
            });
        }
        if self.max_metal_layers < 2 {
            diagnostics.push(TechnologyDiagnostic {
                code: "insufficient_metal_layers",
                field: "max_metal_layers".into(),
                message: "Technology must provide at least two metal routing layers.".into(),
            });
        }
        validate_mos(
            "nmos",
            &self.nmos,
            self.supply_voltage,
            ThresholdPolarity::Positive,
            &mut diagnostics,
        );
        validate_physical_rules(
            &self.physical_rules,
            self.max_metal_layers,
            &mut diagnostics,
        );
        validate_physical_planning(
            &self.physical_planning,
            self.max_metal_layers,
            self.physical_rules.manufacturing_grid_um,
            &mut diagnostics,
        );
        validate_mos(
            "pmos",
            &self.pmos,
            self.supply_voltage,
            ThresholdPolarity::Negative,
            &mut diagnostics,
        );
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(diagnostics)
        }
    }

    pub fn from_yaml(source: &str) -> Result<Self, String> {
        let file: TechnologyFile = serde_yaml::from_str(source)
            .map_err(|error| format!("technology YAML is invalid: {error}"))?;
        let technology = Technology {
            format_version: file.technology.format_version,
            name: file.technology.name,
            supply_voltage: file.technology.supply_voltage,
            max_metal_layers: file.technology.max_metal_layers,
            nmos: file.nmos,
            pmos: file.pmos,
            physical_rules: file.physical_rules,
            physical_planning: file.physical_planning,
        };
        technology.validate().map_err(|diagnostics| {
            diagnostics
                .iter()
                .map(|item| format!("{}: {}", item.field, item.message))
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        Ok(technology)
    }

    pub fn to_yaml(&self) -> Result<String, String> {
        self.validate().map_err(|diagnostics| {
            diagnostics
                .iter()
                .map(|item| format!("{}: {}", item.field, item.message))
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        serde_yaml::to_string(&TechnologyFile {
            technology: TechnologyHeader {
                format_version: self.format_version,
                name: self.name.clone(),
                supply_voltage: self.supply_voltage,
                max_metal_layers: self.max_metal_layers,
            },
            nmos: self.nmos.clone(),
            pmos: self.pmos.clone(),
            physical_rules: self.physical_rules.clone(),
            physical_planning: self.physical_planning.clone(),
        })
        .map_err(|error| format!("technology could not be serialized: {error}"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TechnologyFile {
    technology: TechnologyHeader,
    nmos: MosTechnology,
    pmos: MosTechnology,
    #[serde(default)]
    physical_rules: PhysicalRuleDeck,
    #[serde(default)]
    physical_planning: PhysicalPlanningRules,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TechnologyHeader {
    format_version: u32,
    name: String,
    supply_voltage: f64,
    max_metal_layers: u16,
}

const fn default_max_metal_layers() -> u16 {
    DEFAULT_MAX_METAL_LAYERS
}

enum ThresholdPolarity {
    Positive,
    Negative,
}

fn validate_mos(
    prefix: &str,
    device: &MosTechnology,
    supply_voltage: f64,
    polarity: ThresholdPolarity,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    let threshold_valid = device.threshold_voltage.is_finite()
        && match polarity {
            ThresholdPolarity::Positive => device.threshold_voltage > 0.0,
            ThresholdPolarity::Negative => device.threshold_voltage < 0.0,
        };
    if !threshold_valid {
        diagnostics.push(TechnologyDiagnostic {
            code: "invalid_threshold_voltage",
            field: format!("{prefix}.threshold_voltage"),
            message: format!(
                "{prefix} threshold voltage must be finite and {}.",
                match polarity {
                    ThresholdPolarity::Positive => "positive",
                    ThresholdPolarity::Negative => "negative",
                }
            ),
        });
    } else if supply_voltage.is_finite()
        && supply_voltage > 0.0
        && device.threshold_voltage.abs() >= supply_voltage
    {
        diagnostics.push(TechnologyDiagnostic {
            code: "threshold_exceeds_supply",
            field: format!("{prefix}.threshold_voltage"),
            message: format!("{prefix} threshold magnitude must be below the supply voltage."),
        });
    }
    for (field, value) in [
        (
            "nominal_on_resistance_ohms",
            device.nominal_on_resistance_ohms,
        ),
        ("reference_width_um", device.reference_width_um),
        ("reference_length_um", device.reference_length_um),
    ] {
        if !value.is_finite() || value <= 0.0 {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_positive_parameter",
                field: format!("{prefix}.{field}"),
                message: format!("{prefix} {field} must be a finite positive value."),
            });
        }
    }
    for (field, value) in [
        (
            "gate_capacitance_ff_per_um",
            device.gate_capacitance_ff_per_um,
        ),
        (
            "diffusion_capacitance_ff_per_um",
            device.diffusion_capacitance_ff_per_um,
        ),
    ] {
        if !value.is_finite() || value < 0.0 {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_capacitance_parameter",
                field: format!("{prefix}.{field}"),
                message: format!("{prefix} {field} must be finite and non-negative."),
            });
        }
    }
}

fn validate_physical_rules(
    rules: &PhysicalRuleDeck,
    max_metal_layers: u16,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    if rules.format_version != 1 {
        diagnostics.push(TechnologyDiagnostic {
            code: "unsupported_rule_deck_version",
            field: "physical_rules.format_version".into(),
            message: format!(
                "Physical rule-deck format {} is unsupported; expected 1.",
                rules.format_version
            ),
        });
    }
    if rules.database_units_per_micron == 0 {
        diagnostics.push(TechnologyDiagnostic {
            code: "invalid_database_units",
            field: "physical_rules.database_units_per_micron".into(),
            message: "Database units per micron must be greater than zero.".into(),
        });
    }
    validate_positive(
        "physical_rules.manufacturing_grid_um",
        rules.manufacturing_grid_um,
        diagnostics,
    );
    if rules.database_units_per_micron > 0
        && rules.manufacturing_grid_um.is_finite()
        && rules.manufacturing_grid_um > 0.0
    {
        let database_grid =
            rules.manufacturing_grid_um * f64::from(rules.database_units_per_micron);
        if (database_grid - database_grid.round()).abs() > 1e-6 {
            diagnostics.push(TechnologyDiagnostic {
                code: "off_database_grid",
                field: "physical_rules.manufacturing_grid_um".into(),
                message: "Manufacturing grid must resolve to an integer database unit.".into(),
            });
        }
    }
    for (name, rule) in [
        ("diffusion", &rules.diffusion),
        ("poly", &rules.poly),
        ("well", &rules.well),
        ("metal", &rules.metal),
    ] {
        validate_layer_rule(
            &format!("physical_rules.{name}"),
            rule,
            rules.manufacturing_grid_um,
            diagnostics,
        );
    }
    for (name, rule) in [("contact", &rules.contact), ("via", &rules.via)] {
        validate_cut_rule(
            &format!("physical_rules.{name}"),
            rule,
            rules.manufacturing_grid_um,
            diagnostics,
        );
    }
    for (field, value) in [
        ("gate_extension_um", rules.gate_extension_um),
        ("well_enclosure_um", rules.well_enclosure_um),
    ] {
        validate_positive(&format!("physical_rules.{field}"), value, diagnostics);
        validate_on_grid(
            &format!("physical_rules.{field}"),
            value,
            rules.manufacturing_grid_um,
            diagnostics,
        );
    }
    for (name, rule) in &rules.layer_overrides {
        let valid = name
            .strip_prefix("metal")
            .and_then(|index| index.parse::<u16>().ok())
            .is_some_and(|index| (1..=max_metal_layers).contains(&index));
        if !valid {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_layer_override",
                field: format!("physical_rules.layer_overrides.{name}"),
                message: format!(
                    "Layer override must name metal1 through metal{max_metal_layers}."
                ),
            });
        }
        validate_layer_rule(
            &format!("physical_rules.layer_overrides.{name}"),
            rule,
            rules.manufacturing_grid_um,
            diagnostics,
        );
    }
    for (name, rule) in &rules.via_overrides {
        let valid = (1..max_metal_layers)
            .map(|lower| format!("via{lower}{}", lower + 1))
            .any(|candidate| candidate == *name);
        if !valid {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_via_override",
                field: format!("physical_rules.via_overrides.{name}"),
                message: format!(
                    "Via override must identify adjacent metals within the {max_metal_layers}-layer process."
                ),
            });
        }
        validate_cut_rule(
            &format!("physical_rules.via_overrides.{name}"),
            rule,
            rules.manufacturing_grid_um,
            diagnostics,
        );
    }
}

fn validate_physical_planning(
    planning: &PhysicalPlanningRules,
    max_metal_layers: u16,
    grid: f64,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    for (field, value) in [
        ("placement_site_width_um", planning.placement_site_width_um),
        ("row_height_um", planning.row_height_um),
        ("floorplan_growth_factor", planning.floorplan_growth_factor),
    ] {
        let path = format!("physical_planning.{field}");
        validate_positive(&path, value, diagnostics);
        validate_on_grid(&path, value, grid, diagnostics);
    }
    for (field, value) in [
        ("target_device_density", planning.target_device_density),
        (
            "target_routing_utilization",
            planning.target_routing_utilization,
        ),
    ] {
        if !value.is_finite() || value <= 0.0 || value > 1.0 {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_planning_ratio",
                field: format!("physical_planning.{field}"),
                message: "Planning ratio must be greater than zero and at most one.".into(),
            });
        }
    }
    for (field, value) in [
        (
            "max_floorplan_growth_passes",
            planning.max_floorplan_growth_passes,
        ),
        (
            "global_route_max_iterations",
            planning.global_route_max_iterations,
        ),
        (
            "global_route_stall_iterations",
            planning.global_route_stall_iterations,
        ),
        (
            "detailed_route_max_iterations",
            planning.detailed_route_max_iterations,
        ),
    ] {
        if value == 0 {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_planning_effort",
                field: format!("physical_planning.{field}"),
                message: "Planning effort limit must be greater than zero.".into(),
            });
        }
    }
    if planning.global_route_stall_iterations > planning.global_route_max_iterations {
        diagnostics.push(TechnologyDiagnostic {
            code: "invalid_planning_effort",
            field: "physical_planning.global_route_stall_iterations".into(),
            message: "Global-route stall limit cannot exceed its iteration limit.".into(),
        });
    }
    for (name, layer) in &planning.routing_layers {
        let valid = name
            .strip_prefix("metal")
            .and_then(|index| index.parse::<u16>().ok())
            .is_some_and(|index| (1..=max_metal_layers).contains(&index));
        if !valid {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_routing_resource_layer",
                field: format!("physical_planning.routing_layers.{name}"),
                message: format!(
                    "Routing resource must name metal1 through metal{max_metal_layers}."
                ),
            });
        }
        for (field, value) in [("pitch_um", layer.pitch_um), ("offset_um", layer.offset_um)] {
            let path = format!("physical_planning.routing_layers.{name}.{field}");
            validate_positive(&path, value, diagnostics);
            validate_on_grid(&path, value, grid, diagnostics);
        }
        if !layer.capacity_adjustment.is_finite()
            || layer.capacity_adjustment <= 0.0
            || layer.capacity_adjustment > 1.0
        {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_routing_capacity",
                field: format!("physical_planning.routing_layers.{name}.capacity_adjustment"),
                message: "Routing capacity adjustment must be greater than zero and at most one."
                    .into(),
            });
        }
    }
}

fn validate_layer_rule(
    prefix: &str,
    rule: &LayerRule,
    grid: f64,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    for (field, value) in [
        ("min_width_um", rule.min_width_um),
        ("min_spacing_um", rule.min_spacing_um),
        ("min_area_um2", rule.min_area_um2),
    ] {
        let path = format!("{prefix}.{field}");
        validate_positive(&path, value, diagnostics);
        if field != "min_area_um2" {
            validate_on_grid(&path, value, grid, diagnostics);
        }
    }
}

fn validate_cut_rule(
    prefix: &str,
    rule: &CutRule,
    grid: f64,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    for (field, value) in [
        ("size_um", rule.size_um),
        ("min_spacing_um", rule.min_spacing_um),
        ("enclosure_um", rule.enclosure_um),
    ] {
        let path = format!("{prefix}.{field}");
        validate_positive(&path, value, diagnostics);
        validate_on_grid(&path, value, grid, diagnostics);
    }
}

fn validate_positive(field: &str, value: f64, diagnostics: &mut Vec<TechnologyDiagnostic>) {
    if !value.is_finite() || value <= 0.0 {
        diagnostics.push(TechnologyDiagnostic {
            code: "invalid_physical_rule",
            field: field.into(),
            message: "Physical rule must be a finite positive value.".into(),
        });
    }
}

fn validate_on_grid(
    field: &str,
    value: f64,
    grid: f64,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    if value.is_finite() && value > 0.0 && grid.is_finite() && grid > 0.0 {
        let grid_units = value / grid;
        if (grid_units - grid_units.round()).abs() > 1e-6 {
            diagnostics.push(TechnologyDiagnostic {
                code: "off_manufacturing_grid",
                field: field.into(),
                message: format!("Physical rule must align to the {grid} µm manufacturing grid."),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CutRule, LayerRule, Technology, CURRENT_TECHNOLOGY_FORMAT_VERSION, DEFAULT_MAX_METAL_LAYERS,
    };

    #[test]
    fn educational_default_is_valid_and_serializable() {
        let technology = Technology::default();
        assert!(technology.validate().is_ok());
        assert_eq!(technology.format_version, CURRENT_TECHNOLOGY_FORMAT_VERSION);
        assert_eq!(technology.max_metal_layers, DEFAULT_MAX_METAL_LAYERS);
        assert_eq!(technology.physical_rules.format_version, 1);
        assert_eq!(technology.physical_rules.database_units_per_micron, 1_000);
        assert!(technology.nmos.threshold_voltage < technology.supply_voltage);
        assert!(technology.pmos.threshold_voltage.abs() < technology.supply_voltage);

        let serialized = serde_json::to_string(&technology).unwrap();
        let reloaded: Technology = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reloaded, technology);
    }

    #[test]
    fn validation_reports_fields_for_non_physical_parameters() {
        let mut technology = Technology::default();
        technology.name = " ".into();
        technology.supply_voltage = 0.0;
        technology.max_metal_layers = 1;
        technology.nmos.threshold_voltage = -0.45;
        technology.nmos.nominal_on_resistance_ohms = f64::INFINITY;
        technology.pmos.threshold_voltage = 0.45;
        technology.pmos.reference_width_um = 0.0;
        technology.pmos.gate_capacitance_ff_per_um = -1.0;

        let diagnostics = technology.validate().unwrap_err();
        for field in [
            "name",
            "supply_voltage",
            "max_metal_layers",
            "nmos.threshold_voltage",
            "nmos.nominal_on_resistance_ohms",
            "pmos.threshold_voltage",
            "pmos.reference_width_um",
            "pmos.gate_capacitance_ff_per_um",
        ] {
            assert!(
                diagnostics.iter().any(|item| item.field == field),
                "missing diagnostic for {field}"
            );
        }
    }

    #[test]
    fn threshold_magnitude_must_remain_below_supply() {
        let mut technology = Technology::default();
        technology.nmos.threshold_voltage = technology.supply_voltage;
        technology.pmos.threshold_voltage = -technology.supply_voltage;
        let diagnostics = technology.validate().unwrap_err();
        assert_eq!(
            diagnostics
                .iter()
                .filter(|item| item.code == "threshold_exceeds_supply")
                .count(),
            2
        );
    }

    #[test]
    fn yaml_aliases_load_and_round_trip() {
        let source = r#"
technology:
  format_version: 1
  name: Test EDU CMOS
  supply_voltage: 1.2
  max_metal_layers: 7
nmos:
  vt: 0.35
  ron: 10000
  reference_width: 1.0
  reference_length: 0.5
  gate_capacitance_ff_per_um: 1.8
  diffusion_capacitance_ff_per_um: 0.9
pmos:
  vt: -0.35
  ron: 18000
  reference_width: 1.5
  reference_length: 0.5
  gate_capacitance_ff_per_um: 2.0
  diffusion_capacitance_ff_per_um: 1.1
"#;
        let mut technology = Technology::from_yaml(source).unwrap();
        assert_eq!(technology.name, "Test EDU CMOS");
        assert_eq!(technology.max_metal_layers, 7);
        assert_eq!(technology.nmos.nominal_on_resistance_ohms, 10_000.0);
        technology.physical_rules.layer_overrides.insert(
            "metal7".into(),
            LayerRule {
                min_width_um: 0.3,
                min_spacing_um: 0.3,
                min_area_um2: 0.09,
            },
        );
        technology.physical_rules.via_overrides.insert(
            "via67".into(),
            CutRule {
                size_um: 0.3,
                min_spacing_um: 0.2,
                enclosure_um: 0.02,
            },
        );
        assert_eq!(
            Technology::from_yaml(&technology.to_yaml().unwrap()).unwrap(),
            technology
        );
    }

    #[test]
    fn yaml_rejects_missing_unknown_and_non_physical_values() {
        let missing = "technology: { format_version: 1, name: Missing, supply_voltage: 1.8 }";
        assert!(Technology::from_yaml(missing)
            .unwrap_err()
            .contains("missing field"));

        let unknown = Technology::default().to_yaml().unwrap().replace(
            "supply_voltage: 1.8",
            "supply_voltage: 1.8\n  surprise: true",
        );
        assert!(Technology::from_yaml(&unknown)
            .unwrap_err()
            .contains("unknown field"));

        let unknown_device = Technology::default().to_yaml().unwrap().replace(
            "threshold_voltage: 0.45",
            "threshold_voltage: 0.45\n  surprise: true",
        );
        assert!(Technology::from_yaml(&unknown_device)
            .unwrap_err()
            .contains("unknown field"));

        let invalid = Technology::default()
            .to_yaml()
            .unwrap()
            .replace("supply_voltage: 1.8", "supply_voltage: 0.0");
        assert!(Technology::from_yaml(&invalid)
            .unwrap_err()
            .contains("supply_voltage"));

        let too_few_layers = Technology::default()
            .to_yaml()
            .unwrap()
            .replace("max_metal_layers: 5", "max_metal_layers: 1");
        assert!(Technology::from_yaml(&too_few_layers)
            .unwrap_err()
            .contains("max_metal_layers"));
    }

    #[test]
    fn older_embedded_technology_defaults_to_five_metals() {
        let serialized = serde_json::to_string(&Technology::default())
            .unwrap()
            .replace(",\"max_metal_layers\":5", "");
        let technology: Technology = serde_json::from_str(&serialized).unwrap();
        assert_eq!(technology.max_metal_layers, DEFAULT_MAX_METAL_LAYERS);
    }

    #[test]
    fn older_embedded_technology_receives_the_educational_rule_deck() {
        let mut serialized = serde_json::to_value(Technology::default()).unwrap();
        serialized.as_object_mut().unwrap().remove("physical_rules");
        let technology: Technology = serde_json::from_value(serialized).unwrap();
        assert_eq!(
            technology.physical_rules,
            super::PhysicalRuleDeck::default()
        );
        assert!(technology.validate().is_ok());
    }

    #[test]
    fn physical_rule_deck_validates_grid_values_and_process_overrides() {
        let mut technology = Technology::default();
        technology.physical_rules.metal.min_width_um = 0.205;
        technology.physical_rules.layer_overrides.insert(
            "metal6".into(),
            LayerRule {
                min_width_um: 0.2,
                min_spacing_um: 0.2,
                min_area_um2: 0.04,
            },
        );
        technology.physical_rules.via_overrides.insert(
            "via24".into(),
            CutRule {
                size_um: 0.2,
                min_spacing_um: 0.2,
                enclosure_um: 0.02,
            },
        );
        let diagnostics = technology.validate().unwrap_err();
        assert!(diagnostics.iter().any(|item| {
            item.code == "off_manufacturing_grid"
                && item.field == "physical_rules.metal.min_width_um"
        }));
        assert!(diagnostics.iter().any(|item| {
            item.code == "invalid_layer_override"
                && item.field == "physical_rules.layer_overrides.metal6"
        }));
        assert!(diagnostics.iter().any(|item| {
            item.code == "invalid_via_override"
                && item.field == "physical_rules.via_overrides.via24"
        }));
    }

    #[test]
    fn physical_planning_defaults_resolve_resources_and_validate_effort() {
        let mut serialized = serde_json::to_value(Technology::default()).unwrap();
        serialized
            .as_object_mut()
            .unwrap()
            .remove("physical_planning");
        let technology: Technology = serde_json::from_value(serialized).unwrap();
        assert_eq!(
            technology
                .physical_planning
                .resolved_routing_layers(technology.max_metal_layers)
                .len(),
            usize::from(technology.max_metal_layers)
        );
        assert!(technology.validate().is_ok());

        let mut invalid = Technology::default();
        invalid.physical_planning.target_device_density = 1.2;
        invalid
            .physical_planning
            .routing_layers
            .get_mut("metal2")
            .unwrap()
            .capacity_adjustment = 0.0;
        invalid.physical_planning.global_route_stall_iterations = 31;
        let diagnostics = invalid.validate().unwrap_err();
        assert!(diagnostics
            .iter()
            .any(|item| item.code == "invalid_planning_ratio"));
        assert!(diagnostics
            .iter()
            .any(|item| item.code == "invalid_routing_capacity"));
        assert!(diagnostics
            .iter()
            .any(|item| item.field == "physical_planning.global_route_stall_iterations"));
    }
}
