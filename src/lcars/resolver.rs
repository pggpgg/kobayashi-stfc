//! Resolves parsed LCARS abilities into a [BuffSet] (static buffs + crew config for the engine).

use std::collections::HashMap;

use crate::combat::{
    Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat, CrewSeatContext,
    TimingWindow,
};
use crate::data::profile;
use crate::lcars::parser::{LcarsAbility, LcarsEffect, LcarsOfficer};

/// Options when resolving officer abilities (e.g. officer tier for scaling).
#[derive(Debug, Clone, Default)]
pub struct ResolveOptions {
    /// Officer tier (1-based). Used for scaling base + per_rank.
    pub tier: Option<u8>,
}

/// Resolved set of buffs: static modifiers (applied once) and dynamic crew config (per-round/triggered).
/// Per DESIGN.md: "LCARS definitions are collapsed into a BuffSet" before combat.
#[derive(Debug, Clone, Default)]
pub struct BuffSet {
    /// Stat modifiers applied once at combat start (e.g. passive permanent stat_modify).
    /// Keys are engine stat names (weapon_damage, shield_pierce, etc.); values are the resolved delta.
    pub static_buffs: HashMap<String, f64>,
    /// Per-round and triggered effects: the crew configuration the engine evaluates each round.
    pub crew: CrewConfiguration,
}

impl BuffSet {
    /// Convert this BuffSet into the crew configuration for the existing combat API.
    /// Static buffs are intended to be applied to ship/attacker stats before simulation;
    /// callers can do that in a follow-up. This returns the dynamic part.
    pub fn to_crew_config(&self) -> &CrewConfiguration {
        &self.crew
    }

    /// Apply this BuffSet's static_buffs to a Combatant (isolytic_damage, isolytic_defense, shield_mitigation).
    /// Call this when building a Combatant from ship/hostile + crew resolved via [resolve_crew_to_buff_set].
    pub fn apply_static_buffs_to_combatant(&self, combatant: Combatant) -> Combatant {
        profile::apply_static_buffs_to_combatant(combatant, &self.static_buffs)
    }
}

/// Map LCARS trigger string to engine timing window. Unknown triggers return None (effect skipped).
fn trigger_to_timing(trigger: Option<&str>) -> Option<TimingWindow> {
    match trigger.map(str::trim) {
        Some("passive") => Some(TimingWindow::CombatBegin),
        Some("on_combat_start") => Some(TimingWindow::CombatBegin),
        Some("on_round_start") => Some(TimingWindow::RoundStart),
        Some("on_attack") => Some(TimingWindow::AttackPhase),
        Some("on_hit") | Some("on_critical") => Some(TimingWindow::AttackPhase),
        Some("on_defense") => Some(TimingWindow::DefensePhase),
        Some("on_round_end") => Some(TimingWindow::RoundEnd),
        _ => None,
    }
}

/// True if this effect is passive and permanent (should go only into static_buffs, not crew).
fn is_static_effect(effect: &LcarsEffect) -> bool {
    let passive = effect.trigger.as_deref().map(str::trim) == Some("passive");
    let permanent = effect
        .duration
        .as_ref()
        .map(|d| d.is_permanent())
        .unwrap_or(false);
    passive && permanent && effect.effect_type == "stat_modify"
}

