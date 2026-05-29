use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::data::building::{infer_building_bid, DEFAULT_BUILDINGS_INDEX_PATH};
use crate::data::forbidden_chaos::{forbidden_chaos_sync_readiness_issues, load_forbidden_chaos};
use crate::data::hostile::{HostileIndex, HostileRecord, DEFAULT_HOSTILES_INDEX_PATH};
use crate::data::mapping_gap_report::{
    load_building_mapping_gaps_baseline, load_opaque_buff_allowlist,
    run_research_mapping_gaps_scan, scan_building_bonus_gaps,
    scan_canonical_officer_conditions, scan_forbidden_tech_bonus_gaps,
    unmapped_canonical_condition_rows,
};
use crate::data::officer::DEFAULT_CANONICAL_OFFICERS_PATH;
use crate::data::registry::Registry;
use crate::data::ship::{
    ExtendedShipIndex, ExtendedShipRecord, ShipIndex, ShipRecord, DEFAULT_SHIPS_EXTENDED_DIR,
};
use crate::data::support_buffs::{
    support_buff_catalog_validation_issues, SupportBuffCatalog, DEFAULT_SUPPORT_BUFFS_PATH,
};
use crate::data::upstream_hostile_ship_type::{
    upstream_ship_type_deferral_reason, upstream_ship_type_is_known_category,
};
use crate::lcars;

/// One named slice of diagnostics (registry, officers, hostiles, …).
#[derive(Debug, Clone, Serialize)]
pub struct NamedValidationReport {
    pub name: String,
    pub diagnostics: Vec<ValidationDiagnostic>,
}

