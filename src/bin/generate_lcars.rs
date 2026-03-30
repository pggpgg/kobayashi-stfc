//! Generate LCARS YAML files from officers.canonical.json.
//! Run: cargo run --bin generate_lcars [-- path/to/officers.canonical.json] [--output data/officers]
//!   [--summary data/upstream/data-stfc-space/summary-officer.json]
//!   [--translations data/upstream/data-stfc-space/translations-officer_buffs.json]
//! Output: data/officers/<faction>.lcars.yaml files grouped by faction.
//!
//! When `--summary` and `--translations` point at data-stfc-space exports, ability block names are
//! resolved from `officer_ability_name` rows (`loca_id` in summary matches `id` in translations).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use kobayashi::lcars::{
    LcarsAbility, LcarsDuration, LcarsEffect, LcarsFile, LcarsOfficer, LcarsScaling,
};
use serde::Deserialize;

const DEFAULT_INPUT: &str = "data/officers/officers.canonical.json";
const DEFAULT_OUTPUT_DIR: &str = "data/officers";
const DEFAULT_SUMMARY: &str = "data/upstream/data-stfc-space/summary-officer.json";
const DEFAULT_TRANSLATIONS: &str = "data/upstream/data-stfc-space/translations-officer_buffs.json";

#[derive(Debug, Deserialize)]
struct CanonicalFile {
    officers: Vec<CanonicalOfficer>,
}

#[derive(Debug, Deserialize)]
struct CanonicalOfficer {
    id: String,
    name: String,
    #[serde(default)]
    faction: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    rarity: Option<String>,
    #[serde(default, rename = "slot")]
    _slot: Option<String>,
    #[serde(default)]
    source_officer_id: Option<String>,
    #[serde(default)]
    abilities: Vec<CanonicalAbility>,
}

#[derive(Debug, Deserialize)]
struct CanonicalAbility {
    #[serde(default)]
    ability_id: Option<String>,
    #[serde(default)]
    slot: String,
    #[serde(default)]
    trigger: Option<String>,
    #[serde(default)]
    modifier: Option<String>,
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    attributes: Option<String>,
    #[serde(default, rename = "description")]
    _description: Option<String>,
    #[serde(default)]
    chance_by_rank: Vec<f64>,
    #[serde(default)]
    value_by_rank: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct SummaryOfficer {
    id: u64,
    #[serde(default)]
    captain_ability: Option<SummaryAbility>,
    #[serde(default)]
    ability: Option<SummaryAbility>,
    #[serde(default)]
    below_decks_ability: Option<SummaryAbility>,
}

#[derive(Debug, Deserialize)]
struct SummaryAbility {
    id: u64,
    loca_id: u64,
}

#[derive(Debug, Deserialize)]
struct TranslationRow {
    id: Option<u64>,
    key: String,
    text: String,
}

struct NameResolveContext {
    summary_by_officer: HashMap<u64, SummaryOfficer>,
    name_by_loca: HashMap<u64, String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base = Path::new(&manifest_dir);

