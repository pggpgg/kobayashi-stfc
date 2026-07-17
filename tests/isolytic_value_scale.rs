//! Pinning tests for the isolytic value-scale conventions ground-truthed 2026-07-16
//! (COMBAT_FIDELITY_BACKLOG.md #13). One representative catalog id per scale family,
//! resolved from the real upstream hostile record through the shipped catalog:
//!
//! - `{0:0.#%}`-style percent placeholders render the upstream number ×100, so the upstream
//!   value is already an engine-unit fraction — never divided by 100, regardless of the
//!   upstream `value_is_percentage` flag ("Something To Prove": 68.1 → +6,810%).
//! - Multi-stat texts reuse `values[0].chance` as the second placeholder `{1}`
//!   ("Double Down": value = flat apex barrier, chance = isolytic defense fraction).
//! - Hardcoded-number texts pin `value_override` from the text (Isolytic Dampeners 1000%,
//!   Interdimensional Threat II 1500% + 20,000 apex barrier).
//! - Conditional self-debuffs ("Isolytic Defense is reduced by X when fighting a
//!   battleship") resolve to a negative seat gated on the attacker hull class.
//! - Programmable Matter: final-damage reduction (new engine hook), forced-zero player
//!   shield mitigation, round-1 hyperthermic decay, and the 1000% dampener defense.

use kobayashi::combat::{
    AbilityEffect, Combatant, CrewConfiguration, OpponentFactionTag, ShipType, SimulationConfig,
    TraceMode, WeaponStats,
};
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};
use kobayashi::data::loader::resolve_hostile;

fn crew_for(hostile_id: &str) -> CrewConfiguration {
    let rec = resolve_hostile(hostile_id).expect("hostile record");
    let catalog = hostile_ability_catalog_for_default_path();
    hostile_abilities_to_defender_crew(&rec.ability, catalog)
}

fn seat_values<F: Fn(&AbilityEffect) -> Option<f64>>(
    crew: &CrewConfiguration,
    ability_id: &str,
    pick: F,
) -> Vec<f64> {
    crew.seats
        .iter()
        .filter(|s| s.ability.name == ability_id)
        .filter_map(|s| pick(&s.ability.effect))
        .collect()
}

fn assert_single(values: &[f64], expected: f64, what: &str) {
    assert_eq!(
        values.len(),
        1,
        "expected exactly one {what} seat, got {values:?}"
    );
    assert!(
        (values[0] - expected).abs() < 1e-9,
        "{what}: expected {expected}, got {}",
        values[0]
    );
}

/// Something To Prove (417858229 on hostile 1001966809, L73): upstream value 68.1 /
/// chance 7.4, flag=true. Damage is a fraction (renders "6,810%") — the old convention
/// divided by 100. Apex shred comes from the `chance` field (placeholder {1}).
#[test]
fn something_to_prove_damage_fraction_and_shred_from_chance() {
    let crew = crew_for("1001966809");
    assert_single(
        &seat_values(&crew, "417858229", |e| match e {
            AbilityEffect::IsolyticDamageBonus(v) => Some(*v),
            _ => None,
        }),
        68.1,
        "isolytic damage",
    );
    assert_single(
        &seat_values(&crew, "417858229", |e| match e {
            AbilityEffect::ApexShredBonus(v) => Some(*v),
            _ => None,
        }),
        7.4,
        "apex shred",
    );
}

/// Double Down (1636824 on hostile 1666885627, L76): upstream value 36,250 / chance 16.25.
/// The old convention used the value slot (the flat apex barrier) as isolytic defense
/// (362.5 = +36,250%); the defense fraction lives in `chance` and the barrier in `value`,
/// plus the hardcoded "Critical Damage cannot fall below 100%" floor.
#[test]
fn double_down_defense_from_chance_barrier_from_value() {
    let crew = crew_for("1666885627");
    assert_single(
        &seat_values(&crew, "1636824", |e| match e {
            AbilityEffect::IsolyticDefenseBonus(v) => Some(*v),
            _ => None,
        }),
        16.25,
        "isolytic defense",
    );
    assert_single(
        &seat_values(&crew, "1636824", |e| match e {
            AbilityEffect::ApexBarrierBonus(v) => Some(*v),
            _ => None,
        }),
        36250.0,
        "apex barrier",
    );
    assert_single(
        &seat_values(&crew, "1636824", |e| match e {
            AbilityEffect::HostileCritDamageFloorBonus(v) => Some(*v),
            _ => None,
        }),
        1.0,
        "crit damage floor",
    );
}

