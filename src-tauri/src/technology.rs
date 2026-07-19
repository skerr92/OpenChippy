use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::{Technology, CURRENT_TECHNOLOGY_FORMAT_VERSION, DEFAULT_MAX_METAL_LAYERS};

    #[test]
    fn educational_default_is_valid_and_serializable() {
        let technology = Technology::default();
        assert!(technology.validate().is_ok());
        assert_eq!(technology.format_version, CURRENT_TECHNOLOGY_FORMAT_VERSION);
        assert_eq!(technology.max_metal_layers, DEFAULT_MAX_METAL_LAYERS);
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
        let technology = Technology::from_yaml(source).unwrap();
        assert_eq!(technology.name, "Test EDU CMOS");
        assert_eq!(technology.max_metal_layers, 7);
        assert_eq!(technology.nmos.nominal_on_resistance_ohms, 10_000.0);
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
}
