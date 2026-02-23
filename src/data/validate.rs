use std::collections::HashSet;
use std::fmt;
use std::fs;

use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

impl ValidationSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

impl fmt::Display for ValidationSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationDiagnostic {
    pub severity: ValidationSeverity,
    pub context: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub diagnostics: Vec<ValidationDiagnostic>,
}

impl ValidationReport {
    pub fn push(
        &mut self,
        severity: ValidationSeverity,
        context: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(ValidationDiagnostic {
            severity,
            context: context.into(),
            message: message.into(),
        });
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diag| diag.severity == ValidationSeverity::Error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MechanicSupport {
    Implemented,
    Partial,
    Planned,
}

const TRIGGER_ENUM: &[&str] = &[
    "BattleWon",
    "CombatStart",
    "CriticalShotFired",
    "CriticalShotTaken",
    "EnemyTakesHit",
    "HitTaken",
    "HullDamageTaken",
    "RoundEnd",
    "RoundStart",
    "ShieldDamageTaken",
    "ShieldsDepleted",
    "ShipLaunched",
    "ShipRecalled",
    "TargetShieldsDepleted",
    "passive",
    "on_attack",
    "on_combat_end",
    "on_combat_start",
    "on_critical",
    "on_hit",
    "on_hull_breach",
    "on_kill",
    "on_receive_damage",
    "on_round_end",
    "on_round_start",
    "on_shield_break",
];

const OPERATOR_ENUM: &[&str] = &[
    "Add",
    "MultiplyAdd",
    "MultiplyBaseAdd",
    "MultiplyBaseSub",
    "MultiplySub",
    "Set",
    "Sub",
    "add",
    "mul_add",
    "mul_sub",
    "set",
    "sub",
];

pub fn validate_officer_dataset(path: &str) -> Result<ValidationReport, String> {
    let raw = fs::read_to_string(path).map_err(|err| format!("unable to read '{path}': {err}"))?;
    let payload: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("unable to parse json '{path}': {err}"))?;

    let entries = payload
        .get("officers")
        .and_then(Value::as_array)
        .or_else(|| payload.as_array())
        .ok_or_else(|| "expected top-level JSON array or { officers: [...] }".to_string())?;

    let mut report = ValidationReport::default();
    let mut seen_ids = HashSet::new();

    for (index, entry) in entries.iter().enumerate() {
        let base_context = format!("entry[{index}]");
        let Some(object) = entry.as_object() else {
            report.push(
                ValidationSeverity::Error,
                base_context,
                "entry is not an object",
            );
            continue;
        };

        let officer_id = match object.get("id").and_then(Value::as_str) {
            Some(id) if !id.trim().is_empty() => {
                if !seen_ids.insert(id.to_string()) {
                    report.push(
                        ValidationSeverity::Error,
                        format!("{base_context}.id"),
                        format!("duplicate id '{id}'"),
                    );
                }
                id.to_string()
            }
            _ => {
                report.push(
                    ValidationSeverity::Error,
                    format!("{base_context}.id"),
                    "missing non-empty 'id'",
                );
                "<missing-id>".to_string()
            }
        };

        match object.get("name").and_then(Value::as_str) {
            Some(name) if !name.trim().is_empty() => {}
            _ => report.push(
                ValidationSeverity::Error,
                format!("{base_context}.name"),
                "missing non-empty 'name'",
            ),
        }

        validate_abilities(&mut report, object, &officer_id, index);
    }

    Ok(report)
}

fn validate_abilities(
    report: &mut ValidationReport,
    object: &Map<String, Value>,
    officer_id: &str,
    entry_index: usize,
) {
    let context = format!("entry[{entry_index}] id='{officer_id}'.abilities");
    let Some(abilities) = object.get("abilities") else {
        report.push(
            ValidationSeverity::Error,
            context.clone(),
            "missing 'abilities' array",
        );
        return;
    };

    let Some(abilities) = abilities.as_array() else {
        report.push(ValidationSeverity::Error, context, "expected array");
        return;
    };

    for (ability_index, ability) in abilities.iter().enumerate() {
        let ability_context =
            format!("entry[{entry_index}] id='{officer_id}'.abilities[{ability_index}]");
        let Some(ability_obj) = ability.as_object() else {
            report.push(
                ValidationSeverity::Error,
                ability_context,
                "ability is not an object",
            );
            continue;
        };

        if let Some(trigger) = ability_obj.get("trigger").and_then(Value::as_str) {
            if !TRIGGER_ENUM.contains(&trigger) {
                report.push(
                    ValidationSeverity::Error,
                    format!("{ability_context}.trigger"),
                    format!("unsupported trigger enum '{trigger}'"),
                );
            }
        } else {
            report.push(
                ValidationSeverity::Error,
                format!("{ability_context}.trigger"),
                "missing non-empty 'trigger'",
            );
        }

        if let Some(operation) = ability_obj.get("operation").and_then(Value::as_str) {
            if !OPERATOR_ENUM.contains(&operation) {
                report.push(
                    ValidationSeverity::Error,
                    format!("{ability_context}.operation"),
                    format!("unsupported operator enum '{operation}'"),
                );
            }
        }

        if let Some(effects) = ability_obj.get("effects") {
            let Some(effects) = effects.as_array() else {
                report.push(
                    ValidationSeverity::Error,
                    format!("{ability_context}.effects"),
                    "expected array",
                );
                continue;
            };

            for (effect_index, effect) in effects.iter().enumerate() {
                let effect_context = format!("{ability_context}.effects[{effect_index}]");
                let Some(effect_obj) = effect.as_object() else {
                    report.push(
                        ValidationSeverity::Error,
                        effect_context,
                        "effect is not an object",
                    );
                    continue;
                };

                validate_effect_key(
                    report,
                    effect_context.clone(),
                    "stat",
                    effect_obj.get("stat").and_then(Value::as_str),
                );
                validate_effect_key(
                    report,
                    effect_context.clone(),
                    "condition",
                    effect_obj.get("condition").and_then(Value::as_str),
                );
                if let Some(operator) = effect_obj.get("operator").and_then(Value::as_str) {
                    if !OPERATOR_ENUM.contains(&operator) {
                        report.push(
                            ValidationSeverity::Error,
                            format!("{effect_context}.operator"),
                            format!("unsupported operator enum '{operator}'"),
                        );
                    }
                }
            }
        } else {
            validate_effect_key(
                report,
                ability_context.clone(),
                "modifier",
                ability_obj.get("modifier").and_then(Value::as_str),
            );

            if let Some(conditions) = ability_obj.get("conditions").and_then(Value::as_array) {
                for (condition_index, condition) in conditions.iter().enumerate() {
                    validate_effect_key(
                        report,
                        format!("{ability_context}.conditions[{condition_index}]"),
                        "condition",
                        condition.as_str(),
                    );
                }
            }
        }
    }
}

fn validate_effect_key(
    report: &mut ValidationReport,
    context: String,
    label: &str,
    raw_key: Option<&str>,
) {
    let Some(raw_key) = raw_key else {
        if label == "modifier" || label == "stat" {
            report.push(
                ValidationSeverity::Error,
                context,
                format!("missing non-empty '{label}'"),
            );
        }
        return;
    };

    let normalized = normalize_key(raw_key);
    match mechanic_support_for_key(&normalized) {
        None => report.push(
            ValidationSeverity::Warning,
            context.clone(),
            format!("unrecognized {label} key '{raw_key}' (not mapped in mechanic matrix)"),
        ),
        Some(MechanicSupport::Implemented) => {}
        Some(MechanicSupport::Partial) => report.push(
            ValidationSeverity::Warning,
            context.clone(),
            format!("recognized {label} key '{raw_key}' maps to partially implemented mechanic"),
        ),
        Some(MechanicSupport::Planned) => report.push(
            ValidationSeverity::Warning,
            context.clone(),
            format!("recognized {label} key '{raw_key}' maps to planned mechanic"),
        ),
    }

    if is_non_combat_key(&normalized) {
        report.push(
            ValidationSeverity::Info,
            context,
            format!("{label} key '{raw_key}' is non-combat and ignored by simulator"),
        );
    }
}

fn normalize_key(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len());
    let trimmed = raw.trim();
    for (index, ch) in trimmed.chars().enumerate() {
        if ch.is_uppercase() {
            if index != 0 {
                normalized.push('_');
            }
            normalized.extend(ch.to_lowercase());
        } else {
            normalized.push(ch.to_ascii_lowercase());
        }
    }
    normalized
}