/// Resolve a single LCARS effect into (TimingWindow, AbilityEffect) if supported.
/// Unknown effect types or stats are skipped (graceful degradation); returns None.
/// Static effects (passive + permanent stat_modify) return None so they are only in static_buffs.
fn resolve_effect(effect: &LcarsEffect, _ability_name: &str, options: &ResolveOptions) -> Option<(TimingWindow, AbilityEffect)> {
    if is_static_effect(effect) {
        return None;
    }
    let tier = options.tier;
    let timing = trigger_to_timing(effect.trigger.as_deref())?;

    match effect.effect_type.as_str() {
        "stat_modify" => {
            let value = effect.value.or_else(|| effect.scaling.as_ref().map(|s| s.value_at_rank(tier)))?;
            let stat = effect.stat.as_deref().unwrap_or("");
            let op = effect.operator.as_deref().unwrap_or("add");

            // Map stat + operator to engine effect. Multiplicative damage -> AttackMultiplier; pierce -> PierceBonus.
            match stat {
                "weapon_damage" | "attack" => {
                    let mult = if op == "multiply" { value } else { 1.0 + value };
                    Some((timing, AbilityEffect::AttackMultiplier(mult)))
                }
                "shield_pierce" | "armor_pierce" => {
                    let add = if op == "multiply" { value - 1.0 } else { value };
                    Some((timing, AbilityEffect::PierceBonus(add)))
                }
                "crit_chance" | "crit_damage" => {
                    // Engine applies crit from ship; we could fold into static_buffs later.
                    Some((timing, AbilityEffect::AttackMultiplier(1.0 + value * 0.5)))
                }
                "apex_shred" => Some((timing, AbilityEffect::ApexShredBonus(value))),
                "apex_barrier" => Some((timing, AbilityEffect::ApexBarrierBonus(value))),
                "shield_regen" | "shield_hp_repair" => Some((timing, AbilityEffect::ShieldRegen(value))),
                "hull_repair" | "hull_hp_repair" => Some((timing, AbilityEffect::HullRegen(value))),
                "isolytic_damage" => {
                    let add = if op == "multiply" { value - 1.0 } else { value };
                    Some((timing, AbilityEffect::IsolyticDamageBonus(add)))
                }
                "isolytic_defense" => {
                    let add = if op == "multiply" { value - 1.0 } else { value };
                    Some((timing, AbilityEffect::IsolyticDefenseBonus(add)))
                }
                "shield_mitigation" => {
                    let add = if op == "multiply" { value - 1.0 } else { value };
                    Some((timing, AbilityEffect::ShieldMitigationBonus(add)))
                }
                _ => None,
            }
        }
        "extra_attack" => {
            let chance = effect.chance.or_else(|| effect.scaling.as_ref().map(|s| s.chance_at_rank(tier))).unwrap_or(0.0);
            let mult = effect.multiplier.unwrap_or(1.0);
            // Engine represents extra shot as proc_chance/proc_multiplier on Combatant; we emit attack multiplier
            // as a proxy for "extra damage this round" (simplified). Full extra_attack would need engine support.
            Some((timing, AbilityEffect::AttackMultiplier(1.0 + chance * (mult - 1.0))))
        }
        "tag" => None, // Non-combat; skip.
        _ => None,
    }
}

/// Resolve one officer ability block (captain, bridge, or below decks) into seat contexts.
pub fn resolve_officer_ability(
    _officer: &LcarsOfficer,
    ability: &LcarsAbility,
    seat: CrewSeat,
    class: AbilityClass,
    options: &ResolveOptions,
) -> Vec<CrewSeatContext> {
    let mut contexts = Vec::new();
    for effect in &ability.effects {
        if let Some((timing, effect_effect)) = resolve_effect(effect, &ability.name, options) {
            contexts.push(CrewSeatContext {
                seat,
                ability: Ability {
                    name: ability.name.clone(),
                    class,
                    timing,
                    boostable: true,
                    effect: effect_effect,
                },
                boosted: false,
            });
        }
    }
    contexts
}

/// Build a BuffSet for a crew: captain_id, bridge_ids, below_deck_ids.
/// Officers are looked up from the provided map (id -> LcarsOfficer).
/// Static buffs are accumulated from passive permanent stat_modify effects;
/// all resolved effects that have a timing go into crew.
pub fn resolve_crew_to_buff_set(
    captain_id: &str,
    bridge: &[String],
    below_decks: &[String],
    officers: &HashMap<String, LcarsOfficer>,
    options: &ResolveOptions,
) -> BuffSet {
    let mut static_buffs: HashMap<String, f64> = HashMap::new();
    let mut seats: Vec<CrewSeatContext> = Vec::new();

    let mut add_ability = |officer: &LcarsOfficer, ability: &LcarsAbility, seat: CrewSeat, class: AbilityClass| {
        for effect in &ability.effects {
            if effect.effect_type != "stat_modify"
                || effect.trigger.as_deref().map(str::trim) != Some("passive")
                || effect.duration.as_ref().map_or(false, |d| !d.is_permanent())
            {
                continue;
            }
            let value = effect.value.or_else(|| effect.scaling.as_ref().map(|s| s.value_at_rank(options.tier)));
            if let (Some(stat), Some(v)) = (effect.stat.as_deref(), value) {
                if effect.operator.as_deref() == Some("multiply") {
                    static_buffs
                        .entry(stat.to_string())
                        .and_modify(|x| *x *= v)
                        .or_insert(v);
                } else {
                    static_buffs
                        .entry(stat.to_string())
                        .and_modify(|x| *x += v)
                        .or_insert(v);
                }
            }
        }
        let contexts = resolve_officer_ability(officer, ability, seat, class, options);
        seats.extend(contexts);
    };

    if let Some(o) = officers.get(captain_id) {
        if let Some(ref a) = o.captain_ability {
            add_ability(o, a, CrewSeat::Captain, AbilityClass::CaptainManeuver);
        }
    }
    for id in bridge {
        if let Some(o) = officers.get(id.as_str()) {
            if let Some(ref a) = o.bridge_ability {
                add_ability(o, a, CrewSeat::Bridge, AbilityClass::BridgeAbility);
            }
        }
    }
    for id in below_decks {
        if let Some(o) = officers.get(id.as_str()) {
            if let Some(ref a) = o.below_decks_ability {
                add_ability(o, a, CrewSeat::BelowDeck, AbilityClass::BelowDeck);
            }
        }
    }

    BuffSet {
        static_buffs,
        crew: CrewConfiguration { seats },
    }
}

