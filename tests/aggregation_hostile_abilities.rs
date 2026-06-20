//! Aggregation hostile ability family: hyperthermic decay, mitigation inflation, offense bundles.

use kobayashi::combat::abilities::AbilityEffect;
use kobayashi::combat::types::{OpponentFactionTag, ShipType};
use kobayashi::combat::{
    hostile_defender_mitigation_additive_factor_from_defender_crew,
    hostile_hyperthermic_decay_fraction_from_defender_crew, mitigation_for_hostile,
    simulate_combat_with_defender_faction_and_defender_crew, AttackerStats, Combatant,
    CrewConfiguration, MITIGATION_CEILING, MITIGATION_FLOOR, SimulationConfig, TraceMode,
    WeaponStats,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
    HostileAbilityCatalog,
};
use kobayashi::data::loader::resolve_hostile;
use kobayashi::data::profile::PlayerProfile;

const AGGREGATION_HOSTILE_ID: &str = "260810365";

fn pve_config(seed: u64, hyperthermic: f64) -> SimulationConfig {
    SimulationConfig {
        rounds: 1,
        seed,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: Some(56),
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        attacker_hyperthermic_decay_fraction: hyperthermic,
        emit_state_snapshots: false,
    }
}

fn sim(
    attacker: &Combatant,
    defender: &Combatant,
    config: &SimulationConfig,
    defender_crew: &CrewConfiguration,
) -> kobayashi::combat::SimulationResult {
    simulate_combat_with_defender_faction_and_defender_crew(
        attacker,
        defender,
        config,
        &CrewConfiguration { seats: vec![] },
        OpponentFactionTag::Unknown,
        ShipType::Survey,
        ShipType::Survey,
        true,
        false,
        defender_crew,
    )
}

fn weak_attacker(hull: f64) -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 0.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: hull,
        shield_health: 0.0,
        shield_mitigation: 0.0,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 0.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn aggregation_defender(rec: &kobayashi::data::hostile::HostileRecord) -> Combatant {
    Combatant {
        id: AGGREGATION_HOSTILE_ID.into(),
        attack: 1.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: rec.hull_health,
        shield_health: rec.shield_health,
        shield_mitigation: rec.shield_mitigation.unwrap_or(0.8),
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 1.0,
            shots: Some(1),
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

#[test]
fn aggregation_hostile_260810365_is_tagged_and_resolves_catalog_seats() {
    let rec = resolve_hostile(AGGREGATION_HOSTILE_ID).expect("aggregation hostile");
    assert!(rec.is_aggregation_hostile(), "expected aggregation faction tag");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::HostileHyperthermicDecay { fraction } if (fraction - 0.5).abs() < 1e-9
            )
        }),
        "expected 50% hyperthermic decay seat (loca 82103 tier)"
    );
    assert!(
        (hostile_defender_mitigation_additive_factor_from_defender_crew(&crew) - 32.0).abs()
            < 1e-9,
        "expected +3200% mitigation inflation (+32 additive factor)"
    );
    assert!(
        crew.seats.iter().any(|s| {
            matches!(
                s.ability.effect,
                AbilityEffect::ProcAttackMultiplier { multiplier, .. } if multiplier >= 60.0
            )
        }),
        "expected offense weapon-damage proc seat"
    );
    assert!(
        crew.seats.iter().any(|s| {
            matches!(s.ability.effect, AbilityEffect::IsolyticDamageBonus(v) if v >= 5.0)
        }),
        "expected offense isolytic seat"
    );
}

