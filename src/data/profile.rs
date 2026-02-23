//! Player profile: effective_bonuses applied as pre-combat modifier layer (DESIGN §5).
//! Keys match engine/LCARS stats: weapon_damage, hull_hp, shield_hp, crit_chance, crit_damage, pierce, etc.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::combat::Combatant;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerProfile {
    #[serde(default)]
    pub bonuses: HashMap<String, f64>,
}

pub const DEFAULT_PROFILE_PATH: &str = "data/profile.json";

/// Load profile from JSON file. Returns default (empty bonuses) if file missing or invalid.
pub fn load_profile(path: &str) -> PlayerProfile {
    let path = Path::new(path);
    if !path.exists() {
        return PlayerProfile::default();
    }
    let raw = match fs::read_to_string(path) {
        Ok(s) => s,
        _ => return PlayerProfile::default(),
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn get_bonus(profile: &PlayerProfile, key: &str) -> f64 {
    profile.bonuses.get(key).copied().unwrap_or(0.0)
}

/// Apply LCARS/officer static buffs to a Combatant (e.g. from [BuffSet::static_buffs]).
/// Intended for use when building a Combatant from ship/hostile + crew where crew is resolved via
/// [crate::lcars::resolve_crew_to_buff_set]. Keys applied: isolytic_damage, isolytic_defense,
/// shield_mitigation (additive; shield_mitigation clamped to [0, 1]).
pub fn apply_static_buffs_to_combatant(
    combatant: Combatant,
    static_buffs: &HashMap<String, f64>,
) -> Combatant {
    if static_buffs.is_empty() {
        return combatant;
    }
    let isolytic_damage_add = static_buffs.get("isolytic_damage").copied().unwrap_or(0.0);
    let isolytic_defense_add = static_buffs.get("isolytic_defense").copied().unwrap_or(0.0);
    let shield_mitigation_add = static_buffs.get("shield_mitigation").copied().unwrap_or(0.0);

    Combatant {
        isolytic_damage: (combatant.isolytic_damage + isolytic_damage_add).max(0.0),
        isolytic_defense: (combatant.isolytic_defense + isolytic_defense_add).max(0.0),
        shield_mitigation: (combatant.shield_mitigation + shield_mitigation_add)
            .max(0.0)
            .min(1.0),
        ..combatant
    }
}

/// Apply effective_bonuses to attacker Combatant (multipliers and additive bonuses).
/// Keys: weapon_damage, hull_hp, shield_hp, crit_chance, crit_damage, pierce (additive), shield_mitigation (additive to base).
pub fn apply_profile_to_attacker(attacker: Combatant, profile: &PlayerProfile) -> Combatant {
    if profile.bonuses.is_empty() {
        return attacker;
    }
    let weapon = 1.0 + get_bonus(profile, "weapon_damage");
    let hull_hp = 1.0 + get_bonus(profile, "hull_hp");
    let shield_hp = 1.0 + get_bonus(profile, "shield_hp");
    let crit_chance_add = get_bonus(profile, "crit_chance");
    let crit_damage_mult = 1.0 + get_bonus(profile, "crit_damage");
    let pierce_add = get_bonus(profile, "pierce");
    let shield_mit_add = get_bonus(profile, "shield_mitigation");

    Combatant {
        attack: attacker.attack * weapon,
        hull_health: attacker.hull_health * hull_hp,
        shield_health: attacker.shield_health * shield_hp,
        crit_chance: (attacker.crit_chance + crit_chance_add).max(0.0).min(1.0),
        crit_multiplier: (attacker.crit_multiplier * crit_damage_mult).max(0.0),
        pierce: (attacker.pierce + pierce_add).max(0.0),
        shield_mitigation: (attacker.shield_mitigation + shield_mit_add).max(0.0).min(1.0),
        ..attacker
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::combat::Combatant;

    use super::*;

    fn combatant_with(isolytic_damage: f64, isolytic_defense: f64, shield_mitigation: f64) -> Combatant {
        Combatant {
            id: "test".to_string(),
            attack: 0.0,
            mitigation: 0.0,
            pierce: 0.0,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            isolytic_damage,
            isolytic_defense,
        }
    }

    #[test]
    fn apply_static_buffs_to_combatant_applies_and_clamps() {
        let c = combatant_with(0.0, 0.0, 0.5);
        let mut buffs = HashMap::new();
        buffs.insert("isolytic_damage".to_string(), 0.1);
        buffs.insert("isolytic_defense".to_string(), 10.0);
        buffs.insert("shield_mitigation".to_string(), 0.3);
        let out = apply_static_buffs_to_combatant(c, &buffs);
        assert_eq!(out.isolytic_damage, 0.1);
        assert_eq!(out.isolytic_defense, 10.0);
        assert_eq!(out.shield_mitigation, 0.8);

        let c2 = combatant_with(0.0, 0.0, 0.9);
        let mut buffs2 = HashMap::new();
        buffs2.insert("shield_mitigation".to_string(), 0.5);
        let out2 = apply_static_buffs_to_combatant(c2, &buffs2);
        assert_eq!(out2.shield_mitigation, 1.0, "shield_mitigation should clamp to 1.0");
    }
}