    let args: Vec<String> = std::env::args().collect();
    let mut input_path = base.join(DEFAULT_INPUT);
    let mut output_dir = base.join(DEFAULT_OUTPUT_DIR);
    let mut summary_path = base.join(DEFAULT_SUMMARY);
    let mut translations_path = base.join(DEFAULT_TRANSLATIONS);
    let mut skip_names = false;

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--output" && i + 1 < args.len() {
            output_dir = base.join(&args[i + 1]);
            i += 2;
        } else if args[i] == "--summary" && i + 1 < args.len() {
            summary_path = Path::new(&args[i + 1]).to_path_buf();
            if !summary_path.is_absolute() {
                summary_path = base.join(&summary_path);
            }
            i += 2;
        } else if args[i] == "--translations" && i + 1 < args.len() {
            translations_path = Path::new(&args[i + 1]).to_path_buf();
            if !translations_path.is_absolute() {
                translations_path = base.join(&translations_path);
            }
            i += 2;
        } else if args[i] == "--no-ability-names" {
            skip_names = true;
            i += 1;
        } else if !args[i].starts_with("--") {
            input_path = Path::new(&args[i]).to_path_buf();
            if !input_path.is_absolute() {
                input_path = base.join(&input_path);
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    let raw = fs::read_to_string(&input_path)?;
    let parsed: CanonicalFile = serde_json::from_str(&raw)?;

    let name_ctx = if skip_names {
        NameResolveContext {
            summary_by_officer: HashMap::new(),
            name_by_loca: HashMap::new(),
        }
    } else {
        load_name_resolve_context(&summary_path, &translations_path)?
    };

    let (officers_by_faction, names_resolved) =
        convert_officers_to_lcars(parsed.officers, &name_ctx);

    fs::create_dir_all(&output_dir)?;

    for (faction_key, officers) in officers_by_faction {
        if officers.is_empty() {
            continue;
        }
        let filename = format!("{}.lcars.yaml", faction_key);
        let out_path = output_dir.join(&filename);
        let file = LcarsFile { officers };
        let yaml = serde_yaml::to_string(&file)?;
        fs::write(&out_path, yaml)?;
        println!(
            "Wrote {} ({} officers)",
            out_path.display(),
            file.officers.len()
        );
    }

    println!(
        "Done. Ability names resolved from translations: {names_resolved} blocks (use --no-ability-names to skip).",
    );
    Ok(())
}

fn load_name_resolve_context(
    summary_path: &Path,
    translations_path: &Path,
) -> Result<NameResolveContext, Box<dyn std::error::Error>> {
    let mut summary_by_officer: HashMap<u64, SummaryOfficer> = HashMap::new();
    if summary_path.is_file() {
        let raw = fs::read_to_string(summary_path)?;
        let list: Vec<SummaryOfficer> = serde_json::from_str(&raw)?;
        for o in list {
            summary_by_officer.insert(o.id, o);
        }
    } else {
        eprintln!(
            "Warning: summary not found at {} — ability names will use placeholders.",
            summary_path.display()
        );
    }

    let mut name_by_loca: HashMap<u64, String> = HashMap::new();
    if translations_path.is_file() {
        let raw = fs::read_to_string(translations_path)?;
        let rows: Vec<TranslationRow> = serde_json::from_str(&raw)?;
        for row in rows {
            if row.key != "officer_ability_name" {
                continue;
            }
            let Some(id) = row.id else {
                continue;
            };
            name_by_loca.entry(id).or_insert_with(|| row.text.trim().to_string());
        }
    } else {
        eprintln!(
            "Warning: translations not found at {} — ability names will use placeholders.",
            translations_path.display()
        );
    }

    Ok(NameResolveContext {
        summary_by_officer,
        name_by_loca,
    })
}

fn faction_to_filename(faction: &str) -> String {
    let normalized = faction
        .to_lowercase()
        .replace([' ', '-'], "_")
        .replace("section_31", "section31");
    match normalized.as_str() {
        "" | "unknown" | "faction" => "independent".to_string(),
        _ => normalized,
    }
}

fn convert_officers_to_lcars(
    officers: Vec<CanonicalOfficer>,
    ctx: &NameResolveContext,
) -> (HashMap<String, Vec<LcarsOfficer>>, u64) {
    let mut by_faction: HashMap<String, Vec<LcarsOfficer>> = HashMap::new();
    let mut names_resolved: u64 = 0;

    for officer in officers {
        let faction_key = officer.faction.as_deref().unwrap_or("Unknown");
        let faction_key = faction_to_filename(faction_key);

        let (lcars, n) = convert_officer(officer, ctx);
        names_resolved += n;
        by_faction.entry(faction_key).or_default().push(lcars);
    }

    (by_faction, names_resolved)
}

fn parse_numeric_id(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Ok(v) = s.parse::<u64>() {
        return Some(v);
    }
    if let Ok(f) = s.parse::<f64>() {
        let u = f as u64;
        if f >= 0.0 && (f - u as f64).abs() < 1.0 {
            return Some(u);
        }
    }
    None
}

fn loca_for_game_ability(summary: &SummaryOfficer, game_ability_id: u64) -> Option<u64> {
    if let Some(a) = &summary.captain_ability {
        if a.id == game_ability_id {
            return Some(a.loca_id);
        }
    }
    if let Some(a) = &summary.ability {
        if a.id == game_ability_id {
            return Some(a.loca_id);
        }
    }
    if let Some(a) = &summary.below_decks_ability {
        if a.id == game_ability_id {
            return Some(a.loca_id);
        }
    }
    None
}

fn resolve_ability_display_name(
    ability: &CanonicalAbility,
    summary: Option<&SummaryOfficer>,
    ctx: &NameResolveContext,
) -> Option<String> {
    let aid = ability.ability_id.as_deref()?;
    let game_id = parse_numeric_id(aid)?;
    let so = summary?;
    let loca = loca_for_game_ability(so, game_id)?;
    let name = ctx.name_by_loca.get(&loca).cloned()?;
    if name.is_empty() {
        return None;
    }
    Some(name)
}

fn ability_block_name(
    seat_label: &str,
    abilities: &[CanonicalAbility],
    summary: Option<&SummaryOfficer>,
    ctx: &NameResolveContext,
    officer_name: &str,
) -> (String, bool) {
    for a in abilities {
        if let Some(n) = resolve_ability_display_name(a, summary, ctx) {
            return (n, true);
        }
    }
    (
        format!("{} ({})", officer_name, seat_label),
        false,
    )
}

fn convert_officer(o: CanonicalOfficer, ctx: &NameResolveContext) -> (LcarsOfficer, u64) {
    let summary = o
        .source_officer_id
        .as_deref()
        .and_then(parse_numeric_id)
        .and_then(|id| ctx.summary_by_officer.get(&id));

    let mut captain_abs: Vec<CanonicalAbility> = Vec::new();
    let mut bridge_abs: Vec<CanonicalAbility> = Vec::new();
    let mut below_abs: Vec<CanonicalAbility> = Vec::new();

    for ability in o.abilities {
        match ability.slot.to_lowercase().as_str() {
            "captain" => captain_abs.push(ability),
            "officer" => bridge_abs.push(ability),
            "below" | "below_decks" => below_abs.push(ability),
            _ => bridge_abs.push(ability),
        }
    }

    let mut names_resolved: u64 = 0;

    let captain_ability = if !captain_abs.is_empty() {
        let effects: Vec<LcarsEffect> = captain_abs
            .iter()
            .filter_map(|a| convert_ability_to_effect(a, &o.name))
            .collect();
        let (name, ok) = ability_block_name("Captain", &captain_abs, summary, ctx, &o.name);
        if ok {
            names_resolved += 1;
        }
        Some(LcarsAbility { name, effects })
    } else {
        None
    };

    let bridge_ability = if !bridge_abs.is_empty() {
        let effects: Vec<LcarsEffect> = bridge_abs
            .iter()
            .filter_map(|a| convert_ability_to_effect(a, &o.name))
            .collect();
        let (name, ok) = ability_block_name("Bridge", &bridge_abs, summary, ctx, &o.name);
        if ok {
            names_resolved += 1;
        }
        Some(LcarsAbility { name, effects })
    } else {
        None
    };

    let below_decks_ability = if !below_abs.is_empty() {
        let effects: Vec<LcarsEffect> = below_abs
            .iter()
            .filter_map(|a| convert_ability_to_effect(a, &o.name))
            .collect();
        let (name, ok) = ability_block_name("Below Decks", &below_abs, summary, ctx, &o.name);
        if ok {
            names_resolved += 1;
        }
        Some(LcarsAbility { name, effects })
    } else {
        None
    };

    (
        LcarsOfficer {
            id: o.id,
            name: o.name,
            faction: o.faction,
            rarity: o.rarity,
            group: o.group,
            captain_ability,
            bridge_ability,
            below_decks_ability,
        },
        names_resolved,
    )
}

fn convert_ability_to_effect(a: &CanonicalAbility, _officer_name: &str) -> Option<LcarsEffect> {
    let modifier = a.modifier.as_deref().unwrap_or("");
    let trigger = map_trigger(a.trigger.as_deref().unwrap_or("ShipLaunched"));
    let mapped = map_modifier(modifier, a)?;
    let scaling = scaling_from_ranks(&a.value_by_rank, &a.chance_by_rank, modifier);
    let target = map_target(a);

    match mapped {
        MappedEffect::Tag(tag_name) => Some(LcarsEffect {
            effect_type: "tag".to_string(),
            stat: None,
            target: Some(target.to_string()),
            operator: None,
            value: a.value_by_rank.first().copied(),
            trigger: Some(trigger.to_string()),
            duration: Some(LcarsDuration::Permanent("permanent".to_string())),
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: Some(tag_name),
            accumulate: None,
            decay: None,
        }),
        MappedEffect::State(state_type, chance) => {
            let effect_type = match state_type {
                StateType::Morale => "morale",
                StateType::Assimilated => "assimilated",
                StateType::HullBreach => "hull_breach",
                StateType::Burning => "burning",
            };
            Some(LcarsEffect {
                effect_type: effect_type.to_string(),
                stat: None,
                target: Some(target.to_string()),
                operator: None,
                value: None,
                trigger: Some(trigger.to_string()),
                duration: Some(LcarsDuration::Permanent("permanent".to_string())),
                scaling: scaling_from_ranks(&[], &a.chance_by_rank, "AddState"),
                condition: None,
                chance: Some(chance),
                multiplier: None,
                tag: None,
                accumulate: None,
                decay: None,
            })
        }
        MappedEffect::StatModify(stat, operator, value) => Some(LcarsEffect {
            effect_type: "stat_modify".to_string(),
            stat: Some(stat),
            target: Some(target.to_string()),
            operator: Some(operator),
            value: Some(value),
            trigger: Some(trigger.to_string()),
            duration: Some(LcarsDuration::Permanent("permanent".to_string())),
            scaling,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
            accumulate: None,
            decay: None,
        }),
    }
}

enum MappedEffect {
    Tag(String),
    State(StateType, f64),
    StatModify(String, String, f64),
}

enum StateType {
    Morale,
    Assimilated,
    HullBreach,
    Burning,
}

fn map_modifier(modifier: &str, a: &CanonicalAbility) -> Option<MappedEffect> {
    let val = a.value_by_rank.first().copied().unwrap_or(0.0);
    let op = a.operation.as_deref().unwrap_or("Add");
    let chance = a.chance_by_rank.first().copied().unwrap_or(1.0);

    let result = match modifier {
        "CritChance" => MappedEffect::StatModify("crit_chance".into(), "add".into(), val),
        "CritDamage" => MappedEffect::StatModify("crit_damage".into(), "add".into(), val),
        "AllDamage" | "OfficerStatAttack" => {
            let (op_str, v) = if op.eq_ignore_ascii_case("MultiplyAdd") {
                ("multiply", 1.0 + val)
            } else {
                ("add", val)
            };
            MappedEffect::StatModify("weapon_damage".into(), op_str.into(), v)
        }
        "ShipArmor" | "OfficerStatDefense" => {
            MappedEffect::StatModify("armor".into(), "add".into(), val)
        }
        "AllDefenses" => {
            if op.eq_ignore_ascii_case("MultiplySub") {
                MappedEffect::StatModify("shield_mitigation".into(), "add".into(), -val)
            } else {
                MappedEffect::StatModify("armor".into(), "add".into(), val)
            }
        }
        "ArmorPiercing" | "AllPiercing" => {
            MappedEffect::StatModify("shield_pierce".into(), "add".into(), val)
        }
        "ShieldHPMax" => MappedEffect::StatModify("shield_hp".into(), "multiply".into(), 1.0 + val),
        "HullHPMax" => MappedEffect::StatModify("hull_hp".into(), "multiply".into(), 1.0 + val),
        "ApexShred" => MappedEffect::StatModify("apex_shred".into(), "add".into(), val),
        "ApexBarrier" => MappedEffect::StatModify("apex_barrier".into(), "add".into(), val),
        "IsolyticDamage" => MappedEffect::StatModify("isolytic_damage".into(), "add".into(), val),
        "IsolyticDefense" => MappedEffect::StatModify("isolytic_defense".into(), "add".into(), val),
        "IsolyticCascade" | "IsolyticCascadeDamage" => {
            MappedEffect::StatModify("isolytic_cascade_damage".into(), "add".into(), val)
        }
        "ShieldHPRepair" | "ShieldRegen" => {
            MappedEffect::StatModify("shield_regen".into(), "add".into(), val)
        }
        "HullHPRepair" | "HullRegen" => {
            MappedEffect::StatModify("hull_hp_repair".into(), "add".into(), val)
        }
        "AddState" => {
            let attrs = a.attributes.as_deref().unwrap_or("").to_lowercase();
            if attrs.contains("state8") || attrs.contains("morale") {
                MappedEffect::State(StateType::Morale, chance)
            } else if attrs.contains("state64") || attrs.contains("assimilat") {
                MappedEffect::State(StateType::Assimilated, chance)
            } else if attrs.contains("state4") || attrs.contains("hullbreach") {
                MappedEffect::State(StateType::HullBreach, chance)
            } else if attrs.contains("state2") || attrs.contains("burning") {
                MappedEffect::State(StateType::Burning, chance)
            } else {
                MappedEffect::Tag(format!("add_state:{}", modifier.to_lowercase()))
            }
        }
        "MiningRate"
        | "CargoCapacity"
        | "FactionPointsGain"
        | "PveChestLootMultiplierLimitedResources"
        | "HostileLoot"
        | "CombatScavenger"
        | "SkillCloakingDuration"
        | "OffAbilityEffect" => {
            MappedEffect::Tag(format!("{}:non_combat", modifier.to_lowercase()))
        }
        _ => MappedEffect::Tag(format!("{}:unmapped", modifier.to_lowercase())),
    };

    Some(result)
}

fn map_trigger(canonical: &str) -> &'static str {
    match canonical {
        "ShipLaunched" => "passive",
        "CombatStart" => "on_combat_start",
        "RoundStart" => "on_round_start",
        "EnemyTakesHit" | "HitTaken" | "CriticalShotFired" => "on_hit",
        "ShieldsDepleted" => "on_shield_break",
        "Kill" | "EnemyKilled" => "on_kill",
        _ => "passive",
    }
}

fn map_target(a: &CanonicalAbility) -> &'static str {
    let t = a.target.as_deref().unwrap_or("").to_lowercase();
    if t.contains("enemy") {
        "enemy"
    } else {
        "self"
    }
}