/// Build a map of officer id -> LcarsOfficer from a list (e.g. from load_lcars_dir).
pub fn index_lcars_officers_by_id(officers: Vec<LcarsOfficer>) -> HashMap<String, LcarsOfficer> {
    officers.into_iter().map(|o| (o.id.clone(), o)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::AbilityEffect;
    use crate::lcars::parser::{load_lcars_file, LcarsAbility, LcarsEffect, LcarsOfficer};
    use std::path::Path;

    fn lcars_effect_stat_modify(stat: &str, value: f64, trigger: &str) -> LcarsEffect {
        LcarsEffect {
            effect_type: "stat_modify".to_string(),
            stat: Some(stat.to_string()),
            target: None,
            operator: Some("add".to_string()),
            value: Some(value),
            trigger: Some(trigger.to_string()),
            duration: None,
            scaling: None,
            condition: None,
            chance: None,
            multiplier: None,
            tag: None,
        }
    }

    #[test]
    fn resolve_effect_maps_isolytic_and_shield_mitigation_to_ability_effects() {
        let officer = LcarsOfficer {
            id: "test".to_string(),
            name: "Test".to_string(),
            faction: None,
            rarity: None,
            group: None,
            captain_ability: None,
            bridge_ability: None,
            below_decks_ability: None,
        };
        let options = ResolveOptions { tier: Some(5) };
        let ability_iso = LcarsAbility {
            name: "iso".to_string(),
            effects: vec![lcars_effect_stat_modify("isolytic_damage", 0.15, "on_round_start")],
        };
        let contexts =
            resolve_officer_ability(&officer, &ability_iso, CrewSeat::Bridge, AbilityClass::BridgeAbility, &options);
        assert_eq!(contexts.len(), 1);
        assert!(matches!(contexts[0].ability.effect, AbilityEffect::IsolyticDamageBonus(v) if (v - 0.15).abs() < 1e-12));

        let ability_def = LcarsAbility {
            name: "def".to_string(),
            effects: vec![lcars_effect_stat_modify("isolytic_defense", 20.0, "on_round_start")],
        };
        let contexts_def =
            resolve_officer_ability(&officer, &ability_def, CrewSeat::Bridge, AbilityClass::BridgeAbility, &options);
        assert_eq!(contexts_def.len(), 1);
        assert!(matches!(contexts_def[0].ability.effect, AbilityEffect::IsolyticDefenseBonus(v) if (v - 20.0).abs() < 1e-12));

        let ability_shield = LcarsAbility {
            name: "shield".to_string(),
            effects: vec![lcars_effect_stat_modify("shield_mitigation", 0.05, "on_combat_start")],
        };
        let contexts_shield =
            resolve_officer_ability(&officer, &ability_shield, CrewSeat::Bridge, AbilityClass::BridgeAbility, &options);
        assert_eq!(contexts_shield.len(), 1);
        assert!(matches!(contexts_shield[0].ability.effect, AbilityEffect::ShieldMitigationBonus(v) if (v - 0.05).abs() < 1e-12));
    }

    #[test]
    fn resolve_khan_from_lcars_yaml() {
        let path = Path::new("data/officers/augments.lcars.yaml");
        if !path.exists() {
            return; // skip if data not present (e.g. in minimal checkouts)
        }
        let file = load_lcars_file(path).unwrap();
        let officers = index_lcars_officers_by_id(file.officers);
        let options = ResolveOptions { tier: Some(5) };
        let buff_set = resolve_crew_to_buff_set(
            "khan",
            &["khan".to_string()],
            &["khan".to_string()],
            &officers,
            &options,
        );
        // Khan's captain/bridge/below are all passive permanent -> static_buffs only; no dynamic seats.
        assert!(
            buff_set.static_buffs.contains_key("shield_pierce"),
            "expected static shield_pierce from captain ability"
        );
        assert!(
            buff_set.static_buffs.contains_key("weapon_damage"),
            "expected static weapon_damage from bridge ability"
        );
        assert!(
            buff_set.static_buffs.contains_key("hull_hp"),
            "expected static hull_hp from below decks ability"
        );
    }
}
