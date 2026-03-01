use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::data::hostile::{HostileIndex, HostileRecord, DEFAULT_HOSTILES_INDEX_PATH};
use crate::data::officer::DEFAULT_CANONICAL_OFFICERS_PATH;
use crate::data::ship::{ShipIndex, ShipRecord, DEFAULT_SHIPS_INDEX_PATH};
use crate::lcars;

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

/// Validate a path: if directory, validate LCARS YAML files; if file, validate canonical JSON.
pub fn validate_officer_dataset(path: &str) -> Result<ValidationReport, String> {
    let p = Path::new(path);
    if p.is_dir() {
        validate_lcars_dir(path)
    } else {
        validate_officer_dataset_canonical(path)
    }
}

/// Validate LCARS YAML files in a directory.
pub fn validate_lcars_dir(path: &str) -> Result<ValidationReport, String> {
    let officers = lcars::load_lcars_dir(path)
        .map_err(|e| format!("failed to load LCARS from '{path}': {e}"))?;

    let mut report = ValidationReport::default();
    let mut seen_ids = HashSet::new();

    for (file_index, officer) in officers.iter().enumerate() {
        let base_context = format!("officer[{file_index}] id='{}'", officer.id);

        if officer.id.trim().is_empty() {
            report.push(
                ValidationSeverity::Error,
                format!("{base_context}"),
                "missing non-empty 'id'",
            );
        } else if !seen_ids.insert(officer.id.clone()) {
            report.push(
                ValidationSeverity::Error,
                format!("{base_context}"),
                format!("duplicate id '{}'", officer.id),
            );
        }

        if officer.name.trim().is_empty() {
            report.push(
                ValidationSeverity::Error,
                format!("{base_context}"),
                "missing non-empty 'name'",
            );
        }

        if officer.captain_ability.is_none()
            && officer.bridge_ability.is_none()
            && officer.below_decks_ability.is_none()
        {
            report.push(
                ValidationSeverity::Warning,
                format!("{base_context}"),
                "officer has no abilities defined",
            );
        }

        for (slot, ability_opt) in [
            ("captain_ability", &officer.captain_ability),
            ("bridge_ability", &officer.bridge_ability),
            ("below_decks_ability", &officer.below_decks_ability),
        ] {
            if let Some(ability) = ability_opt {
                validate_lcars_ability(&mut report, &base_context, slot, ability);
            }
        }
    }

    Ok(report)
}

fn validate_lcars_ability(
    report: &mut ValidationReport,
    base_context: &str,
    slot: &str,
    ability: &lcars::LcarsAbility,
) {
    let context = format!("{base_context}.{slot}");
    if ability.name.trim().is_empty() {
        report.push(
            ValidationSeverity::Warning,
            context.clone(),
            "ability has empty name",
        );
    }
    for (i, effect) in ability.effects.iter().enumerate() {
        let eff_ctx = format!("{context}.effects[{i}]");
        if effect.effect_type.trim().is_empty() {
            report.push(
                ValidationSeverity::Error,
                eff_ctx.clone(),
                "effect has empty type",
            );
        }
        if effect.effect_type == "stat_modify" {
            if let Some(ref stat) = effect.stat {
                if let Some(support) = mechanic_support_for_lcars_stat(stat) {
                    if matches!(support, MechanicSupport::Partial) {
                        report.push(
                            ValidationSeverity::Warning,
                            eff_ctx.clone(),
                            format!("stat '{stat}' maps to partially implemented mechanic"),
                        );
                    } else if matches!(support, MechanicSupport::Planned) {
                        report.push(
                            ValidationSeverity::Info,
                            eff_ctx,
                            format!("stat '{stat}' maps to planned mechanic"),
                        );
                    }
                }
            } else {
                report.push(
                    ValidationSeverity::Warning,
                    eff_ctx,
                    "stat_modify effect missing 'stat'",
                );
            }
        }
    }
}

fn mechanic_support_for_lcars_stat(stat: &str) -> Option<MechanicSupport> {
    let key = stat.to_lowercase().replace('-', "_");
    mechanic_support_for_key(&key)
}

