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
