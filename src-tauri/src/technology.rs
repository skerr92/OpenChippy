use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CURRENT_TECHNOLOGY_FORMAT_VERSION: u32 = 1;
pub const DEFAULT_MAX_METAL_LAYERS: u16 = 5;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Technology {
    pub format_version: u32,
    pub name: String,
    #[serde(default = "default_process_id")]
    pub process_id: String,
    #[serde(default = "default_deck_revision")]
    pub deck_revision: String,
    #[serde(default = "default_deck_source")]
    pub source: String,
    pub supply_voltage: f64,
    #[serde(default = "default_max_metal_layers")]
    pub max_metal_layers: u16,
    pub nmos: MosTechnology,
    pub pmos: MosTechnology,
    #[serde(default)]
    pub physical_parasitics: PhysicalParasiticRules,
    #[serde(default)]
    pub tapeout_window: TapeoutWindow,
    #[serde(default)]
    pub physical_rules: PhysicalRuleDeck,
    #[serde(default)]
    pub physical_planning: PhysicalPlanningRules,
    #[serde(default)]
    pub gds_layers: GdsLayerMap,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GdsLayerMap {
    pub format_version: u32,
    pub label_datatype: u16,
    pub layers: BTreeMap<String, Vec<GdsLayerPurpose>>,
    /// Process-purpose mappings used only for non-electrical density fill.
    /// Keeping these separate prevents dummy COMP/poly/metal from being
    /// interpreted as devices or routed conductors on import.
    #[serde(default)]
    pub dummy_layers: BTreeMap<String, Vec<GdsLayerPurpose>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GdsLayerPurpose {
    pub purpose: String,
    pub layer: u16,
    pub datatype: u16,
    /// Symmetric process-purpose expansion applied only while streaming GDS.
    /// This lets one Physical IR shape emit distinct drawn COMP and implant
    /// envelopes without conflating their manufacturing geometry.
    #[serde(default)]
    pub enclosure_um: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct TapeoutWindow {
    pub format_version: u32,
    pub name: String,
    pub width_um: f64,
    pub height_um: f64,
    pub edge_margin_um: f64,
}

impl Default for TapeoutWindow {
    fn default() -> Self {
        Self {
            format_version: 1,
            name: "Caravel SKY130 user area".into(),
            width_um: 2_920.0,
            height_um: 3_520.0,
            edge_margin_um: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct PhysicalParasiticRules {
    pub format_version: u32,
    pub wire_capacitance_ff_per_um: f64,
    pub via_capacitance_ff: f64,
    #[serde(default)]
    pub layer_capacitance_ff_per_um: BTreeMap<String, f64>,
    #[serde(default)]
    pub via_capacitance_overrides_ff: BTreeMap<String, f64>,
}

impl Default for PhysicalParasiticRules {
    fn default() -> Self {
        Self {
            format_version: 1,
            wire_capacitance_ff_per_um: 0.16,
            via_capacitance_ff: 0.05,
            layer_capacitance_ff_per_um: BTreeMap::new(),
            via_capacitance_overrides_ff: BTreeMap::new(),
        }
    }
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
    pub max_tap_distance_um: Option<f64>,
    #[serde(default)]
    pub layer_overrides: BTreeMap<String, LayerRule>,
    #[serde(default)]
    pub via_overrides: BTreeMap<String, CutRule>,
    #[serde(default)]
    pub density_fill: DensityFillRuleDeck,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DensityFillRuleDeck {
    pub format_version: u32,
    #[serde(default)]
    pub layers: BTreeMap<String, DensityFillLayerRule>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct DensityFillLayerRule {
    pub target_density: f64,
    #[serde(default)]
    pub maximum_density: Option<f64>,
    pub tile_width_um: f64,
    pub tile_height_um: f64,
    /// Optional smaller square used only after the primary lattice cannot
    /// reach target density around existing circuit geometry.
    #[serde(default)]
    pub fallback_tile_sizes_um: Vec<f64>,
    pub fill_spacing_um: f64,
    pub circuit_spacing_um: f64,
    /// A dummy layer may require another dummy material under it. GF180 dummy
    /// poly, for example, must be generated over dummy COMP.
    #[serde(default)]
    pub support_layer: Option<String>,
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
            process_id: "openchippy-edu-cmos".into(),
            deck_revision: "builtin-v1".into(),
            source: "OpenChippy built-in educational technology".into(),
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
            physical_parasitics: PhysicalParasiticRules::default(),
            tapeout_window: TapeoutWindow::default(),
            physical_rules: PhysicalRuleDeck::default(),
            physical_planning: PhysicalPlanningRules::educational(DEFAULT_MAX_METAL_LAYERS),
            gds_layers: GdsLayerMap::educational(DEFAULT_MAX_METAL_LAYERS),
        }
    }
}

impl Default for GdsLayerMap {
    fn default() -> Self {
        Self::educational(DEFAULT_MAX_METAL_LAYERS)
    }
}

impl GdsLayerMap {
    pub fn educational(max_metal_layers: u16) -> Self {
        let mut layers = BTreeMap::new();
        for (name, layer, datatype) in [
            ("substrate", 1, 0),
            ("pwell", 20, 0),
            ("nwell", 21, 0),
            ("ndiff", 22, 0),
            ("pdiff", 22, 1),
            ("poly", 30, 0),
            ("contact", 33, 0),
        ] {
            layers.insert(
                name.into(),
                vec![GdsLayerPurpose {
                    purpose: name.into(),
                    layer,
                    datatype,
                    enclosure_um: 0.0,
                }],
            );
        }
        for index in 1..=max_metal_layers {
            layers.insert(
                format!("metal{index}"),
                vec![GdsLayerPurpose {
                    purpose: format!("metal{index}"),
                    layer: 34 + (index - 1) * 2,
                    datatype: 0,
                    enclosure_um: 0.0,
                }],
            );
            if index < max_metal_layers {
                layers.insert(
                    format!("via{index}{}", index + 1),
                    vec![GdsLayerPurpose {
                        purpose: format!("via{index}{}", index + 1),
                        layer: 35 + (index - 1) * 2,
                        datatype: 0,
                        enclosure_um: 0.0,
                    }],
                );
            }
        }
        Self {
            format_version: 1,
            label_datatype: 10,
            layers,
            dummy_layers: BTreeMap::new(),
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
            max_tap_distance_um: None,
            layer_overrides: BTreeMap::new(),
            via_overrides: BTreeMap::new(),
            density_fill: DensityFillRuleDeck::default(),
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
    /// Fill mappings that became mandatory after older project snapshots were
    /// written. Only known process identities are migrated; custom decks must
    /// remain explicit about manufacturing layers.
    pub fn migrate_legacy_gds_layers(&mut self) -> bool {
        let normalized_name = self
            .name
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>();
        let legacy_named_gf180 = self.process_id == "legacy-unidentified"
            && normalized_name.starts_with("gf180mcu")
            && normalized_name.contains("5m");
        if self.process_id != "gf180mcu-3v3-5m-compat" && !legacy_named_gf180 {
            return false;
        }

        let mut changed = false;
        // Projects persist the selected technology snapshot. Older GF180
        // snapshots predate density rules and dummy-purpose mappings; keeping
        // those empty silently disables manufacturing fill after reopening a
        // perfectly valid design. Hydrate only missing contracts from the
        // packaged deck while preserving any explicit user overrides.
        if self.physical_rules.density_fill.layers.is_empty()
            || self.gds_layers.dummy_layers.is_empty()
        {
            let canonical = Technology::from_yaml(include_str!(
                "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
            ))
            .expect("packaged GF180 technology must remain valid");
            if self.physical_rules.density_fill.layers.is_empty() {
                self.physical_rules.density_fill = canonical.physical_rules.density_fill;
                changed = true;
            }
            if self.gds_layers.dummy_layers.is_empty() {
                self.gds_layers.dummy_layers = canonical.gds_layers.dummy_layers;
                changed = true;
            }
        }
        if !self.gds_layers.layers.contains_key("pwell") {
            self.gds_layers.layers.insert(
                "pwell".into(),
                vec![GdsLayerPurpose {
                    purpose: "lvpwell".into(),
                    layer: 204,
                    datatype: 0,
                    enclosure_um: 0.0,
                }],
            );
            changed = true;
        }
        for (logical_layer, process_purpose) in [("ndiff", "nplus"), ("pdiff", "pplus")] {
            if let Some(purposes) = self.gds_layers.layers.get_mut(logical_layer) {
                for purpose in purposes
                    .iter_mut()
                    .filter(|purpose| purpose.purpose == process_purpose)
                {
                    if purpose.enclosure_um != 0.35 {
                        purpose.enclosure_um = 0.35;
                        changed = true;
                    }
                }
            }
        }
        if self.physical_rules.contact.size_um != 0.22 {
            self.physical_rules.contact.size_um = 0.22;
            changed = true;
        }
        if self.physical_rules.max_tap_distance_um != Some(20.0) {
            self.physical_rules.max_tap_distance_um = Some(20.0);
            changed = true;
        }
        let default_metal = self.physical_rules.metal.clone();
        let metal5 = self
            .physical_rules
            .layer_overrides
            .entry("metal5".into())
            .or_insert(default_metal);
        if metal5.min_width_um != 0.44
            || metal5.min_spacing_um != 0.46
            || metal5.min_area_um2 != 0.5625
        {
            metal5.min_width_um = 0.44;
            metal5.min_spacing_um = 0.46;
            metal5.min_area_um2 = 0.5625;
            changed = true;
        }
        changed
    }

    pub fn fingerprint(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("technology could not be fingerprinted: {error}"))?;
        let mut hash = 0xcbf29ce484222325u64;
        for byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        Ok(format!("fnv1a64:{hash:016x}"))
    }

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
        for (field, value) in [
            ("process_id", self.process_id.as_str()),
            ("deck_revision", self.deck_revision.as_str()),
            ("source", self.source.as_str()),
        ] {
            if value.trim().is_empty() {
                diagnostics.push(TechnologyDiagnostic {
                    code: "missing_process_metadata",
                    field: field.into(),
                    message: format!("{field} cannot be blank."),
                });
            }
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
        validate_physical_parasitics(
            &self.physical_parasitics,
            self.max_metal_layers,
            &mut diagnostics,
        );
        validate_tapeout_window(&self.tapeout_window, &mut diagnostics);
        validate_physical_planning(
            &self.physical_planning,
            self.max_metal_layers,
            self.physical_rules.manufacturing_grid_um,
            &mut diagnostics,
        );
        validate_gds_layers(&self.gds_layers, self.max_metal_layers, &mut diagnostics);
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
            process_id: file.technology.process_id,
            deck_revision: file.technology.deck_revision,
            source: file.technology.source,
            supply_voltage: file.technology.supply_voltage,
            max_metal_layers: file.technology.max_metal_layers,
            nmos: file.nmos,
            pmos: file.pmos,
            physical_parasitics: file.physical_parasitics,
            tapeout_window: file.tapeout_window,
            physical_rules: file.physical_rules,
            physical_planning: file.physical_planning,
            gds_layers: file
                .gds_layers
                .unwrap_or_else(|| GdsLayerMap::educational(file.technology.max_metal_layers)),
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
                process_id: self.process_id.clone(),
                deck_revision: self.deck_revision.clone(),
                source: self.source.clone(),
                supply_voltage: self.supply_voltage,
                max_metal_layers: self.max_metal_layers,
            },
            nmos: self.nmos.clone(),
            pmos: self.pmos.clone(),
            physical_parasitics: self.physical_parasitics.clone(),
            tapeout_window: self.tapeout_window.clone(),
            physical_rules: self.physical_rules.clone(),
            physical_planning: self.physical_planning.clone(),
            gds_layers: Some(self.gds_layers.clone()),
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
    physical_parasitics: PhysicalParasiticRules,
    #[serde(default)]
    tapeout_window: TapeoutWindow,
    #[serde(default)]
    physical_rules: PhysicalRuleDeck,
    #[serde(default)]
    physical_planning: PhysicalPlanningRules,
    #[serde(default)]
    gds_layers: Option<GdsLayerMap>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TechnologyHeader {
    format_version: u32,
    name: String,
    #[serde(default = "default_process_id")]
    process_id: String,
    #[serde(default = "default_deck_revision")]
    deck_revision: String,
    #[serde(default = "default_deck_source")]
    source: String,
    supply_voltage: f64,
    max_metal_layers: u16,
}

fn default_process_id() -> String {
    "legacy-unidentified".into()
}

fn default_deck_revision() -> String {
    "legacy".into()
}

fn default_deck_source() -> String {
    "embedded project snapshot".into()
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
    if let Some(distance) = rules.max_tap_distance_um {
        validate_positive("physical_rules.max_tap_distance_um", distance, diagnostics);
        validate_on_grid(
            "physical_rules.max_tap_distance_um",
            distance,
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
    validate_density_fill(
        &rules.density_fill,
        max_metal_layers,
        rules.manufacturing_grid_um,
        diagnostics,
    );
}

fn validate_density_fill(
    fill: &DensityFillRuleDeck,
    max_metal_layers: u16,
    grid: f64,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    if fill.layers.is_empty() {
        return;
    }
    if fill.format_version != 1 {
        diagnostics.push(TechnologyDiagnostic {
            code: "unsupported_density_fill_version",
            field: "physical_rules.density_fill.format_version".into(),
            message: format!(
                "Density-fill format {} is unsupported; expected 1.",
                fill.format_version
            ),
        });
    }
    let valid_name = |name: &str| {
        matches!(name, "active" | "poly" | "top_metal")
            || name
                .strip_prefix("metal")
                .and_then(|index| index.parse::<u16>().ok())
                .is_some_and(|index| (1..=max_metal_layers).contains(&index))
    };
    for (name, rule) in &fill.layers {
        let prefix = format!("physical_rules.density_fill.layers.{name}");
        if !valid_name(name) {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_density_fill_layer",
                field: prefix.clone(),
                message: "Density fill must name active, poly, metal1..metalN, or top_metal."
                    .into(),
            });
        }
        if !rule.target_density.is_finite()
            || rule.target_density <= 0.0
            || rule.target_density >= 1.0
        {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_density_target",
                field: format!("{prefix}.target_density"),
                message: "Density target must be finite and strictly between zero and one.".into(),
            });
        }
        if let Some(maximum) = rule.maximum_density {
            if !maximum.is_finite() || maximum <= rule.target_density || maximum > 1.0 {
                diagnostics.push(TechnologyDiagnostic {
                    code: "invalid_density_maximum",
                    field: format!("{prefix}.maximum_density"),
                    message: "Maximum density must exceed the target and be at most one.".into(),
                });
            }
        }
        for (field, value) in [
            ("tile_width_um", rule.tile_width_um),
            ("tile_height_um", rule.tile_height_um),
            ("fill_spacing_um", rule.fill_spacing_um),
            ("circuit_spacing_um", rule.circuit_spacing_um),
        ] {
            validate_positive(&format!("{prefix}.{field}"), value, diagnostics);
            validate_on_grid(&format!("{prefix}.{field}"), value, grid, diagnostics);
        }
        validate_centered_extent_on_grid(
            &format!("{prefix}.tile_width_um"),
            rule.tile_width_um,
            grid,
            diagnostics,
        );
        validate_centered_extent_on_grid(
            &format!("{prefix}.tile_height_um"),
            rule.tile_height_um,
            grid,
            diagnostics,
        );
        let mut previous = rule.tile_width_um.min(rule.tile_height_um);
        for (index, value) in rule.fallback_tile_sizes_um.iter().copied().enumerate() {
            validate_positive(
                &format!("{prefix}.fallback_tile_sizes_um.{index}"),
                value,
                diagnostics,
            );
            validate_on_grid(
                &format!("{prefix}.fallback_tile_sizes_um.{index}"),
                value,
                grid,
                diagnostics,
            );
            validate_centered_extent_on_grid(
                &format!("{prefix}.fallback_tile_sizes_um.{index}"),
                value,
                grid,
                diagnostics,
            );
            if value >= previous {
                diagnostics.push(TechnologyDiagnostic {
                    code: "invalid_density_fallback_tile",
                    field: format!("{prefix}.fallback_tile_sizes_um.{index}"),
                    message:
                        "Fallback fill tiles must be strictly decreasing from the primary tile."
                            .into(),
                });
            }
            previous = value;
        }
        if let Some(support) = &rule.support_layer {
            if support == name || !valid_name(support) {
                diagnostics.push(TechnologyDiagnostic {
                    code: "invalid_density_fill_support",
                    field: format!("{prefix}.support_layer"),
                    message: "Density-fill support must name a different configured fill layer."
                        .into(),
                });
            } else if !fill.layers.contains_key(support) {
                diagnostics.push(TechnologyDiagnostic {
                    code: "missing_density_fill_support",
                    field: format!("{prefix}.support_layer"),
                    message: format!("Density-fill support layer {support} is not configured."),
                });
            }
        }
    }
}

fn validate_physical_parasitics(
    parasitics: &PhysicalParasiticRules,
    max_metal_layers: u16,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    if parasitics.format_version != 1 {
        diagnostics.push(TechnologyDiagnostic {
            code: "unsupported_parasitic_version",
            field: "physical_parasitics.format_version".into(),
            message: format!(
                "Physical parasitic format {} is unsupported; expected 1.",
                parasitics.format_version
            ),
        });
    }
    for (field, value) in [
        (
            "wire_capacitance_ff_per_um",
            parasitics.wire_capacitance_ff_per_um,
        ),
        ("via_capacitance_ff", parasitics.via_capacitance_ff),
    ] {
        validate_non_negative(&format!("physical_parasitics.{field}"), value, diagnostics);
    }
    for (name, value) in &parasitics.layer_capacitance_ff_per_um {
        let valid = name
            .strip_prefix("metal")
            .and_then(|index| index.parse::<u16>().ok())
            .is_some_and(|index| (1..=max_metal_layers).contains(&index));
        if !valid {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_parasitic_layer",
                field: format!("physical_parasitics.layer_capacitance_ff_per_um.{name}"),
                message: format!(
                    "Parasitic layer must name metal1 through metal{max_metal_layers}."
                ),
            });
        }
        validate_non_negative(
            &format!("physical_parasitics.layer_capacitance_ff_per_um.{name}"),
            *value,
            diagnostics,
        );
    }
    for (name, value) in &parasitics.via_capacitance_overrides_ff {
        let valid = (1..max_metal_layers)
            .map(|lower| format!("via{lower}{}", lower + 1))
            .any(|candidate| candidate == *name);
        if !valid {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_parasitic_via",
                field: format!("physical_parasitics.via_capacitance_overrides_ff.{name}"),
                message: format!(
                    "Parasitic via must identify adjacent metals within the {max_metal_layers}-layer process."
                ),
            });
        }
        validate_non_negative(
            &format!("physical_parasitics.via_capacitance_overrides_ff.{name}"),
            *value,
            diagnostics,
        );
    }
}

fn validate_gds_layers(
    mapping: &GdsLayerMap,
    max_metal_layers: u16,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    if mapping.format_version != 1 {
        diagnostics.push(TechnologyDiagnostic {
            code: "unsupported_gds_layer_map_version",
            field: "gds_layers.format_version".into(),
            message: format!(
                "GDS layer-map format {} is unsupported; expected 1.",
                mapping.format_version
            ),
        });
    }
    let mut required = vec![
        "substrate".to_string(),
        "pwell".into(),
        "nwell".into(),
        "ndiff".into(),
        "pdiff".into(),
        "poly".into(),
        "contact".into(),
    ];
    required.extend((1..=max_metal_layers).map(|index| format!("metal{index}")));
    required.extend((1..max_metal_layers).map(|index| format!("via{index}{}", index + 1)));
    for name in required {
        let Some(purposes) = mapping.layers.get(&name) else {
            diagnostics.push(TechnologyDiagnostic {
                code: "missing_gds_layer_mapping",
                field: format!("gds_layers.layers.{name}"),
                message: "Every generated Physical IR layer requires an explicit GDS mapping."
                    .into(),
            });
            continue;
        };
        if name != "substrate" && purposes.is_empty() {
            diagnostics.push(TechnologyDiagnostic {
                code: "empty_gds_layer_mapping",
                field: format!("gds_layers.layers.{name}"),
                message: "Manufacturing layers must emit at least one GDS layer purpose.".into(),
            });
        }
    }
    for (name, purposes) in &mapping.layers {
        let valid_name = matches!(
            name.as_str(),
            "substrate" | "pwell" | "nwell" | "ndiff" | "pdiff" | "poly" | "contact"
        ) || name
            .strip_prefix("metal")
            .and_then(|index| index.parse::<u16>().ok())
            .is_some_and(|index| (1..=max_metal_layers).contains(&index))
            || (1..max_metal_layers)
                .map(|lower| format!("via{lower}{}", lower + 1))
                .any(|candidate| candidate == *name);
        if !valid_name {
            diagnostics.push(TechnologyDiagnostic {
                code: "unknown_gds_source_layer",
                field: format!("gds_layers.layers.{name}"),
                message: "GDS mapping names must correspond to generated Physical IR layers."
                    .into(),
            });
        }
        let mut pairs = std::collections::BTreeSet::new();
        for (index, purpose) in purposes.iter().enumerate() {
            let field = format!("gds_layers.layers.{name}.{index}");
            if purpose.purpose.trim().is_empty() {
                diagnostics.push(TechnologyDiagnostic {
                    code: "missing_gds_layer_purpose",
                    field: format!("{field}.purpose"),
                    message: "GDS layer purpose cannot be blank.".into(),
                });
            }
            if purpose.layer > i16::MAX as u16 || purpose.datatype > i16::MAX as u16 {
                diagnostics.push(TechnologyDiagnostic {
                    code: "gds_layer_value_out_of_range",
                    field: field.clone(),
                    message: "GDS layer and datatype must fit the signed 16-bit stream field."
                        .into(),
                });
            }
            if !purpose.enclosure_um.is_finite() || purpose.enclosure_um < 0.0 {
                diagnostics.push(TechnologyDiagnostic {
                    code: "invalid_gds_layer_enclosure",
                    field: format!("{field}.enclosure_um"),
                    message: "GDS purpose enclosure must be a finite non-negative value.".into(),
                });
            }
            if !pairs.insert((purpose.layer, purpose.datatype)) {
                diagnostics.push(TechnologyDiagnostic {
                    code: "duplicate_gds_layer_purpose",
                    field,
                    message: "A Physical IR layer cannot emit the same GDS layer/datatype twice."
                        .into(),
                });
            }
        }
    }
    for (name, purposes) in &mapping.dummy_layers {
        let configured = name == "top_metal"
            || matches!(name.as_str(), "active" | "poly")
            || name
                .strip_prefix("metal")
                .and_then(|index| index.parse::<u16>().ok())
                .is_some_and(|index| (1..=max_metal_layers).contains(&index));
        if !configured {
            diagnostics.push(TechnologyDiagnostic {
                code: "unknown_gds_dummy_layer",
                field: format!("gds_layers.dummy_layers.{name}"),
                message: "Dummy mapping must name active, poly, metal1..metalN, or top_metal."
                    .into(),
            });
        }
        if purposes.len() != 1 {
            diagnostics.push(TechnologyDiagnostic {
                code: "invalid_gds_dummy_mapping",
                field: format!("gds_layers.dummy_layers.{name}"),
                message: "Each dummy material must emit exactly one GDS layer purpose.".into(),
            });
        }
        for (index, purpose) in purposes.iter().enumerate() {
            let field = format!("gds_layers.dummy_layers.{name}.{index}");
            if purpose.purpose.trim().is_empty() {
                diagnostics.push(TechnologyDiagnostic {
                    code: "missing_gds_layer_purpose",
                    field: format!("{field}.purpose"),
                    message: "GDS layer purpose cannot be blank.".into(),
                });
            }
            if purpose.layer > i16::MAX as u16 || purpose.datatype > i16::MAX as u16 {
                diagnostics.push(TechnologyDiagnostic {
                    code: "gds_layer_value_out_of_range",
                    field,
                    message: "GDS layer and datatype must fit the signed 16-bit stream field."
                        .into(),
                });
            }
        }
    }
}

fn validate_tapeout_window(window: &TapeoutWindow, diagnostics: &mut Vec<TechnologyDiagnostic>) {
    if window.format_version != 1 {
        diagnostics.push(TechnologyDiagnostic {
            code: "unsupported_tapeout_window_version",
            field: "tapeout_window.format_version".into(),
            message: format!(
                "Tapeout-window format {} is unsupported; expected 1.",
                window.format_version
            ),
        });
    }
    if window.name.trim().is_empty() {
        diagnostics.push(TechnologyDiagnostic {
            code: "missing_tapeout_window_name",
            field: "tapeout_window.name".into(),
            message: "Tapeout window name cannot be blank.".into(),
        });
    }
    for (field, value) in [
        ("width_um", window.width_um),
        ("height_um", window.height_um),
    ] {
        validate_positive(&format!("tapeout_window.{field}"), value, diagnostics);
    }
    validate_non_negative(
        "tapeout_window.edge_margin_um",
        window.edge_margin_um,
        diagnostics,
    );
    if window.width_um.is_finite()
        && window.height_um.is_finite()
        && window.edge_margin_um.is_finite()
        && (window.edge_margin_um * 2.0 >= window.width_um
            || window.edge_margin_um * 2.0 >= window.height_um)
    {
        diagnostics.push(TechnologyDiagnostic {
            code: "invalid_tapeout_window_margin",
            field: "tapeout_window.edge_margin_um".into(),
            message: "Tapeout edge margin must leave a positive usable width and height.".into(),
        });
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

fn validate_non_negative(field: &str, value: f64, diagnostics: &mut Vec<TechnologyDiagnostic>) {
    if !value.is_finite() || value < 0.0 {
        diagnostics.push(TechnologyDiagnostic {
            code: "invalid_parasitic_parameter",
            field: field.into(),
            message: "Parasitic parameter must be finite and non-negative.".into(),
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

fn validate_centered_extent_on_grid(
    field: &str,
    value: f64,
    grid: f64,
    diagnostics: &mut Vec<TechnologyDiagnostic>,
) {
    if value.is_finite() && value > 0.0 && grid.is_finite() && grid > 0.0 {
        let half_extent_grid_units = value / (2.0 * grid);
        if (half_extent_grid_units - half_extent_grid_units.round()).abs() > 1e-6 {
            diagnostics.push(TechnologyDiagnostic {
                code: "density_tile_edges_off_grid",
                field: field.into(),
                message: format!(
                    "Centered density-fill tile edges must align to the {grid} µm manufacturing grid."
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CutRule, GdsLayerPurpose, LayerRule, Technology, CURRENT_TECHNOLOGY_FORMAT_VERSION,
        DEFAULT_MAX_METAL_LAYERS,
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
        assert_eq!(
            reloaded.fingerprint().unwrap(),
            technology.fingerprint().unwrap()
        );
        let mut changed = technology.clone();
        changed.physical_rules.metal.min_spacing_um += changed.physical_rules.manufacturing_grid_um;
        assert_ne!(
            changed.fingerprint().unwrap(),
            technology.fingerprint().unwrap()
        );
    }

    #[test]
    fn gf180_process_examples_are_linked_by_identity_and_fingerprint() {
        let technology = Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .expect("GF180 compatibility rule deck should load");
        let validation: serde_yaml::Value = serde_yaml::from_str(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_validation.yaml"
        ))
        .expect("GF180 validation record should be valid YAML");
        let process = &validation["process_validation"]["process"];
        let record = &validation["process_validation"]["record"];

        assert_eq!(
            process["process_id"].as_str(),
            Some(technology.process_id.as_str())
        );
        assert_eq!(
            process["deck_revision"].as_str(),
            Some(technology.deck_revision.as_str())
        );
        assert_eq!(
            process["deck_fingerprint"].as_str(),
            Some(technology.fingerprint().unwrap().as_str())
        );
        assert_eq!(process["foundry_signoff_deck"].as_bool(), Some(false));
        assert_eq!(record["overall_status"].as_str(), Some("IMPLEMENTED"));
        assert_eq!(
            validation["process_validation"]["signature"]["status"].as_str(),
            Some("UNSIGNED")
        );
        assert_eq!(technology.gds_layers.layers["metal5"][0].layer, 81);
        assert_eq!(technology.gds_layers.layers["ndiff"].len(), 2);
        assert_eq!(technology.gds_layers.layers["pdiff"].len(), 2);
        assert!(technology.gds_layers.layers["substrate"].is_empty());
    }

    #[test]
    fn density_fill_rejects_centered_tiles_with_off_grid_edges() {
        let mut technology = Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .expect("GF180 compatibility rule deck should load");
        technology
            .physical_rules
            .density_fill
            .layers
            .get_mut("metal1")
            .expect("GF180 should configure Metal1 fill")
            .fallback_tile_sizes_um = vec![1.0, 0.5, 0.25, 0.125];

        let diagnostics = technology
            .validate()
            .expect_err("a centered 0.125 µm tile cannot land both edges on a 0.005 µm grid");
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "density_tile_edges_off_grid"
                && diagnostic
                    .field
                    .ends_with("metal1.fallback_tile_sizes_um.3")
        }));
    }

    #[test]
    fn gds_layer_map_requires_every_generated_process_layer() {
        let mut technology = Technology::default();
        technology.gds_layers.layers.remove("via45");
        let diagnostics = technology.validate().unwrap_err();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "missing_gds_layer_mapping"
                && diagnostic.field == "gds_layers.layers.via45"
        }));
    }

    #[test]
    fn legacy_gf180_snapshots_receive_the_canonical_manufacturing_contract() {
        let mut technology = Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .unwrap();
        technology.gds_layers.layers.remove("pwell");
        technology.gds_layers.layers.get_mut("ndiff").unwrap()[1].enclosure_um = 0.0;
        technology.gds_layers.layers.get_mut("pdiff").unwrap()[1].enclosure_um = 0.0;
        technology.physical_rules.contact.size_um = 0.23;
        technology.physical_rules.layer_overrides.remove("metal5");
        technology.physical_rules.density_fill.layers.clear();
        technology.gds_layers.dummy_layers.clear();

        assert!(technology.migrate_legacy_gds_layers());
        assert_eq!(
            technology.gds_layers.layers["pwell"],
            vec![GdsLayerPurpose {
                purpose: "lvpwell".into(),
                layer: 204,
                datatype: 0,
                enclosure_um: 0.0,
            }]
        );
        assert_eq!(technology.gds_layers.layers["ndiff"][1].enclosure_um, 0.35);
        assert_eq!(technology.gds_layers.layers["pdiff"][1].enclosure_um, 0.35);
        assert_eq!(technology.physical_rules.contact.size_um, 0.22);
        assert!(!technology.physical_rules.density_fill.layers.is_empty());
        assert!(!technology.gds_layers.dummy_layers.is_empty());
        assert_eq!(
            technology.physical_rules.layer_overrides["metal5"],
            LayerRule {
                min_width_um: 0.44,
                min_spacing_um: 0.46,
                min_area_um2: 0.5625,
            }
        );
        assert!(technology.validate().is_ok());
        assert!(!technology.migrate_legacy_gds_layers());
    }

    #[test]
    fn legacy_unidentified_gf180_snapshot_receives_manufacturing_fill() {
        let mut technology = Technology::from_yaml(include_str!(
            "../../docs/examples/process_gf180mcu_3v3_5m_dr.yaml"
        ))
        .unwrap();
        technology.process_id = "legacy-unidentified".into();
        technology.deck_revision = "legacy".into();
        technology.source = "embedded project snapshot".into();
        technology.physical_rules.density_fill.layers.clear();
        technology.gds_layers.dummy_layers.clear();

        assert!(technology.migrate_legacy_gds_layers());
        assert!(!technology.physical_rules.density_fill.layers.is_empty());
        assert!(!technology.gds_layers.dummy_layers.is_empty());
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
    fn older_technology_receives_educational_parasitics() {
        let mut serialized = serde_json::to_value(Technology::default()).unwrap();
        serialized
            .as_object_mut()
            .unwrap()
            .remove("physical_parasitics");
        let technology: Technology = serde_json::from_value(serialized).unwrap();
        assert_eq!(
            technology.physical_parasitics,
            super::PhysicalParasiticRules::default()
        );
        assert!(technology.validate().is_ok());
    }

    #[test]
    fn older_technology_receives_the_educational_tapeout_window() {
        let mut serialized = serde_json::to_value(Technology::default()).unwrap();
        serialized.as_object_mut().unwrap().remove("tapeout_window");
        let technology: Technology = serde_json::from_value(serialized).unwrap();
        assert_eq!(technology.tapeout_window, super::TapeoutWindow::default());
        assert!(technology.validate().is_ok());
    }

    #[test]
    fn example_yaml_loads_and_parasitics_validate() {
        let technology =
            Technology::from_yaml(include_str!("../../docs/examples/openchippy-edu-5m.yaml"))
                .unwrap();
        assert_eq!(technology.name, "OpenChippy Example EDU 5M");
        assert_eq!(technology.tapeout_window.width_um, 2_920.0);
        assert_eq!(technology.tapeout_window.height_um, 3_520.0);
        assert_eq!(
            technology.physical_parasitics.layer_capacitance_ff_per_um["metal1"],
            0.20
        );
        assert_eq!(
            technology.physical_parasitics.via_capacitance_overrides_ff["via45"],
            0.045
        );

        let mut invalid = technology;
        invalid.physical_parasitics.wire_capacitance_ff_per_um = -0.1;
        invalid
            .physical_parasitics
            .layer_capacitance_ff_per_um
            .insert("metal6".into(), 0.1);
        invalid.tapeout_window.edge_margin_um = 2_000.0;
        let diagnostics = invalid.validate().unwrap_err();
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.field == "physical_parasitics.wire_capacitance_ff_per_um"
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.field == "physical_parasitics.layer_capacitance_ff_per_um.metal6"
        }));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.field == "tapeout_window.edge_margin_um" }));
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
