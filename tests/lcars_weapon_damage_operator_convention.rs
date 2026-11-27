//! Dynamic (non-static) LCARS `weapon_damage` rows must land in the engine's additive
//! pre-attack modifier with LCARS operator folding: `multiply v` ⇒ `+(v − 1)`, `add v` ⇒ `+v`,
//! `sub v` ⇒ `−v`. Before the 2026-07-10 fix, `compile_officer_combat_spec_impl` emitted the
//! full multiplicative factor (`multiply 1.2` → `AttackMultiplier(1.2)`), which the accumulator
//! then ADDED as a modifier — so a ×1.2 round-start buff dealt ×2.2 damage and a −20% debuff
//! (`sub 0.2` → 0.8) dealt ×1.8.

use kobayashi::combat::abilities::{AbilityClass, CrewSeat};
use kobayashi::combat::{
    build_combat_setup, simulate_combat_from_setup, Combatant, CrewConfiguration,
    OpponentFactionTag, ShipType, SimulationConfig, TraceMode, WeaponStats,
};
use kobayashi::lcars::{
    resolve_officer_ability, LcarsAbility, LcarsEffect, LcarsOfficer, ResolveOptions,
};

fn weapon_damage_officer(operator: &str, value: f64) -> LcarsOfficer {
    LcarsOfficer {
        id: "wd-conv-officer".to_string(),
        name: "Weapon Damage Convention Officer".to_string(),
        faction: None,
        rarity: None,
        group: None,
        native_synergy: None,
        captain_ability: None,
        bridge_ability: Some(LcarsAbility {
            name: "wd_conv".to_string(),
            effects: vec![LcarsEffect {
                effect_type: "stat_modify".to_string(),
                stat: Some("weapon_damage".to_string()),
                target: None,
                operator: Some(operator.to_string()),
                value: Some(value),
                trigger: Some("on_round_start".to_string()),
                duration: None,
                scaling: None,
                condition: None,
                chance: None,
                multiplier: None,
                tag: None,
                accumulate: None,
                decay: None,
            }],
        }),
        below_decks_ability: None,
        stats: Vec::new(),
        max_level_by_rank: Vec::new(),
    }
}

fn crew_for(operator: &str, value: f64) -> CrewConfiguration {
    let officer = weapon_damage_officer(operator, value);
    let ability = officer.bridge_ability.clone().expect("bridge ability");
    let seats = resolve_officer_ability(
        &officer,
        &ability,
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        &ResolveOptions::default(),
        0,
    );
    assert_eq!(seats.len(), 1, "expected exactly one dynamic seat");
    CrewConfiguration { seats }
}

fn attacker() -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 800.0,
        hull_health: 50_000.0,
        crit_multiplier: 1.0,
        proc_multiplier: 1.0,
        weapons: vec![WeaponStats {
            attack: 2_500.0,
            shots: None,
            ..Default::default()
        }],
        ..Combatant::default()
    }
}

fn defender() -> Combatant {
    Combatant {
        id: "def".into(),
        hull_health: 500_000.0,
        ..attacker()
    }
}