/// Isolytic Simulator + Dampeners (1767142233 on hostile 109780618): the hardcoded
/// "increases its Isolytic Defense by 1000%" overrides the meaningless upstream value
/// (1 → old seat 0.01), and the "can only be damaged by Isolytic Damage" line becomes a
/// vulnerability seat plus the 8% round-1 hull-decay companion.
#[test]
fn isolytic_simulator_dampeners_bundle() {
    let crew = crew_for("109780618");
    assert_single(
        &seat_values(&crew, "1767142233", |e| match e {
            AbilityEffect::IsolyticDefenseBonus(v) => Some(*v),
            _ => None,
        }),
        10.0,
        "dampener defense",
    );
    assert!(
        crew.seats.iter().any(|s| s.ability.name == "1767142233"
            && matches!(
                s.ability.effect,
                AbilityEffect::HostileIsolyticVulnerability
            )),
        "expected an Isolytic Vulnerability seat from the Simulator line"
    );
    assert_single(
        &seat_values(&crew, "1767142233", |e| match e {
            AbilityEffect::HostileHyperthermicDecay { fraction } => Some(*fraction),
            _ => None,
        }),
        0.08,
        "hyperthermic decay",
    );
}

/// Apex Defense bundle (2466223538 on hostile 1145300067): the `{0:#}` placeholder is a
/// flat apex barrier (10,000) — the old catalog mapped it as isolytic defense
/// (+1,000,000%); the dampener defense is the hardcoded 1000%.
#[test]
fn apex_defense_bundle_splits_barrier_from_dampener_defense() {
    let crew = crew_for("1145300067");
    assert_single(
        &seat_values(&crew, "2466223538", |e| match e {
            AbilityEffect::IsolyticDefenseBonus(v) => Some(*v),
            _ => None,
        }),
        10.0,
        "dampener defense",
    );
    assert_single(
        &seat_values(&crew, "2466223538", |e| match e {
            AbilityEffect::ApexBarrierBonus(v) => Some(*v),
            _ => None,
        }),
        10000.0,
        "apex barrier",
    );
}

/// Programmable Matter (2936293636 on hostile 1121103437): `{0:#.#%}` = 0.5 is a 50%
/// final-damage reduction (not isolytic defense); the shield drain maps to forced-zero
/// player shield mitigation; dampener defense 1000%; 10% hyperthermic on round 1.
#[test]
fn programmable_matter_bundle_resolves_all_four_seats() {
    let crew = crew_for("1121103437");
    assert_single(
        &seat_values(&crew, "2936293636", |e| match e {
            AbilityEffect::IsolyticDefenseBonus(v) => Some(*v),
            _ => None,
        }),
        10.0,
        "dampener defense",
    );
    assert_single(
        &seat_values(&crew, "2936293636", |e| match e {
            AbilityEffect::HostileFinalDamageReduction { fraction } => Some(*fraction),
            _ => None,
        }),
        0.5,
        "final damage reduction",
    );
    assert!(
        crew.seats.iter().any(|s| s.ability.name == "2936293636"
            && matches!(
                s.ability.effect,
                AbilityEffect::HostileAttackerShieldMitigationZero
            )),
        "expected the shield-drain proxy seat"
    );
    assert_single(
        &seat_values(&crew, "2936293636", |e| match e {
            AbilityEffect::HostileHyperthermicDecay { fraction } => Some(*fraction),
            _ => None,
        }),
        0.1,
        "hyperthermic decay",
    );
}

/// Mutually Assured Destruction (3308120216 on hostile 1396602572): "if the player ship
/// is a battleship this hostile's Isolytic Defense is reduced by {0:#.#%}" → negative
/// seat (-0.3) gated on the attacker hull class. The old row was an unconditional +0.3.
#[test]
fn mutually_assured_destruction_is_negative_and_battleship_gated() {
    let crew = crew_for("1396602572");
    let seat = crew
        .seats
        .iter()
        .find(|s| s.ability.name == "3308120216")
        .expect("MAD seat");
    match seat.ability.effect {
        AbilityEffect::IsolyticDefenseBonus(v) => {
            assert!((v + 0.3).abs() < 1e-9, "expected -0.3, got {v}")
        }
        ref other => panic!("expected IsolyticDefenseBonus, got {other:?}"),
    }
    let cond = seat.ability.condition.as_ref().expect("hull-class gate");
    assert!(
        format!("{cond:?}").contains("AttackerShipTypeIs(Battleship)"),
        "expected AttackerShipTypeIs(Battleship), got {cond:?}"
    );
}