fn scaling_from_ranks(
    value_by_rank: &[f64],
    chance_by_rank: &[f64],
    modifier: &str,
) -> Option<LcarsScaling> {
    if value_by_rank.len() < 2 && chance_by_rank.len() < 2 {
        return None;
    }

    let max_rank = (value_by_rank.len().max(chance_by_rank.len())) as u8;
    if max_rank < 2 {
        return None;
    }

    let base = value_by_rank.first().copied().unwrap_or(0.0);
    let last = value_by_rank.last().copied().unwrap_or(base);
    let per_rank = if max_rank > 1 {
        (last - base) / (max_rank - 1) as f64
    } else {
        0.0
    };

    if modifier.eq_ignore_ascii_case("AddState") {
        let base_chance = chance_by_rank.first().copied().unwrap_or(0.0);
        let last_chance = chance_by_rank.last().copied().unwrap_or(base_chance);
        let _per_chance = if max_rank > 1 {
            (last_chance - base_chance) / (max_rank - 1) as f64
        } else {
            0.0
        };
        Some(LcarsScaling {
            base: Some(base),
            per_rank: Some(per_rank),
            max_rank: Some(max_rank),
            base_chance: Some(base_chance),
        })
    } else {
        Some(LcarsScaling {
            base: Some(base),
            per_rank: Some(per_rank),
            max_rank: Some(max_rank),
            base_chance: None,
        })
    }
}