#[test]
fn hyperthermic_decay_melts_half_hull_at_round_start_before_weapons() {
    let rec = resolve_hostile(AGGREGATION_HOSTILE_ID).expect("aggregation hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    let decay = hostile_hyperthermic_decay_fraction_from_defender_crew(&crew);
    assert!((decay - 0.5).abs() < 1e-9);

    let attacker = weak_attacker(100_000.0);
    let defender = aggregation_defender(&rec);
    let empty = CrewConfiguration { seats: vec![] };

    let with_decay = sim(&attacker, &defender, &pve_config(42, decay), &empty);
    let baseline = sim(&attacker, &defender, &pve_config(42, 0.0), &empty);

    assert!(
        (with_decay.attacker_hull_remaining - 50_000.0).abs() < 500.0,
        "expected ~50% hull after decay, got {}",
        with_decay.attacker_hull_remaining
    );
    assert!(
        baseline.attacker_hull_remaining > with_decay.attacker_hull_remaining + 40_000.0,
        "decay should remove substantially more hull than noop config"
    );
}

#[test]
fn mitigation_inflation_increases_effective_mitigation_scalar() {
    let rec = resolve_hostile(AGGREGATION_HOSTILE_ID).expect("aggregation hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    let mit_factor = hostile_defender_mitigation_additive_factor_from_defender_crew(&crew);
    assert!(mit_factor > 0.0);

    let base_stats = rec.to_defender_stats();
    let scaled_stats = kobayashi::combat::DefenderStats {
        armor: base_stats.armor * (1.0 + mit_factor),
        shield_deflection: base_stats.shield_deflection * (1.0 + mit_factor),
        dodge: base_stats.dodge * (1.0 + mit_factor),
    };
    let atk = AttackerStats {
        armor_piercing: 100_000.0,
        shield_piercing: 100_000.0,
        accuracy: 100_000.0,
    };
    let mit_base = mitigation_for_hostile(
        base_stats,
        atk,
        ShipType::Survey,
        0.0,
        MITIGATION_FLOOR,
        MITIGATION_CEILING,
    );
    let mit_scaled = mitigation_for_hostile(
        scaled_stats,
        atk,
        ShipType::Survey,
        0.0,
        MITIGATION_FLOOR,
        MITIGATION_CEILING,
    );
    assert!(
        mit_scaled > mit_base + 0.05,
        "scaled mitigation should exceed base (base={mit_base}, scaled={mit_scaled})"
    );
}

#[test]
fn recon_locus_stabilizer_reduces_net_hyperthermic_decay() {
    let rec = resolve_hostile(AGGREGATION_HOSTILE_ID).expect("aggregation hostile");
    let catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, catalog);
    let raw = hostile_hyperthermic_decay_fraction_from_defender_crew(&crew);
    assert!((raw - 0.5).abs() < 1e-9);

    let mut profile = PlayerProfile::default();
    profile
        .bonuses
        .insert("hyperthermic_stabilizer_vs_aggregation_hostile".into(), 0.15);
    let net = (raw
        - profile
            .bonuses
            .get("hyperthermic_stabilizer_vs_aggregation_hostile")
            .copied()
            .unwrap_or(0.0))
    .max(0.0);
    assert!((net - 0.35).abs() < 1e-9);

    let attacker = weak_attacker(100_000.0);
    let defender = aggregation_defender(&rec);
    let empty = CrewConfiguration { seats: vec![] };

    let full = sim(&attacker, &defender, &pve_config(7, raw), &empty);
    let reduced = sim(&attacker, &defender, &pve_config(7, net), &empty);

    assert!(
        reduced.attacker_hull_remaining > full.attacker_hull_remaining + 10_000.0,
        "stabilizer should leave more hull (full={}, reduced={})",
        full.attacker_hull_remaining,
        reduced.attacker_hull_remaining
    );
}

#[test]
fn offense_bundle_increases_counter_damage_vs_empty_catalog() {
    let rec = resolve_hostile(AGGREGATION_HOSTILE_ID).expect("aggregation hostile");
    let full_catalog = hostile_ability_catalog_for_default_path();
    let crew = hostile_abilities_to_defender_crew(&rec.ability, full_catalog);
    let empty_catalog = HostileAbilityCatalog {
        description: Some("empty".into()),
        entries: Default::default(),
    };
    let noop_crew = hostile_abilities_to_defender_crew(&rec.ability, Some(&empty_catalog));

    let attacker = weak_attacker(200_000.0);
    let mut defender = aggregation_defender(&rec);
    defender.attack = 10_000.0;
    defender.weapons = vec![WeaponStats {
        attack: 10_000.0,
        shots: Some(1),
        ..Default::default()
    }];

    let with_offense = sim(&attacker, &defender, &pve_config(99, 0.0), &crew);
    let baseline = sim(&attacker, &defender, &pve_config(99, 0.0), &noop_crew);

    let dmg_offense = 200_000.0 - with_offense.attacker_hull_remaining;
    let dmg_baseline = 200_000.0 - baseline.attacker_hull_remaining;
    assert!(
        dmg_offense > dmg_baseline * 1.5,
        "offense bundle should increase counter damage (offense={dmg_offense}, baseline={dmg_baseline})"
    );
}