fn mechanic_support_for_key(key: &str) -> Option<MechanicSupport> {
    if matches!(
        key,
        "shield_mitigation"
            | "damage_reduction"
            | "shield_pierce"
            | "armor_pierce"
            | "armor"
            | "ship_armor"
            | "crit_chance"
            | "crit_damage"
            | "on_critical"
            | "extra_attack"
            | "shots_per_attack"
            | "ship_dodge"
            | "accuracy"
            | "all_defenses"
            | "all_piercing"
    ) {
        return Some(MechanicSupport::Implemented);
    }

    if key.contains("burn")
        || key.contains("ignite")
        || matches!(
            key,
            "shield_regen" | "hull_repair" | "hull_hp_repair" | "shield_hp_repair"
        )
    {
        return Some(MechanicSupport::Partial);
    }

    if matches!(
        key,
        "isolytic_damage"
            | "isolytic_defense"
            | "mining_rate"
            | "repair_speed"
            | "warp_speed"
            | "cargo_capacity"
    ) || key.contains("loot")
    {
        return Some(MechanicSupport::Planned);
    }

    if matches!(key, "apex_shred" | "apex_barrier") {
        return Some(MechanicSupport::Implemented);
    }

    None
}

fn is_non_combat_key(key: &str) -> bool {
    matches!(
        key,
        "mining_rate" | "repair_speed" | "warp_speed" | "cargo_capacity"
    ) || key.contains("loot")
}
