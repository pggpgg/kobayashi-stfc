//! Generate LCARS YAML files from officers.canonical.json.
//! Run: cargo run --bin generate_lcars [-- path/to/officers.canonical.json] [--output data/officers]
//! Output: data/officers/<faction>.lcars.yaml files grouped by faction.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use kobayashi::lcars::{LcarsAbility, LcarsDuration, LcarsEffect, LcarsFile, LcarsOfficer, LcarsScaling};
use serde::Deserialize;

const DEFAULT_INPUT: &str = "data/officers/officers.canonical.json";
const DEFAULT_OUTPUT_DIR: &str = "data/officers";

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
    #[serde(default)]
    slot: Option<String>,
    #[serde(default)]
    abilities: Vec<CanonicalAbility>,
}

#[derive(Debug, Deserialize)]
struct CanonicalAbility {
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
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    chance_by_rank: Vec<f64>,
    #[serde(default)]
    value_by_rank: Vec<f64>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let base = Path::new(&manifest_dir);

    let args: Vec<String> = std::env::args().collect();
    let mut input_path = base.join(DEFAULT_INPUT);
    let mut output_dir = base.join(DEFAULT_OUTPUT_DIR);

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--output" && i + 1 < args.len() {
            output_dir = base.join(&args[i + 1]);
            i += 2;
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

    let officers_by_faction = convert_officers_to_lcars(parsed.officers);

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
        println!("Wrote {} ({} officers)", out_path.display(), file.officers.len());
    }

    println!("Done.");
    Ok(())
}

fn faction_to_filename(faction: &str) -> String {
    let normalized = faction
        .to_lowercase()
        .replace(' ', "_")
        .replace('-', "_")
        .replace("section_31", "section31");
    match normalized.as_str() {
        "" | "unknown" | "faction" => "independent".to_string(),
        _ => normalized,
    }
}

fn convert_officers_to_lcars(officers: Vec<CanonicalOfficer>) -> HashMap<String, Vec<LcarsOfficer>> {
    let mut by_faction: HashMap<String, Vec<LcarsOfficer>> = HashMap::new();

    for officer in officers {
        let faction_key = officer
            .faction
            .as_deref()
            .unwrap_or("Unknown");
        let faction_key = faction_to_filename(faction_key);

        let lcars = convert_officer(officer);
        by_faction
            .entry(faction_key)
            .or_default()
            .push(lcars);
    }

    by_faction
}

fn convert_officer(o: CanonicalOfficer) -> LcarsOfficer {
    let mut captain_ability = None;
    let mut bridge_ability = None;
    let mut below_decks_ability = None;

    let mut captain_effects = Vec::new();
    let mut bridge_effects = Vec::new();
    let mut below_effects = Vec::new();

    for ability in o.abilities {
        let effects = match ability.slot.to_lowercase().as_str() {
            "captain" => &mut captain_effects,
            "officer" => &mut bridge_effects,
            "below" | "below_decks" => &mut below_effects,
            _ => &mut bridge_effects,
        };

        if let Some(effect) = convert_ability_to_effect(&ability, &o.name) {
            effects.push(effect);
        }
    }

    if !captain_effects.is_empty() {
        captain_ability = Some(LcarsAbility {
            name: format!("{} (Captain)", o.name),
            effects: captain_effects,
        });
    }
    if !bridge_effects.is_empty() {
        bridge_ability = Some(LcarsAbility {
            name: format!("{} (Bridge)", o.name),
            effects: bridge_effects,
        });
    }
    if !below_effects.is_empty() {
        below_decks_ability = Some(LcarsAbility {
            name: format!("{} (Below Decks)", o.name),
            effects: below_effects,
        });
    }

    LcarsOfficer {
        id: o.id,
        name: o.name,
        faction: o.faction,
        rarity: o.rarity,
        group: o.group,
        captain_ability,
        bridge_ability,
        below_decks_ability,
    }
}

fn convert_ability_to_effect(
    a: &CanonicalAbility,
    officer_name: &str,
) -> Option<LcarsEffect> {
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
        "ShipArmor" | "OfficerStatDefense" => MappedEffect::StatModify("armor".into(), "add".into(), val),
        "AllDefenses" => {
            if op.eq_ignore_ascii_case("MultiplySub") {
                MappedEffect::StatModify("shield_mitigation".into(), "add".into(), -val)
            } else {
                MappedEffect::StatModify("armor".into(), "add".into(), val)
            }
        }
        "ArmorPiercing" | "AllPiercing" => MappedEffect::StatModify("shield_pierce".into(), "add".into(), val),
        "ShieldHPMax" => MappedEffect::StatModify("shield_hp".into(), "multiply".into(), 1.0 + val),
        "HullHPMax" => MappedEffect::StatModify("hull_hp".into(), "multiply".into(), 1.0 + val),
        "ApexShred" => MappedEffect::StatModify("apex_shred".into(), "add".into(), val),
        "ApexBarrier" => MappedEffect::StatModify("apex_barrier".into(), "add".into(), val),
        "IsolyticDamage" => MappedEffect::StatModify("isolytic_damage".into(), "add".into(), val),
        "IsolyticDefense" => MappedEffect::StatModify("isolytic_defense".into(), "add".into(), val),
        "ShieldHPRepair" | "ShieldRegen" => MappedEffect::StatModify("shield_regen".into(), "add".into(), val),
        "HullHPRepair" | "HullRegen" => MappedEffect::StatModify("hull_hp_repair".into(), "add".into(), val),
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
        "MiningRate" | "CargoCapacity" | "FactionPointsGain"
        | "PveChestLootMultiplierLimitedResources" | "HostileLoot" | "CombatScavenger"
        | "SkillCloakingDuration" | "OffAbilityEffect" => {
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
    let t = a
        .target
        .as_deref()
        .unwrap_or("")
        .to_lowercase();
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
        let per_chance = if max_rank > 1 {
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