/// One-round outbound damage with the officer, divided by the no-crew baseline.
fn damage_ratio(operator: &str, value: f64) -> f64 {
    let attacker = attacker();
    let defender = defender();
    let config = SimulationConfig {
        rounds: 1,
        seed: 42,
        trace_mode: TraceMode::Off,
        ..Default::default()
    };
    let with_setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &crew_for(operator, value),
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let baseline_setup = build_combat_setup(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let with = simulate_combat_from_setup(&with_setup, config.seed);
    let without = simulate_combat_from_setup(&baseline_setup, config.seed);
    assert!(without.total_damage > 0.0, "baseline dealt no damage");
    with.total_damage / without.total_damage
}

#[test]
fn multiply_operator_scales_damage_by_the_stated_factor() {
    let ratio = damage_ratio("multiply", 1.2);
    assert!(
        (ratio - 1.2).abs() < 1e-6,
        "multiply 1.2 must deal ×1.2 damage, got ×{ratio}"
    );
}

#[test]
fn add_operator_adds_the_stated_bonus_fraction() {
    let ratio = damage_ratio("add", 0.2);
    assert!(
        (ratio - 1.2).abs() < 1e-6,
        "add 0.2 must deal ×1.2 damage, got ×{ratio}"
    );
}

#[test]
fn sub_operator_reduces_damage_by_the_stated_fraction() {
    let ratio = damage_ratio("sub", 0.2);
    assert!(
        (ratio - 0.8).abs() < 1e-6,
        "sub 0.2 must deal ×0.8 damage, got ×{ratio}"
    );
}

#[test]
fn snw_james_kirk_and_alldamage_multiply_add_officers_join_base_weapon_damage_pool() {
    use kobayashi::combat::AbilityEffect;
    use kobayashi::lcars::{
        build_officer_model_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    };

    let officers = index_lcars_officers_by_id(build_officer_model_default().unwrap());
    let resolve = |bridge: &[String], below: &[String], tier: u8| {
        resolve_crew_to_buff_set(
            "",
            bridge,
            below,
            &officers,
            &ResolveOptions {
                tier: Some(tier),
                ..Default::default()
            },
        )
        .crew
    };

    // SNW James Kirk tier 2 (+600% = +6.0) must compile to BaseWeaponDamageBonus(6.0),
    // NOT multiplicative AttackMultiplier(6.0).
    let kirk_crew = resolve(&["snw-james-kirk-6f6300".into()], &[], 2);
    assert!(
        kirk_crew
            .seats
            .iter()
            .any(|s| matches!(s.ability.effect, AbilityEffect::BaseWeaponDamageBonus(v) if (v - 6.0).abs() < 1e-9)),
        "expected BaseWeaponDamageBonus(6.0) for SNW James Kirk tier 2, got {:?}",
        kirk_crew.seats
    );

    // Other high-magnitude MultiplyAdd AllDamage officers across CombatStart and
    // ShieldsDepleted timings must also join BaseWeaponDamageBonus.
    for (bridge, below, tier, expected) in [
        (vec!["mudd-32546a".into()], vec![], 3, 3.0),
        (vec!["mirror-ezri-2f6326".into()], vec![], 5, 35.0),
    ] {
        let c = resolve(&bridge, &below, tier);
        assert!(
            c.seats.iter().any(
                |s| matches!(s.ability.effect, AbilityEffect::BaseWeaponDamageBonus(v) if (v - expected).abs() < 1e-9)
            ),
            "expected BaseWeaponDamageBonus({expected}) for bridge={bridge:?} below={below:?}, got {:?}",
            c.seats
        );
    }

    // Unconditional ShipLaunched MultiplyAdd AllDamage officers (Beckett Mariner, SNW D'Chok)
    // must land in static_buffs["weapon_damage_additive_pool"] (NOT multiplicative "weapon_damage")
    // so scenario construction merges them into the single `1 + MULT` pool alongside profile +
    // officer Attack + dynamic BaseWeaponDamageBonus.
    for (below_id, tier, expected) in [
        ("beckett-mariner-d93865", 5, 1.8),
        ("snw-d-chok-34f7ba", 4, 21.0),
    ] {
        let buffs = resolve_crew_to_buff_set(
            "",
            &[],
            &[below_id.to_string()],
            &officers,
            &ResolveOptions {
                tier: Some(tier),
                ..Default::default()
            },
        );
        let pool = buffs
            .static_buffs
            .get("weapon_damage_additive_pool")
            .copied()
            .unwrap_or(0.0);
        assert!(
            (pool - expected).abs() < 1e-9,
            "expected weapon_damage_additive_pool={expected} for {below_id} tier {tier}, got {:?}",
            buffs.static_buffs
        );
        assert!(
            !buffs.static_buffs.contains_key("weapon_damage"),
            "{below_id} must not land in multiplicative static_buffs[\"weapon_damage\"]: {:?}",
            buffs.static_buffs
        );
    }

    // Combining ShipLaunched (SNW D'Chok +21.0) + CombatStart (SNW James Kirk +6.0) on top of
    // a profile weapon-damage pool (p = 50.0) must fold both into the single 1 + MULT pool:
    // (1 + 50 + 21 + 6) / (1 + 50) = 78/51, NOT ×22 × 7 = ×154.
    {
        use kobayashi::data::profile::{
            apply_profile_to_attacker_with_health_pool_and_target_weapon_bonus,
            take_static_weapon_damage_pool_bonus, OfficerStatRuntimeBonus, PlayerProfile,
            StaticHealthPoolBonus,
        };
        let mut buffs = resolve_crew_to_buff_set(
            "",
            &["snw-james-kirk-6f6300".into()],
            &["snw-d-chok-34f7ba".into()],
            &officers,
            &ResolveOptions {
                officer_tiers: Some(
                    [
                        ("snw-james-kirk-6f6300".to_string(), 2),
                        ("snw-d-chok-34f7ba".to_string(), 4),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..Default::default()
            },
        );
        let static_wd_pool = take_static_weapon_damage_pool_bonus(&mut buffs.static_buffs);
        assert!((static_wd_pool - 21.0).abs() < 1e-9);

        let mut profile = PlayerProfile::default();
        profile.bonuses.insert("weapon_damage".to_string(), 50.0);
        let att_base = apply_profile_to_attacker_with_health_pool_and_target_weapon_bonus(
            attacker(),
            &profile,
            None,
            OfficerStatRuntimeBonus::default(),
            StaticHealthPoolBonus::default(),
            0.0,
        );
        let att_combo = apply_profile_to_attacker_with_health_pool_and_target_weapon_bonus(
            attacker(),
            &profile,
            None,
            OfficerStatRuntimeBonus::default(),
            StaticHealthPoolBonus::default(),
            static_wd_pool,
        );
        let def = defender();
        let base_cfg = SimulationConfig {
            rounds: 1,
            seed: 42,
            trace_mode: TraceMode::Off,
            profile_weapon_damage_fraction: 50.0,
            ..Default::default()
        };
        let combo_cfg = SimulationConfig {
            profile_weapon_damage_fraction: 50.0 + static_wd_pool,
            ..base_cfg.clone()
        };
        let base_setup = build_combat_setup(
            &att_base,
            &def,
            &base_cfg,
            &CrewConfiguration::default(),
            OpponentFactionTag::Unknown,
            ShipType::Interceptor,
            ShipType::Explorer,
            true,
            false,
            &CrewConfiguration::default(),
        );
        let combo_setup = build_combat_setup(
            &att_combo,
            &def,
            &combo_cfg,
            &buffs.crew,
            OpponentFactionTag::Unknown,
            ShipType::Interceptor,
            ShipType::Explorer,
            true,
            false,
            &CrewConfiguration::default(),
        );
        let base_dmg = simulate_combat_from_setup(&base_setup, base_cfg.seed).total_damage;
        let combo_dmg = simulate_combat_from_setup(&combo_setup, combo_cfg.seed).total_damage;
        let ratio = combo_dmg / base_dmg;
        let expected_ratio = 78.0 / 51.0;
        assert!(
            (ratio - expected_ratio).abs() < 1e-6,
            "SNW D'Chok (+21) + SNW James Kirk (+6) on p=50 pool: expected {expected_ratio:.6}, got {ratio:.6}"
        );
    }

    // With a realistic profile weapon-damage pool (p = 50.0, i.e., 1 + p = 51x base),
    // SNW James Kirk's +6.0 adds into the pool: (1 + 50 + 6) / (1 + 50) = 57/51 ≈ 1.1176x,
    // NOT ×7.0. With p = 0.0 (bare ship), it scales by 7.0x.
    let att = attacker();
    let def = defender();
    for (p_prof, expected_ratio) in [(0.0, 7.0), (50.0, 57.0 / 51.0)] {
        let cfg = SimulationConfig {
            rounds: 1,
            seed: 42,
            trace_mode: TraceMode::Off,
            profile_weapon_damage_fraction: p_prof,
            ..Default::default()
        };
        let with_setup = build_combat_setup(
            &att,
            &def,
            &cfg,
            &kirk_crew,
            OpponentFactionTag::Unknown,
            ShipType::Interceptor,
            ShipType::Explorer,
            true,
            false,
            &CrewConfiguration::default(),
        );
        let base_setup = build_combat_setup(
            &att,
            &def,
            &cfg,
            &CrewConfiguration::default(),
            OpponentFactionTag::Unknown,
            ShipType::Interceptor,
            ShipType::Explorer,
            true,
            false,
            &CrewConfiguration::default(),
        );
        let with = simulate_combat_from_setup(&with_setup, cfg.seed).total_damage;
        let base = simulate_combat_from_setup(&base_setup, cfg.seed).total_damage;
        let ratio = with / base;
        assert!(
            (ratio - expected_ratio).abs() < 1e-6,
            "p_prof={p_prof}: expected ratio {expected_ratio:.6}, got {ratio:.6}"
        );
    }
}