/// Validate canonical JSON officer dataset.
pub fn validate_officer_dataset_canonical(path: &str) -> Result<ValidationReport, String> {
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
            | "isolytic_damage"
            | "isolytic_defense"
            | "isolytic_cascade"
            | "isolytic_cascade_damage"
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
        "mining_rate" | "repair_speed" | "warp_speed" | "cargo_capacity"
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

/// Validate ship index + all per-ship record files for basic structure and plausible stats.
/// `path` should be the directory containing `index.json` (typically `data/ships`).
pub fn validate_ships_dataset(path: &str) -> Result<ValidationReport, String> {
    let base = Path::new(path);
    let index_path = base.join("index.json");
    let raw = fs::read_to_string(&index_path)
        .map_err(|err| format!("unable to read '{}': {err}", index_path.display()))?;
    let index: ShipIndex = serde_json::from_str(&raw)
        .map_err(|err| format!("unable to parse '{}': {err}", index_path.display()))?;

    let mut report = ValidationReport::default();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for (idx, entry) in index.ships.iter().enumerate() {
        let ctx = format!("ships[{idx}] id='{}'", entry.id);

        if entry.id.trim().is_empty() {
            report.push(ValidationSeverity::Error, ctx, "missing non-empty 'id'");
            continue;
        }
        if !seen_ids.insert(entry.id.clone()) {
            report.push(
                ValidationSeverity::Error,
                format!("{ctx}.id"),
                format!("duplicate id '{}'", entry.id),
            );
        }
        if entry.ship_name.trim().is_empty() {
            report.push(
                ValidationSeverity::Error,
                ctx.clone(),
                "missing non-empty 'ship_name'",
            );
        }

        let record_path = base.join(format!("{}.json", entry.id));
        if !record_path.is_file() {
            report.push(
                ValidationSeverity::Error,
                ctx.clone(),
                format!("missing ship record file '{}'", record_path.display()),
            );
            continue;
        }

        match fs::read_to_string(&record_path)
            .map_err(|e| e.to_string())
            .and_then(|raw| serde_json::from_str::<ShipRecord>(&raw).map_err(|e| e.to_string()))
        {
            Ok(record) => {
                if record.hull_health <= 0.0 {
                    report.push(
                        ValidationSeverity::Error,
                        ctx.clone(),
                        format!("hull_health is {} (must be > 0)", record.hull_health),
                    );
                }
                if record.attack <= 0.0 {
                    report.push(
                        ValidationSeverity::Warning,
                        ctx,
                        format!("attack is {} (zero or negative)", record.attack),
                    );
                }
            }
            Err(e) => {
                report.push(
                    ValidationSeverity::Error,
                    ctx,
                    format!("failed to load ship record: {e}"),
                );
            }
        }
    }

    Ok(report)
}

/// Validate hostile index + all per-hostile record files for basic structure and plausible stats.
/// `path` should be the directory containing `index.json` (typically `data/hostiles`).
///
/// Individual missing/corrupt file counts are emitted as summary diagnostics rather than
/// one diagnostic per file to avoid flooding the output for large hostile sets.
pub fn validate_hostiles_dataset(path: &str) -> Result<ValidationReport, String> {
    let base = Path::new(path);
    let index_path = base.join("index.json");
    let raw = fs::read_to_string(&index_path)
        .map_err(|err| format!("unable to read '{}': {err}", index_path.display()))?;
    let index: HostileIndex = serde_json::from_str(&raw)
        .map_err(|err| format!("unable to parse '{}': {err}", index_path.display()))?;

    let mut report = ValidationReport::default();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut missing_files: usize = 0;
    let mut parse_errors: usize = 0;
    let mut bad_stats: usize = 0;

    for (idx, entry) in index.hostiles.iter().enumerate() {
        let ctx = format!("hostiles[{idx}] id='{}'", entry.id);

        if entry.id.trim().is_empty() {
            report.push(ValidationSeverity::Error, ctx, "missing non-empty 'id'");
            continue;
        }
        if !seen_ids.insert(entry.id.clone()) {
            report.push(
                ValidationSeverity::Error,
                format!("{ctx}.id"),
                format!("duplicate id '{}'", entry.id),
            );
        }

        let record_path = base.join(format!("{}.json", entry.id));
        if !record_path.is_file() {
            missing_files += 1;
            continue;
        }

        match fs::read_to_string(&record_path)
            .map_err(|e| e.to_string())
            .and_then(|raw| serde_json::from_str::<HostileRecord>(&raw).map_err(|e| e.to_string()))
        {
            Ok(record) => {
                if record.hull_health <= 0.0 {
                    bad_stats += 1;
                }
            }
            Err(_) => {
                parse_errors += 1;
            }
        }
    }

    // Emit summary diagnostics to avoid thousands of individual lines.
    if missing_files > 0 {
        report.push(
            ValidationSeverity::Error,
            "hostiles.records",
            format!(
                "{missing_files} hostile record file(s) referenced in index but not found on disk"
            ),
        );
    }
    if parse_errors > 0 {
        report.push(
            ValidationSeverity::Error,
            "hostiles.records",
            format!("{parse_errors} hostile record file(s) failed to parse"),
        );
    }
    if bad_stats > 0 {
        report.push(
            ValidationSeverity::Error,
            "hostiles.records",
            format!("{bad_stats} hostile record(s) have hull_health ≤ 0"),
        );
    }

    Ok(report)
}

/// Run all startup data validations and print per-category results to stdout.
///
/// Returns `Ok(())` when there are no errors (warnings are printed but allowed).
/// Returns `Err(message)` when any category has errors; the caller should treat
/// this as a fatal startup failure.
pub fn validate_all_startup_data() -> Result<(), String> {
    let mut error_count: usize = 0;
    let mut warning_count: usize = 0;

    fn process_report(
        label: &str,
        result: Result<ValidationReport, String>,
        errors: &mut usize,
        warnings: &mut usize,
    ) {
        match result {
            Err(e) => {
                println!("    [error] {e}");
                *errors += 1;
            }
            Ok(report) => {
                for d in &report.diagnostics {
                    match d.severity {
                        ValidationSeverity::Error => {
                            println!("    [error] {}: {}", d.context, d.message);
                            *errors += 1;
                        }
                        ValidationSeverity::Warning => {
                            println!("    [warn]  {}: {}", d.context, d.message);
                            *warnings += 1;
                        }
                        ValidationSeverity::Info => {}
                    }
                }
                if !report.has_errors() {
                    let w = report
                        .diagnostics
                        .iter()
                        .filter(|d| d.severity == ValidationSeverity::Warning)
                        .count();
                    if w == 0 {
                        println!("  {label}: ok");
                    } else {
                        println!("  {label}: ok ({w} warning(s))");
                    }
                } else {
                    println!("  {label}: ERRORS — see above");
                }
            }
        }
    }

    // Officers are always required.
    let r = validate_officer_dataset_canonical(DEFAULT_CANONICAL_OFFICERS_PATH);
    process_report("officers", r, &mut error_count, &mut warning_count);

    // Ships and hostiles are optional — only validate if the index file is present.
    if Path::new(DEFAULT_SHIPS_INDEX_PATH).is_file() {
        let r = validate_ships_dataset("data/ships");
        process_report("ships", r, &mut error_count, &mut warning_count);
    }

    if Path::new(DEFAULT_HOSTILES_INDEX_PATH).is_file() {
        let r = validate_hostiles_dataset("data/hostiles");
        process_report("hostiles", r, &mut error_count, &mut warning_count);
    }

    if error_count == 0 {
        Ok(())
    } else {
        Err(format!(
            "{error_count} data validation error(s) — fix the above before starting the server"
        ))
    }
}

/// Validate building index + per-building files for basic structure and provenance.
/// `path` should be the directory containing `index.json` (typically `data/buildings`).
pub fn validate_buildings_dataset(path: &str) -> Result<ValidationReport, String> {
    let base = Path::new(path);
    let index_path = base.join("index.json");
    let raw = fs::read_to_string(&index_path)
        .map_err(|err| format!("unable to read '{}': {err}", index_path.display()))?;
    let payload: Value = serde_json::from_str(&raw)
        .map_err(|err| format!("unable to parse json '{}': {err}", index_path.display()))?;

    let mut report = ValidationReport::default();

    let data_version = payload.get("data_version");
    if data_version.is_none() {
        report.push(
            ValidationSeverity::Warning,
            "buildings.index",
            "missing optional 'data_version' (recommended for provenance)",
        );
    }

    let Some(buildings) = payload
        .get("buildings")
        .and_then(Value::as_array)
    else {
        report.push(
            ValidationSeverity::Error,
            "buildings.index",
            "missing 'buildings' array",
        );
        return Ok(report);
    };

    let mut seen_ids = HashSet::new();
    for (idx, entry) in buildings.iter().enumerate() {
        let ctx = format!("buildings.index.buildings[{idx}]");
        let Some(obj) = entry.as_object() else {
            report.push(
                ValidationSeverity::Error,
                ctx.clone(),
                "entry is not an object",
            );
            continue;
        };

        let id = match obj.get("id").and_then(Value::as_str) {
            Some(id) if !id.trim().is_empty() => {
                if !seen_ids.insert(id.to_string()) {
                    report.push(
                        ValidationSeverity::Error,
                        format!("{ctx}.id"),
                        format!("duplicate id '{id}'"),
                    );
                }
                id.to_string()
            }
            _ => {
                report.push(
                    ValidationSeverity::Error,
                    format!("{ctx}.id"),
                    "missing non-empty 'id'",
                );
                continue;
            }
        };

        if let Some(name) = obj.get("building_name").and_then(Value::as_str) {
            if name.trim().is_empty() {
                report.push(
                    ValidationSeverity::Error,
                    format!("{ctx}.building_name"),
                    "missing non-empty 'building_name'",
                );
            }
        } else {
            report.push(
                ValidationSeverity::Error,
                format!("{ctx}.building_name"),
                "missing non-empty 'building_name'",
            );
        }

        let record_path = base.join(format!("{id}.json"));
        if !record_path.is_file() {
            report.push(
                ValidationSeverity::Error,
                format!("{ctx}.id='{id}'"),
                format!(
                    "missing building record file '{}'",
                    record_path.display()
                ),
            );
            continue;
        }

        // Light-weight structural checks on the per-building file.
        if let Ok(rec_raw) = fs::read_to_string(&record_path) {
            if let Ok(rec_json) = serde_json::from_str::<Value>(&rec_raw) {
                if rec_json.get("levels").and_then(Value::as_array).is_none() {
                    report.push(
                        ValidationSeverity::Error,
                        format!("{}.file", ctx),
                        "missing 'levels' array on building record",
                    );
                }
            } else {
                report.push(
                    ValidationSeverity::Error,
                    format!("{}.file", ctx),
                    "unable to parse building record JSON",
                );
            }
        }
    }

    Ok(report)
}