/// Full strict validation output for CI / `validate_data` (JSON + Markdown).
#[derive(Debug, Clone, Serialize)]
pub struct FullDataValidationReport {
    pub generated_at: String,
    pub manifest_dir: String,
    pub summary: ValidationSummaryCounts,
    pub categories: Vec<NamedValidationReport>,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ValidationSummaryCounts {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

impl FullDataValidationReport {
    pub fn from_categories(manifest_dir: &Path, categories: Vec<NamedValidationReport>) -> Self {
        let mut summary = ValidationSummaryCounts::default();
        for cat in &categories {
            for d in &cat.diagnostics {
                match d.severity {
                    ValidationSeverity::Error => summary.errors += 1,
                    ValidationSeverity::Warning => summary.warnings += 1,
                    ValidationSeverity::Info => summary.infos += 1,
                }
            }
        }
        Self {
            generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            manifest_dir: manifest_dir.display().to_string(),
            summary,
            categories,
        }
    }

    pub fn has_errors(&self) -> bool {
        self.summary.errors > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationDiagnostic {
    pub severity: ValidationSeverity,
    pub context: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ValidationReport {
    pub diagnostics: Vec<ValidationDiagnostic>,
}

impl ValidationReport {
    pub fn append(&mut self, other: ValidationReport) {
        self.diagnostics.extend(other.diagnostics);
    }

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
    "on_own_shield_break",
    "on_enemy_shield_break",
];

/// `ship_class` strings that map cleanly in [`crate::data::hostile::ship_class_to_type`].
pub fn hostile_ship_class_is_recognized(ship_class: &str) -> bool {
    matches!(
        ship_class.trim().to_lowercase().as_str(),
        "battleship" | "explorer" | "interceptor" | "survey" | "armada"
    )
}

const OPERATOR_ENUM: &[&str] = &[
    "Add",
    "Multiply",
    "MultiplyAdd",
    "MultiplyBaseAdd",
    "MultiplyBaseSub",
    "MultiplySub",
    "Set",
    "Sub",
    "add",
    "multiply",
    "mul",
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
                base_context.to_string(),
                "missing non-empty 'id'",
            );
        } else if !seen_ids.insert(officer.id.clone()) {
            report.push(
                ValidationSeverity::Error,
                base_context.to_string(),
                format!("duplicate id '{}'", officer.id),
            );
        }

        if officer.name.trim().is_empty() {
            report.push(
                ValidationSeverity::Error,
                base_context.to_string(),
                "missing non-empty 'name'",
            );
        }

        if officer.captain_ability.is_none()
            && officer.bridge_ability.is_none()
            && officer.below_decks_ability.is_none()
        {
            report.push(
                ValidationSeverity::Warning,
                base_context.to_string(),
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
                            ValidationSeverity::Info,
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
        if let Some(ref cond) = effect.condition {
            if let Err(msg) = lcars::lcars_condition_to_spec(cond) {
                report.push(
                    ValidationSeverity::Error,
                    format!("{context}.effects[{i}]"),
                    format!("invalid effect condition: {msg}"),
                );
            }
        }
    }
}

fn mechanic_support_for_lcars_stat(stat: &str) -> Option<MechanicSupport> {
    let key = stat.to_lowercase().replace('-', "_");
    mechanic_support_for_key(&key)
}

/// Canonical `officers.canonical.json` uses PascalCase `modifier` strings; [`normalize_key`] turns them
/// into snake segments (e.g. `HullHPMax` → `hull_h_p_max`). This layer extends [`mechanic_support_for_key`]
/// with every modifier token present in the committed catalog so `validate_data` triages **unknown**
/// upstream strings rather than re-listing the whole roster.
fn mechanic_support_for_canonical_modifier_or_stat(key: &str) -> Option<MechanicSupport> {
    if let Some(support) = mechanic_support_for_key(key) {
        return Some(support);
    }

    match key {
        // `generate_lcars` `map_modifier` combat stats (beyond the generic [`mechanic_support_for_key`] set).
        "all_damage"
        | "officer_stat_attack"
        | "officer_stat_defense"
        | "ship_armor"
        | "armor_piercing"
        | "shield_piercing"
        | "accuracy"
        | "ship_dodge"
        | "hull_h_p_max"
        | "shield_h_p_max" => Some(MechanicSupport::Implemented),

        // Regeneration / proc scaffolding / bridge tags — modeled incompletely vs live STFC.
        "shield_h_p_repair"
        | "hull_h_p_repair"
        | "hull_repair"
        | "shield_repair_prev_round"
        | "officer_stat_all"
        | "officer_stat_health"
        | "add_state"
        | "add_random_state"
        | "off_ability_effect"
        | "cpt_maneuver_effect"
        | "all_reload_speed"
        | "all_load_speed"
        | "shields" => Some(MechanicSupport::Partial),

        // Explicit `map_modifier` non-combat tags + economy/exploration modifiers in the catalog.
        "mining_rate"
        | "cargo_capacity"
        | "warp_speed"
        | "faction_points_gain"
        | "pve_chest_loot_multiplier_limited_resources"
        | "hostile_loot"
        | "combat_scavenger"
        | "skill_cloaking_duration"
        | "armada_loot"
        | "cargo_protection"
        | "impulse_speed"
        | "warp_distance"
        | "combat_x_p_reward"
        | "combat_pve_rewards"
        | "repair_time"
        | "repair_costs_post"
        | "jump_and_tow_cost_eff"
        | "mining_reward"
        | "skill_cloaking_cooldown"
        | "skill_cutting_beam_pv_p_base_damage_percentage"
        | "voyager_asa_c_e"
        | "skill_cutting_beam_ability_cost"
        | "omega13_cooldown"
        | "combat_parsteel_reward"
        | "combat_tritanium_reward"
        | "combat_dilithium_reward"
        | "trellium_rewards"
        | "actian_venom_and_nanoprobe_loot"
        | "broken_ship_parts_loot"
        | "gorn_hostile_volatile_loot"
        | "hirogen_relic_and_biotoxin_loot"
        | "artifact_token_loot"
        | "wok_augment_all_loot_rewards"
        | "xindi_hostile_loot" => Some(MechanicSupport::Planned),

        _ if key.contains("loot") || key.contains("reward") => Some(MechanicSupport::Planned),

        _ => None,
    }
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
    let support = if label == "condition" {
        if lcars::is_canonical_officer_condition_resolved(raw_key) {
            Some(MechanicSupport::Implemented)
        } else {
            None
        }
    } else {
        mechanic_support_for_canonical_modifier_or_stat(&normalized)
    };

    match support {
        None => report.push(
            ValidationSeverity::Warning,
            context.clone(),
            format!("unrecognized {label} key '{raw_key}' (not mapped in mechanic matrix)"),
        ),
        Some(MechanicSupport::Implemented) => {}
        // Expected catalog limitations — keep visible for triage without counting as warnings.
        Some(MechanicSupport::Partial) => report.push(
            ValidationSeverity::Info,
            context.clone(),
            format!("recognized {label} key '{raw_key}' maps to partially implemented mechanic"),
        ),
        Some(MechanicSupport::Planned) => report.push(
            ValidationSeverity::Info,
            context.clone(),
            format!("recognized {label} key '{raw_key}' maps to planned mechanic"),
        ),
    }

    if label != "condition" && is_non_combat_key(&normalized) {
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
            | "shots_per_weapon"
            | "weapon_shots"
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

    if matches!(key, "hull_hp_repair_prev_round" | "hull_repair_prev_round") {
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
        "shield_regen_max_fraction"
            | "shield_hp_repair_max_fraction"
            | "shield_regen_max_pct"
            | "shield_hp_repair_max_pct"
            | "hull_repair_max_fraction"
            | "hull_hp_repair_max_fraction"
            | "hull_repair_max_pct"
            | "hull_hp_repair_max_pct"
    ) {
        return Some(MechanicSupport::Implemented);
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

fn normalize_building_condition(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace(['-', ' '], "_")
}

/// Whether `raw` matches a **documented** `BonusEntry.conditions` tag after the same normalization
/// as building data (`trim`, lower case, `-` / space → `_`).
///
/// Other strings may still flow through [`crate::data::building`] mode matching; this allowlist is for
/// validation triage and maintainer reports.
pub fn is_known_building_condition(raw: &str) -> bool {
    matches!(
        normalize_building_condition(raw).as_str(),
        "ship_combat"
            | "ship_combat_only"
            | "ships_only"
            | "space_combat_only"
            | "station_defense"
            | "station_defense_only"
            | "starbase_defense"
            | "defense_platform"
            | "defense_platform_only"
            | "platform_only"
            | "base_defense"
            | "defender_is_npc_hostile"
            | "defender_is_player_ship"
            | "attacker_officer_tal_not_on_bridge"
            | "attacker_ship_faction_any_fed_klg_rom"
            | "attacker_ship_type_battleship"
            | "attacker_ship_type_explorer"
            | "attacker_ship_type_interceptor"
            | "attacker_ship_type_survey"
            | "attacker_ship_type_armada"
    )
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

/// Validate extended ship index + per-ship extended records (data/ships_extended).
pub fn validate_ships_extended_dataset(path: &str) -> Result<ValidationReport, String> {
    let base = Path::new(path);
    let index_path = base.join("index.json");
    let raw = fs::read_to_string(&index_path)
        .map_err(|err| format!("unable to read '{}': {err}", index_path.display()))?;
    let index: ExtendedShipIndex = serde_json::from_str(&raw)
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
            .and_then(|raw| {
                serde_json::from_str::<ExtendedShipRecord>(&raw).map_err(|e| e.to_string())
            }) {
            Ok(extended) => {
                if extended.tiers.is_empty() {
                    report.push(
                        ValidationSeverity::Error,
                        ctx.clone(),
                        "extended ship has empty `tiers`",
                    );
                } else {
                    if !extended.tiers.iter().any(|t| t.tier == 1) {
                        report.push(
                            ValidationSeverity::Error,
                            ctx.clone(),
                            "extended ship has no tier 1 entry in `tiers`",
                        );
                    }
                    if extended.levels.is_empty() {
                        report.push(
                            ValidationSeverity::Warning,
                            ctx.clone(),
                            "extended ship has empty `levels` (no per-level shield/hull bonuses)",
                        );
                    }
                    if let Some(rec) = extended.to_ship_record(Some(1), Some(1)) {
                        if rec.hull_health <= 0.0 {
                            report.push(
                                ValidationSeverity::Error,
                                ctx.clone(),
                                format!(
                                    "tier 1 level 1 hull_health is {} (must be > 0)",
                                    rec.hull_health
                                ),
                            );
                        }
                        if rec.attack <= 0.0 {
                            report.push(
                                ValidationSeverity::Warning,
                                ctx,
                                format!(
                                    "tier 1 level 1 attack is {} (zero or negative)",
                                    rec.attack
                                ),
                            );
                        }
                    } else {
                        report.push(
                            ValidationSeverity::Error,
                            ctx,
                            "failed to resolve tier 1 level 1 ShipRecord".to_string(),
                        );
                    }
                }
            }
            Err(e) => {
                report.push(
                    ValidationSeverity::Error,
                    ctx,
                    format!("failed to load extended ship record: {e}"),
                );
            }
        }
    }

    Ok(report)
}

/// Validate hostile index + all per-hostile record files for basic structure and plausible stats.
/// `path` should be the directory containing `index.json` (typically `data/hostiles`).
///
/// `HostileRecord` may include optional data.stfc.space fields (`components`, `ability`, `stat_*`, …);
/// deserialization of those is covered by parsing each file as a full record (no per-field asserts here).
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
    let mut unknown_upstream_ship_type: BTreeMap<u32, (usize, Vec<String>)> = BTreeMap::new();
    let mut deferred_upstream_ship_type: BTreeMap<u32, (usize, Vec<String>)> = BTreeMap::new();
    let mut empty_ship_class: usize = 0;
    let mut empty_ship_class_samples: Vec<String> = Vec::new();
    let mut unknown_ship_class: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();

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
                if record.ship_class.trim().is_empty() {
                    empty_ship_class += 1;
                    if empty_ship_class_samples.len() < 4 {
                        empty_ship_class_samples.push(entry.id.clone());
                    }
                } else if !hostile_ship_class_is_recognized(&record.ship_class) {
                    let key = record.ship_class.clone();
                    let slot = unknown_ship_class
                        .entry(key)
                        .or_insert_with(|| (0, Vec::new()));
                    slot.0 += 1;
                    if slot.1.len() < 4 && !slot.1.contains(&entry.id) {
                        slot.1.push(entry.id.clone());
                    }
                }
                if !upstream_ship_type_is_known_category(record.upstream_ship_type) {
                    if upstream_ship_type_deferral_reason(record.upstream_ship_type).is_some() {
                        let slot = deferred_upstream_ship_type
                            .entry(record.upstream_ship_type)
                            .or_insert_with(|| (0, Vec::new()));
                        slot.0 += 1;
                        if slot.1.len() < 4 {
                            slot.1.push(entry.id.clone());
                        }
                    } else {
                        let slot = unknown_upstream_ship_type
                            .entry(record.upstream_ship_type)
                            .or_insert_with(|| (0, Vec::new()));
                        slot.0 += 1;
                        if slot.1.len() < 4 {
                            slot.1.push(entry.id.clone());
                        }
                    }
                }
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
    if empty_ship_class > 0 {
        let samples = empty_ship_class_samples.join(", ");
        report.push(
            ValidationSeverity::Error,
            "hostiles.ship_class",
            format!(
                "{empty_ship_class} hostile record(s) have empty `ship_class` (sample id(s): {samples})"
            ),
        );
    }
    if !unknown_ship_class.is_empty() {
        let distinct = unknown_ship_class.len();
        let total: usize = unknown_ship_class.values().map(|(n, _)| *n).sum();
        let mut parts: Vec<String> = Vec::new();
        for (cls, (count, ids)) in unknown_ship_class.iter().take(12) {
            let s = ids.join(", ");
            parts.push(format!("`{cls}`×{count} (e.g. {s})"));
        }
        let tail = if distinct > 12 { " …" } else { "" };
        let detail = parts.join("; ");
        report.push(
            ValidationSeverity::Warning,
            "hostiles.ship_class",
            format!(
                "{distinct} distinct non-standard `ship_class` value(s) ({total} hostile row(s)); combat defaults these to battleship — {detail}{tail}"
            ),
        );
    }
    for (ty, (count, sample_ids)) in deferred_upstream_ship_type {
        let samples = sample_ids.join(", ");
        let reason = upstream_ship_type_deferral_reason(ty).unwrap_or("");
        report.push(
            ValidationSeverity::Warning,
            "hostiles.upstream_ship_type",
            format!(
                "upstream_ship_type {ty} is deferred ({count} hostile row(s); sample id(s): {samples}) — {reason}; remove from DEFERRED_UPSTREAM_HOSTILE_SHIP_TYPES after documenting in KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES and docs/UPSTREAM_HOSTILE_SHIP_TYPES.md"
            ),
        );
    }
    for (ty, (count, sample_ids)) in unknown_upstream_ship_type {
        let samples = sample_ids.join(", ");
        report.push(
            ValidationSeverity::Error,
            "hostiles.upstream_ship_type",
            format!(
                "upstream_ship_type {ty} is not a documented category ({count} hostile row(s); sample id(s): {samples}) — extend KNOWN_UPSTREAM_HOSTILE_SHIP_TYPES and docs/UPSTREAM_HOSTILE_SHIP_TYPES.md, or add a temporary entry to DEFERRED_UPSTREAM_HOSTILE_SHIP_TYPES with a reason"
            ),
        );
    }

    Ok(report)
}

fn named_validation_report(
    name: &str,
    result: Result<ValidationReport, String>,
) -> NamedValidationReport {
    match result {
        Ok(r) => NamedValidationReport {
            name: name.to_string(),
            diagnostics: r.diagnostics,
        },
        Err(e) => NamedValidationReport {
            name: name.to_string(),
            diagnostics: vec![ValidationDiagnostic {
                severity: ValidationSeverity::Error,
                context: name.to_string(),
                message: e,
            }],
        },
    }
}

/// Validate `data/registry.json`: each entry path exists under `data_root` and parses as JSON.
pub fn validate_registry_dataset(data_root: &Path) -> Result<ValidationReport, String> {
    let registry_path = data_root.join("registry.json");
    let mut report = ValidationReport::default();
    if !registry_path.is_file() {
        report.push(
            ValidationSeverity::Error,
            "registry",
            format!(
                "registry.json not found at {} (run normalizers / importers first)",
                registry_path.display()
            ),
        );
        return Ok(report);
    }

    let content = fs::read_to_string(&registry_path)
        .map_err(|e| format!("{}: {e}", registry_path.display()))?;
    let registry: Registry =
        serde_json::from_str(&content).map_err(|e| format!("{}: {e}", registry_path.display()))?;

    for (name, entry) in &registry {
        let path = data_root.join(&entry.path);
        let ctx = format!("registry.{name}");
        if !path.exists() {
            report.push(
                ValidationSeverity::Error,
                ctx,
                format!("path missing: {}", path.display()),
            );
            continue;
        }
        let file_content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                report.push(ValidationSeverity::Error, ctx, format!("read failed: {e}"));
                continue;
            }
        };
        if let Err(e) = serde_json::from_str::<Value>(&file_content) {
            report.push(ValidationSeverity::Error, ctx, format!("invalid JSON: {e}"));
        }
    }

    Ok(report)
}

/// Validate forbidden/chaos tech catalog sync readiness (unique `fid`, etc.).
pub fn validate_forbidden_chaos_catalog_data(data_root: &Path) -> Result<ValidationReport, String> {
    let path = data_root.join("forbidden_chaos_tech.json");
    let mut report = ValidationReport::default();
    let Some(path_str) = path.to_str() else {
        return Ok(report);
    };
    let Some(list) = load_forbidden_chaos(path_str) else {
        return Ok(report);
    };
    for msg in forbidden_chaos_sync_readiness_issues(&list) {
        report.push(ValidationSeverity::Error, "forbidden_chaos.catalog", msg);
    }
    Ok(report)
}

/// Validate support-buff catalog metadata and modeled static combat keys.
pub fn validate_support_buffs_catalog_data(data_root: &Path) -> Result<ValidationReport, String> {
    let path = data_root.join(
        Path::new(DEFAULT_SUPPORT_BUFFS_PATH)
            .file_name()
            .unwrap_or_default(),
    );
    let mut report = ValidationReport::default();
    if !path.is_file() {
        report.push(
            ValidationSeverity::Error,
            "support_buffs.catalog",
            format!("missing {}", path.display()),
        );
        return Ok(report);
    }

    let catalog = SupportBuffCatalog::load(&path)
        .map_err(|e| format!("failed to load {}: {e}", path.display()))?;
    for msg in support_buff_catalog_validation_issues(&catalog) {
        report.push(ValidationSeverity::Error, "support_buffs.catalog", msg);
    }
    Ok(report)
}

fn env_flag_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// When `KOBAYASHI_REQUIRE_CANONICAL_CONDITION_MAPS` is `1` / `true` / `yes`, unmapped canonical
/// officer `conditions` tokens are validation **errors** instead of warnings (see
/// `docs/CANONICAL_CONDITIONS.md` § After editing canonical officers).
fn strict_canonical_officer_condition_maps_required() -> bool {
    env_flag_truthy("KOBAYASHI_REQUIRE_CANONICAL_CONDITION_MAPS")
}

/// When `KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS` is `1` / `true` / `yes`, opaque `buff_*` building
/// stats and unrecognized building `conditions` tokens are validation **errors** instead of
/// warnings (see `data/buildings/IMPORTING.md` § Mapping coverage).
fn strict_building_bonus_maps_required() -> bool {
    env_flag_truthy("KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS")
}

/// When `KOBAYASHI_REQUIRE_FORBIDDEN_TECH_MAPS` is set, forbidden-tech bonus routing gaps are
/// validation **errors** instead of warnings.
fn strict_forbidden_tech_bonus_maps_required() -> bool {
    env_flag_truthy("KOBAYASHI_REQUIRE_FORBIDDEN_TECH_MAPS")
}

/// When `KOBAYASHI_REQUIRE_RESEARCH_MAPS` is set, research mapping gap **regressions** vs
/// `data/research/mapping_gaps_baseline.json` are validation errors (see
/// [`validate_research_mapping_gaps`]).
fn strict_research_mapping_gaps_required() -> bool {
    env_flag_truthy("KOBAYASHI_REQUIRE_RESEARCH_MAPS")
}

/// Warnings (or errors when `KOBAYASHI_REQUIRE_CANONICAL_CONDITION_MAPS` is set) for canonical
/// `conditions` tokens not yet mapped for the officer LCARS pipeline.
pub fn validate_unmapped_canonical_officer_conditions(
    data_root: &Path,
) -> Result<ValidationReport, String> {
    let path = data_root.join("officers/officers.canonical.json");
    let mut report = ValidationReport::default();
    if !path.is_file() {
        report.push(
            ValidationSeverity::Error,
            "canonical.officers",
            format!("missing {}", path.display()),
        );
        return Ok(report);
    }

    let map = scan_canonical_officer_conditions(&path)?;
    let severity = if strict_canonical_officer_condition_maps_required() {
        ValidationSeverity::Error
    } else {
        ValidationSeverity::Warning
    };
    for (tok, count, examples) in unmapped_canonical_condition_rows(&map) {
        let ex = examples.join("; ");
        report.push(
            severity,
            "canonical.unmapped_condition",
            format!("token `{tok}`: {count} occurrence(s); examples: {ex}"),
        );
    }
    Ok(report)
}

/// Warnings (or errors when `KOBAYASHI_REQUIRE_RESEARCH_MAPS` is set) for research import mapping
/// hygiene: unmapped upstream buff id count vs baseline and suspect global-scope catalog rows.
pub fn validate_research_mapping_gaps(manifest_dir: &Path) -> Result<ValidationReport, String> {
    let mut report = ValidationReport::default();
    let catalog = manifest_dir.join("data/research_catalog.json");
    if !catalog.is_file() {
        report.push(
            ValidationSeverity::Info,
            "research.mapping_gaps",
            format!(
                "skipped: missing {} (populate via import_stfcspace_research.mjs)",
                catalog.display()
            ),
        );
        return Ok(report);
    }

    let gaps = match run_research_mapping_gaps_scan(manifest_dir) {
        Ok(g) => g,
        Err(e) => {
            report.push(
                ValidationSeverity::Warning,
                "research.mapping_gaps",
                format!("research gap scan failed: {e}"),
            );
            return Ok(report);
        }
    };

    report.push(
        ValidationSeverity::Info,
        "research.mapping_gaps.summary",
        format!(
            "unmapped_buff_ids={} suspect_global_scopes={} catalog_projects={}",
            gaps.summary.unmapped_buff_id_count,
            gaps.summary.suspect_global_scope_count,
            gaps.summary.catalog_projects
        ),
    );

    let regression_severity = if strict_research_mapping_gaps_required() {
        ValidationSeverity::Error
    } else {
        ValidationSeverity::Warning
    };

    if gaps.has_regression_vs_baseline() {
        if let Some(reg) = &gaps.regression {
            report.push(
                regression_severity,
                "research.mapping_gaps.regression",
                format!(
                    "mapping gap regression vs baseline: unmapped delta {:+}, suspect global scopes delta {:+}",
                    reg.unmapped_buff_ids_delta, reg.suspect_global_scopes_delta
                ),
            );
        }
    }

    const MAX_SUSPECT_SAMPLES: usize = 12;
    for row in gaps.suspect_global_scopes.iter().take(MAX_SUSPECT_SAMPLES) {
        let name = row.name.as_deref().unwrap_or("<unnamed>");
        report.push(
            ValidationSeverity::Warning,
            "research.mapping_gaps.suspect_global_scope",
            format!(
                "rid {} ({name}): unconditional `{}` at level {} with {} scope — {}",
                row.rid,
                row.stat,
                row.level,
                row.scope_category,
                row.description_snippet
            ),
        );
    }
    if gaps.suspect_global_scopes.len() > MAX_SUSPECT_SAMPLES {
        report.push(
            ValidationSeverity::Info,
            "research.mapping_gaps.suspect_global_scope",
            format!(
                "… and {} more suspect global-scope catalog rows (see report_unknown_mappings or node scripts/research_mapping_gaps.mjs)",
                gaps.suspect_global_scopes.len() - MAX_SUSPECT_SAMPLES
            ),
        );
    }

    Ok(report)
}

/// Warnings (or errors when `KOBAYASHI_REQUIRE_FORBIDDEN_TECH_MAPS` is set) for forbidden-tech
/// catalog bonus rows with no combat routing.
pub fn validate_forbidden_tech_bonus_gaps(data_root: &Path) -> Result<ValidationReport, String> {
    let mut report = ValidationReport::default();
    let catalog_path = data_root.join("forbidden_chaos_tech.json");
    if !catalog_path.is_file() {
        report.push(
            ValidationSeverity::Info,
            "forbidden_tech.bonus_routing",
            format!("skipped: missing {}", catalog_path.display()),
        );
        return Ok(report);
    }

    let gaps = scan_forbidden_tech_bonus_gaps(&catalog_path)?;
    report.push(
        ValidationSeverity::Info,
        "forbidden_tech.bonus_routing.summary",
        format!(
            "catalog_items={} routed_bonus_rows={} actionable={}",
            gaps.catalog_items,
            gaps.routed_bonus_rows,
            gaps.actionable_count()
        ),
    );

    let severity = if strict_forbidden_tech_bonus_maps_required() {
        ValidationSeverity::Error
    } else {
        ValidationSeverity::Warning
    };
    for row in &gaps.actionable {
        report.push(
            severity,
            "forbidden_tech.bonus_routing.gap",
            format!(
                "fid {} ({}) stat `{}` has no combat route (see forbidden_tech_bonus_combat_route in profile.rs)",
                row.fid,
                row.tech_name,
                row.stat
            ),
        );
    }
    Ok(report)
}

/// All validation categories for strict reports / CI (paths relative to `manifest_dir`).
pub fn all_dataset_validation_reports(manifest_dir: &Path) -> Vec<NamedValidationReport> {
    let data_root = manifest_dir.join("data");
    let mut out: Vec<NamedValidationReport> = Vec::new();

    out.push(named_validation_report(
        "registry",
        validate_registry_dataset(&data_root),
    ));

    let canonical_path = data_root.join("officers/officers.canonical.json");
    out.push(named_validation_report(
        "officers_canonical",
        validate_officer_dataset_canonical(
            canonical_path
                .to_str()
                .unwrap_or("data/officers/officers.canonical.json"),
        ),
    ));

    if let Some(officers_dir) = data_root.join("officers").to_str() {
        out.push(named_validation_report(
            "officers_lcars",
            validate_lcars_dir(officers_dir),
        ));
    }

    let ext_dir = data_root.join("ships_extended");
    if ext_dir.join("index.json").is_file() {
        if let Some(p) = ext_dir.to_str() {
            out.push(named_validation_report(
                "ships_extended",
                validate_ships_extended_dataset(p),
            ));
        }
    }

    let hostiles_dir = data_root.join("hostiles");
    if hostiles_dir.join("index.json").is_file() {
        if let Some(p) = hostiles_dir.to_str() {
            out.push(named_validation_report(
                "hostiles",
                validate_hostiles_dataset(p),
            ));
        }
    }

    let buildings_dir = data_root.join("buildings");
    if buildings_dir.join("index.json").is_file() {
        if let Some(p) = buildings_dir.to_str() {
            out.push(named_validation_report(
                "buildings",
                validate_buildings_dataset(p),
            ));
        }
    }

    out.push(named_validation_report(
        "forbidden_chaos",
        validate_forbidden_chaos_catalog_data(&data_root),
    ));

    out.push(named_validation_report(
        "forbidden_tech_bonus_routing",
        validate_forbidden_tech_bonus_gaps(&data_root),
    ));

    out.push(named_validation_report(
        "support_buffs",
        validate_support_buffs_catalog_data(&data_root),
    ));

    out.push(named_validation_report(
        "canonical_conditions",
        validate_unmapped_canonical_officer_conditions(&data_root),
    ));

    out.push(named_validation_report(
        "research_mapping_gaps",
        validate_research_mapping_gaps(manifest_dir),
    ));

    out
}

/// Build the full structured validation report (used by `validate_data` and tests).
pub fn validate_all_data_for_report(manifest_dir: &Path) -> FullDataValidationReport {
    let categories = all_dataset_validation_reports(manifest_dir);
    FullDataValidationReport::from_categories(manifest_dir, categories)
}

/// Serialize [`FullDataValidationReport`] as pretty JSON.
pub fn full_validation_report_to_json(
    report: &FullDataValidationReport,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}

/// Render [`FullDataValidationReport`] as Markdown tables (CI artifact / human triage).
pub fn full_validation_report_to_markdown(report: &FullDataValidationReport) -> String {
    use crate::data::mapping_gap_report::md_escape_cell;

    let mut s = String::new();
    s.push_str("# Kobayashi data validation report\n\n");
    s.push_str(&format!("- Generated: `{}`\n", report.generated_at));
    s.push_str(&format!("- Manifest dir: `{}`\n", report.manifest_dir));
    s.push_str(&format!(
        "- Summary: **{}** error(s), **{}** warning(s), **{}** info message(s)\n\n",
        report.summary.errors, report.summary.warnings, report.summary.infos
    ));
    s.push_str("Canonical condition triage: repo path `docs/CANONICAL_CONDITIONS.md`.\n\n");

    for cat in &report.categories {
        s.push_str(&format!("## `{}`\n\n", md_escape_cell(&cat.name)));
        if cat.diagnostics.is_empty() {
            s.push_str("_No diagnostics._\n\n");
            continue;
        }
        s.push_str("| severity | context | message |\n| --- | --- | --- |\n");
        for d in &cat.diagnostics {
            let sev = d.severity.as_str();
            let ctx = md_escape_cell(&d.context);
            let msg = md_escape_cell(&d.message);
            s.push_str(&format!("| {sev} | {ctx} | {msg} |\n"));
        }
        s.push('\n');
    }
    s
}

fn startup_validation_pairs() -> Vec<(&'static str, Result<ValidationReport, String>)> {
    let mut v = vec![(
        "officers",
        validate_officer_dataset_canonical(DEFAULT_CANONICAL_OFFICERS_PATH),
    )];

    let ext_dir = Path::new(DEFAULT_SHIPS_EXTENDED_DIR);
    if ext_dir.join("index.json").is_file() {
        v.push((
            "ships_extended",
            validate_ships_extended_dataset(DEFAULT_SHIPS_EXTENDED_DIR),
        ));
    }

    if Path::new(DEFAULT_HOSTILES_INDEX_PATH).is_file() {
        v.push(("hostiles", validate_hostiles_dataset("data/hostiles")));
    }

    if Path::new(DEFAULT_BUILDINGS_INDEX_PATH).is_file() {
        v.push(("buildings", validate_buildings_dataset("data/buildings")));
    }

    v
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

    for (label, result) in startup_validation_pairs() {
        process_report(label, result, &mut error_count, &mut warning_count);
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
///
/// **Mapping coverage:** Opaque `buff_*` bonus stats and unknown `conditions` values are emitted as
/// **one diagnostic per distinct entry** (severity `Warning` by default). When
/// `KOBAYASHI_REQUIRE_BUILDING_BONUS_MAPS=1` is set (e.g. via `cargo run --bin validate_data --
/// --strict`), those rows are upgraded to `Error` so CI / strict reports fail until the catalog is
/// extended. The same gap data is also available as Markdown via
/// `cargo run --bin report_building_mapping_gaps`.
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

    let Some(buildings) = payload.get("buildings").and_then(Value::as_array) else {
        report.push(
            ValidationSeverity::Error,
            "buildings.index",
            "missing 'buildings' array",
        );
        return Ok(report);
    };

    let mut seen_ids = HashSet::new();
    let mut seen_bids = HashSet::new();

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

        let file_opt = obj
            .get("file")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty());

        match obj.get("bid").and_then(|v| v.as_i64()) {
            Some(bid) => {
                if !seen_bids.insert(bid) {
                    report.push(
                        ValidationSeverity::Error,
                        format!("{ctx}.bid"),
                        format!("duplicate bid {bid}"),
                    );
                }
                if let Some(inf) = infer_building_bid(&id, file_opt) {
                    if inf != bid {
                        report.push(
                            ValidationSeverity::Error,
                            format!("{ctx}.bid"),
                            format!(
                                "bid {bid} disagrees with inferred {inf} from id/file (fix typo or id/file fields)"
                            ),
                        );
                    }
                }
            }
            None => {
                report.push(
                    ValidationSeverity::Error,
                    format!("{ctx}.bid"),
                    "missing or invalid 'bid' (integer upstream starbase module id required)",
                );
            }
        }

        let file_stem = file_opt.unwrap_or(id.as_str());
        let record_path = base.join(format!("{file_stem}.json"));
        if !record_path.is_file() {
            report.push(
                ValidationSeverity::Error,
                format!("{ctx}.id='{id}'"),
                format!("missing building record file '{}'", record_path.display()),
            );
            continue;
        }

        // Structural and semantic checks on the per-building file. Mapping-gap aggregation runs
        // separately below via the shared `scan_building_bonus_gaps` helper so the per-row
        // diagnostics line up with `report_building_mapping_gaps`.
        if let Ok(rec_raw) = fs::read_to_string(&record_path) {
            if let Ok(rec_json) = serde_json::from_str::<Value>(&rec_raw) {
                let Some(levels) = rec_json.get("levels").and_then(Value::as_array) else {
                    report.push(
                        ValidationSeverity::Error,
                        format!("{}.file", ctx),
                        "missing 'levels' array on building record",
                    );
                    continue;
                };

                for (level_index, level) in levels.iter().enumerate() {
                    let level_ctx = format!("{}.file.levels[{level_index}]", ctx);
                    let Some(level_obj) = level.as_object() else {
                        report.push(
                            ValidationSeverity::Error,
                            level_ctx,
                            "level entry is not an object",
                        );
                        continue;
                    };

                    let ops_min = level_obj
                        .get("ops_min")
                        .and_then(Value::as_u64)
                        .map(|v| v as u32);
                    let ops_max = level_obj
                        .get("ops_max")
                        .and_then(Value::as_u64)
                        .map(|v| v as u32);
                    if let (Some(min), Some(max)) = (ops_min, ops_max) {
                        if min > max {
                            report.push(
                                ValidationSeverity::Error,
                                level_ctx.clone(),
                                format!("ops_min {min} is greater than ops_max {max}"),
                            );
                        }
                    }

                    let Some(bonuses) = level_obj.get("bonuses").and_then(Value::as_array) else {
                        report.push(
                            ValidationSeverity::Error,
                            level_ctx,
                            "missing 'bonuses' array",
                        );
                        continue;
                    };

                    for (bonus_index, bonus) in bonuses.iter().enumerate() {
                        let bonus_ctx = format!("{}.bonuses[{bonus_index}]", level_ctx);
                        if !bonus.is_object() {
                            report.push(
                                ValidationSeverity::Error,
                                bonus_ctx,
                                "bonus entry is not an object",
                            );
                        }
                    }
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

    let gaps = scan_building_bonus_gaps(base)?;
    let allowlist_path = base.join("opaque_buff_allowlist.json");
    let allowlist = load_opaque_buff_allowlist(&allowlist_path);
    let actionable = gaps.actionable_opaque_buff_stats(&allowlist);
    let actionable_count = actionable.len();

    report.push(
        ValidationSeverity::Info,
        "buildings.bonuses.opaque_buff.summary",
        format!(
            "opaque_distinct={} allowlisted={} actionable={}",
            gaps.opaque_buff_stats.len(),
            gaps.allowlisted_opaque_buff_stats(&allowlist).len(),
            actionable_count
        ),
    );

    if strict_building_bonus_maps_required() {
        if let Some(baseline) =
            load_building_mapping_gaps_baseline(&base.join("mapping_gaps_baseline.json"))
        {
            if actionable_count > baseline.actionable_opaque_buff_stats {
                report.push(
                    ValidationSeverity::Error,
                    "buildings.bonuses.opaque_buff.regression",
                    format!(
                        "actionable opaque buff count {actionable_count} exceeds baseline {} — extend allowlist, map stat, or refresh baseline",
                        baseline.actionable_opaque_buff_stats
                    ),
                );
            }
        }
    }

    let severity = if strict_building_bonus_maps_required() {
        ValidationSeverity::Error
    } else {
        ValidationSeverity::Warning
    };
    for (stat, agg) in &actionable {
        let samples = if agg.samples.is_empty() {
            "<none>".to_string()
        } else {
            agg.samples.join(", ")
        };
        report.push(
            severity,
            "buildings.bonuses.opaque_buff",
            format!(
                "stat `{stat}`: {} bonus row(s); samples: {samples}; not merged via normalize_profile_combat_stat (data/profile.rs)",
                agg.count
            ),
        );
    }
    for (token, agg) in &gaps.unknown_conditions {
        let samples = if agg.samples.is_empty() {
            "<none>".to_string()
        } else {
            agg.samples.join(", ")
        };
        report.push(
            severity,
            "buildings.bonuses.unknown_condition",
            format!(
                "condition `{token}`: {} occurrence(s); samples: {samples}; not in is_known_building_condition (data/validate.rs)",
                agg.count
            ),
        );
    }

    Ok(report)
}