/// Elite Assassin Training (3172395625 on hostile 1107147565): single `{0:#.#%}`
/// placeholder with flag=false — fraction passthrough (1.0 = +100%), unchanged scale.
#[test]
fn elite_assassin_single_placeholder_passthrough() {
    let crew = crew_for("1107147565");
    assert_single(
        &seat_values(&crew, "3172395625", |e| match e {
            AbilityEffect::IsolyticDamageBonus(v) => Some(*v),
            _ => None,
        }),
        1.0,
        "isolytic damage",
    );
}

/// Interdimensional Threat II (133748097 on hostile 1121284456): hardcoded
/// "1500% and Apex Barrier by 20000" — override 15.0 plus the flat barrier companion.
#[test]
fn interdimensional_threat_hardcoded_overrides() {
    let crew = crew_for("1121284456");
    assert_single(
        &seat_values(&crew, "133748097", |e| match e {
            AbilityEffect::IsolyticDamageBonus(v) => Some(*v),
            _ => None,
        }),
        15.0,
        "isolytic damage",
    );
    assert_single(
        &seat_values(&crew, "133748097", |e| match e {
            AbilityEffect::ApexBarrierBonus(v) => Some(*v),
            _ => None,
        }),
        20000.0,
        "apex barrier",
    );
}

/// Replicated Honorguard Apex (1823160651 on hostile 1119847302): four stats, no numbers,
/// upstream value 0.01/flag=true — unattributable, routed to the review bucket (no seats).
#[test]
fn honorguard_multi_stat_routes_to_review_noop() {
    let crew = crew_for("1119847302");
    assert!(
        !crew.seats.iter().any(|s| s.ability.name == "1823160651"),
        "honorguard must not emit seats until its per-stat values are ground-truthed"
    );
}

/// Engine behavior: HostileFinalDamageReduction halves the player's outbound damage
/// (Programmable Matter 50%), on top of an otherwise unmitigated defender.
#[test]
fn final_damage_reduction_halves_outbound_damage() {
    fn bare(id: &str, hull: f64, attack: f64) -> Combatant {
        Combatant {
            id: id.into(),
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
                attack,
                shots: Some(1),
                ..Default::default()
            }],
            hostile_mitigation_params: None,
        }
    }
    let attacker = bare("att", 1_000_000.0, 10_000.0);
    let defender = bare("def", 1_000_000.0, 0.0);
    let cfg = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: Default::default(),
        defender_level: None,
        attacker_roster_officer_ids: Default::default(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        attacker_hyperthermic_decay_fraction: 0.0,
        emit_state_snapshots: false,
    };
    let run = |crew: &CrewConfiguration| {
        kobayashi::combat::simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &cfg,
            &CrewConfiguration { seats: vec![] },
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Explorer,
            true,
            false,
            crew,
        )
    };
    let baseline = run(&CrewConfiguration { seats: vec![] });
    // Reuse the real Programmable Matter seats, keeping only the final-damage reduction so
    // the comparison isolates the new hook.
    let full = crew_for("1121103437");
    let reduced_crew = CrewConfiguration {
        seats: full
            .seats
            .into_iter()
            .filter(|s| {
                matches!(
                    s.ability.effect,
                    AbilityEffect::HostileFinalDamageReduction { .. }
                )
            })
            .collect(),
    };
    assert_eq!(
        reduced_crew.seats.len(),
        1,
        "expected the PM reduction seat"
    );
    let reduced = run(&reduced_crew);
    let base_dmg = baseline.total_damage;
    let red_dmg = reduced.total_damage;
    assert!(base_dmg > 0.0, "baseline must deal damage");
    assert!(
        (red_dmg - base_dmg * 0.5).abs() < base_dmg * 1e-6,
        "expected 50% of baseline outbound damage (base {base_dmg}, reduced {red_dmg})"
    );
}
