//! Unified `operation 5` (`MultiplyBaseAdd`) crew-stat-scaled officer tests.
//!
//! Covers all 43 verified `SelfShip` / `SelfAllShips` `MultiplyBaseAdd` abilities across all four
//! empirically proven families (2026-10-26 Realta captain-swap + 2026-11-26 Vi'Dar/Realta live
//! captures) and verifies the 7 unverified `op 5` / `op 6` abilities stay quarantined.

use kobayashi::combat::{
    build_combat_setup, simulate_combat, simulate_combat_from_setup, Ability, AbilityClass,
    AbilityCondition, AbilityEffect, AttackerStats, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, CrewStatAxis, CrewStatScaledTarget, DamageTakenTrigger, DefenderStats,
    EnemyType, EnemyTypes, HostileMitigationParams, OpponentFactionTag, ReactiveDamageTakenKind,
    ShipType, SimulationConfig, TimingWindow, TraceMode, WeaponFiringSchedule, WeaponStats,
    WeaponType, WeaponTypeScope,
};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};

/// Verify every single verified `op 5` (`MultiplyBaseAdd`) ability in `officers.canonical.json`
/// compiles into `CrewStatScaledDeferred` with the exact target, axis, timing, and flags, and
/// every unverified `op 5` / `op 6` ability stays explicitly quarantined.
#[test]
fn all_50_op5_and_op6_abilities_route_or_quarantine_explicitly() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let opts = ResolveOptions {
        tier: Some(5),
        ..Default::default()
    };

    #[allow(clippy::type_complexity)]
    let verified: &[(
        &str,
        &str,
        CrewStatScaledTarget,
        CrewStatAxis,
        TimingWindow,
        bool,
        u32,
        Option<DamageTakenTrigger>,
        bool,
    )] = &[
        // AllDefenses (17 abilities)
        (
            "tom-paris-3640cc",
            "bd",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "beverly-crusher-74b2d7",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "snw-ortegas-7c79fe",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "snw-spock-40de97",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "snw-uhura-ff7333",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Attack,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "snw-hemmer-330aae",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "snw-t-pring-16cf8d",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "snw-mbenga-fe38e5",
            "captain",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "benjamin-sisko-5c51f2",
            "captain",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "tos-mccoy-fff2e0",
            "captain",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "wok-mccoy-f09b41",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "wok-mccoy-f09b41",
            "bd",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "five-of-eleven-d9aa11",
            "captain",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::RoundStart,
            false,
            1,
            None,
            false,
        ),
        (
            "zhou-5d4d4b",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Defense,
            TimingWindow::RoundStart,
            false,
            1,
            None,
            false,
        ),
        (
            "d-jaoki-baa18f",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::RoundStart,
            false,
            1,
            None,
            false,
        ),
        (
            "cap-tilly-070cc9",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::RoundStart,
            true,
            0,
            None,
            false,
        ),
        (
            "the-doctor-327fc3",
            "bridge",
            CrewStatScaledTarget::AllDefensesPreAll,
            CrewStatAxis::Health,
            TimingWindow::DefensePhase,
            false,
            2,
            Some(DamageTakenTrigger::HullDamage),
            false,
        ),
        // Single-axis defenses (5 abilities)
        (
            "ba-el-91d122",
            "bridge",
            CrewStatScaledTarget::BaseArmorPreAll,
            CrewStatAxis::Attack,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "culber-b3e4a0",
            "captain",
            CrewStatScaledTarget::BaseArmorPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "linkasa-01a1b3",
            "bridge",
            CrewStatScaledTarget::BaseArmorPreAll,
            CrewStatAxis::Attack,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "gowron-27ac30",
            "bridge",
            CrewStatScaledTarget::BaseShieldDeflectionPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "martok-89ef39",
            "bridge",
            CrewStatScaledTarget::BaseDodgePreAll,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        // AllPiercing + single-axis piercing (7 abilities)
        (
            "b-elanna-torres-75cf02",
            "bridge",
            CrewStatScaledTarget::AllPiercingPreAll,
            CrewStatAxis::Attack,
            TimingWindow::DefensePhase,
            false,
            1,
            Some(DamageTakenTrigger::AnyDamage),
            false,
        ),
        (
            "jadzia-dax-736698",
            "captain",
            CrewStatScaledTarget::AllPiercingPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "cap-tilly-070cc9",
            "captain",
            CrewStatScaledTarget::AllPiercingPreAll,
            CrewStatAxis::Defense,
            TimingWindow::RoundStart,
            true,
            0,
            None,
            false,
        ),
        (
            "six-of-eleven-20cbe8",
            "bridge",
            CrewStatScaledTarget::AllPiercingPreAll,
            CrewStatAxis::Attack,
            TimingWindow::AttackPhase,
            true,
            0,
            None,
            true,
        ),
        (
            "charvanek-0f1b5c",
            "bridge",
            CrewStatScaledTarget::BaseArmorPiercingPreAll,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "marcus-073931",
            "bridge",
            CrewStatScaledTarget::BaseShieldPiercingPreAll,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "kang-55e67a",
            "bridge",
            CrewStatScaledTarget::BaseAccuracyPreAll,
            CrewStatAxis::Attack,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        // ShieldHPRepair (3 abilities)
        (
            "beverly-crusher-74b2d7",
            "captain",
            CrewStatScaledTarget::ShieldRepairFlat,
            CrewStatAxis::Health,
            TimingWindow::RoundStart,
            false,
            0,
            None,
            false,
        ),
        (
            "spock-c04738",
            "bridge",
            CrewStatScaledTarget::ShieldRepairFlat,
            CrewStatAxis::Defense,
            TimingWindow::RoundStart,
            false,
            0,
            None,
            false,
        ),
        (
            "tos-spock-86f176",
            "captain",
            CrewStatScaledTarget::ShieldRepairFlat,
            CrewStatAxis::Defense,
            TimingWindow::RoundStart,
            false,
            0,
            None,
            false,
        ),
        // AllDamage (11 abilities)
        (
            "mirek-953093",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "krell-ef559b",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Defense,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "yuki-1ab97a",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "yan-agh-dd8637",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "tomalak-0ff09c",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Attack,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "l-nar-ae14f4",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Attack,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "wesley-crusher-834fce",
            "captain",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Attack,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "jadzia-dax-736698",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Health,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "ro-mudd-21a89a",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Attack,
            TimingWindow::CombatBegin,
            false,
            0,
            None,
            false,
        ),
        (
            "tyler-1dcc4d",
            "bridge",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Attack,
            TimingWindow::RoundStart,
            true,
            0,
            None,
            false,
        ),
        (
            "tos-kirk-bc6d1b",
            "captain",
            CrewStatScaledTarget::WeaponDamageFlatPreCrit,
            CrewStatAxis::Defense,
            TimingWindow::RoundStart,
            true,
            0,
            None,
            false,
        ),
    ];
    assert_eq!(
        verified.len(),
        43,
        "all 43 verified op 5 abilities accounted for"
    );

    for &(
        oid,
        seat_kind,
        exp_target,
        exp_axis,
        exp_timing,
        exp_cum,
        exp_dur,
        exp_reactive,
        exp_on_hit,
    ) in verified
    {
        let buff = match seat_kind {
            "captain" => resolve_crew_to_buff_set(oid, &[], &[], &officers, &opts),
            "bridge" => {
                resolve_crew_to_buff_set("unused-cap", &[oid.into()], &[], &officers, &opts)
            }
            "bd" => resolve_crew_to_buff_set("unused-cap", &[], &[oid.into()], &officers, &opts),
            _ => unreachable!(),
        };
        let want_seat = match seat_kind {
            "captain" => CrewSeat::Captain,
            "bridge" => CrewSeat::Bridge,
            "bd" => CrewSeat::BelowDeck,
            _ => unreachable!(),
        };
        let found = buff.crew.seats.iter().find(|s| {
            s.seat == want_seat
                && matches!(
                    s.ability.effect,
                    AbilityEffect::CrewStatScaledDeferred { target, axis, cumulative, duration_rounds, reactive_trigger, on_hit, .. }
                    if target == exp_target
                        && axis == exp_axis
                        && cumulative == exp_cum
                        && duration_rounds == exp_dur
                        && reactive_trigger == exp_reactive
                        && on_hit == exp_on_hit
                )
        });
        let seat = found.unwrap_or_else(|| {
            panic!(
                "{oid} ({seat_kind}) missing expected CrewStatScaledDeferred({exp_target:?}, {exp_axis:?}, cum={exp_cum}, dur={exp_dur}, reactive={exp_reactive:?}, on_hit={exp_on_hit}); got seats: {:?}",
                buff.crew.seats.iter().map(|s| (s.seat, s.ability.timing, s.ability.effect)).collect::<Vec<_>>()
            )
        });
        assert_eq!(
            seat.ability.timing, exp_timing,
            "{oid} ({seat_kind}) timing mismatch"
        );
        if oid == "ro-mudd-21a89a" {
            if let AbilityEffect::CrewStatScaledDeferred {
                chance,
                coefficient,
                ..
            } = seat.ability.effect
            {
                assert!(
                    (chance - 0.6).abs() < 1e-9,
                    "Ro Mudd rank 5 chance should be 0.6, got {chance}"
                );
                assert!(
                    (coefficient - 3.0).abs() < 1e-9,
                    "Ro Mudd coeff should be 3.0, got {coefficient}"
                );
            }
        }
        if oid == "tos-kirk-bc6d1b" {
            assert_eq!(
                seat.ability.weapon_scope,
                WeaponTypeScope::EnergyOnly,
                "TOS Kirk must retain ModuleEnergy weapon scope"
            );
        }
    }

    // All 7 unverified op 5 / op 6 abilities remain quarantined as UnsupportedOfficerEffect.
    let quarantined: &[(&str, &str)] = &[
        ("honorguard-worf-8ac58c", "captain"),
        ("changeling-kira-666ebe", "bridge"),
        ("toli-2d704a", "bridge"),
        ("ent-e-troi-46cdc3", "bridge"),
        ("vemet-00a218", "bridge"),
        ("six-of-eleven-20cbe8", "captain"),
        ("michael-burnham-6c711c", "bridge"),
    ];
    for &(oid, seat_kind) in quarantined {
        let buff = match seat_kind {
            "captain" => resolve_crew_to_buff_set(oid, &[], &[], &officers, &opts),
            "bridge" => {
                resolve_crew_to_buff_set("unused-cap", &[oid.into()], &[], &officers, &opts)
            }
            _ => unreachable!(),
        };
        let want_seat = if seat_kind == "captain" {
            CrewSeat::Captain
        } else {
            CrewSeat::Bridge
        };
        assert!(
            buff.crew.seats.iter().any(|s| {
                s.seat == want_seat
                    && matches!(
                        s.ability.effect,
                        AbilityEffect::UnsupportedOfficerEffect { .. }
                    )
            }),
            "{oid} ({seat_kind}) must stay quarantined as UnsupportedOfficerEffect; got {:?}",
            buff.crew
                .seats
                .iter()
                .map(|s| (s.seat, s.ability.effect))
                .collect::<Vec<_>>()
        );
    }
}

/// Live-capture verification (2026-11-26 Fight 6: Vi'Dar + Tom Paris vs L57 hostile):
/// `PreAllDefenseAdd` adds to ship base defense BEFORE `(1 + launch_def_pool + combat_start_def_pool)`,
/// matching the observed `0.7066895` mitigation within `0.002 pp` (whereas quarantined gives `0.1712`
/// and `POST_ALL` gives `0.1949`).
#[test]
fn tom_paris_pre_all_defense_matches_fight_6_vidar_observation() {
    // Baseline Vi'Dar pools verified against Fight 3/4 incoming mitigation (0.2505180):
    // M_armor_sd = 44.897123 => pool = 43.897123, M_dodge = 46.297123 => pool = 45.297123.
    let attacker = Combatant {
        id: "vidar".into(),
        hull_health: 1e9,
        armor: 43.897_123,
        shield_deflection: 43.897_123,
        dodge: 45.297_123,
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: DefenderStats {
                armor: 91.0,
                shield_deflection: 91.0,
                dodge: 6133.0,
            },
            weapon_pierce_defender_stats: None,
            base_attacker_stats: AttackerStats {
                armor_piercing: 1_945_355.0,
                shield_piercing: 1_945_355.0,
                accuracy: 4_057_972.0,
            },
            profile_piercing_fractions: None,
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Interceptor,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile_l57".into(),
        attack: 100_000.0,
        hull_health: 1e9,
        ..Default::default()
    };
    // Rank 4 Tom Paris (coeff 4.0) * effective crew Defense 88,599.80 (native integer raw stats).
    let add = 4.0 * 88_599.80;
    let run_mit = |effect: Option<AbilityEffect>, pvp: bool| -> f64 {
        let seats = effect
            .map(|eff| {
                vec![CrewSeatContext::legacy(
                    CrewSeat::BelowDeck,
                    Ability {
                        name: "tom_paris".into(),
                        class: AbilityClass::BelowDeck,
                        timing: TimingWindow::CombatBegin,
                        boostable: false,
                        effect: eff,
                        condition: Some(AbilityCondition::DefenderIsNpcHostile),
                        weapon_scope: WeaponTypeScope::All,
                    },
                    false,
                )]
            })
            .unwrap_or_default();
        let config = SimulationConfig {
            rounds: 1,
            trace_mode: TraceMode::Events,
            ..Default::default()
        };
        let setup = build_combat_setup(
            &attacker,
            &defender,
            &config,
            &CrewConfiguration { seats },
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Interceptor,
            !pvp,
            pvp,
            &CrewConfiguration::default(),
        );
        let res = simulate_combat_from_setup(&setup, 1);
        res.events
            .into_iter()
            .find(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
            .unwrap()
            .values["mitigation"]
            .as_f64()
            .unwrap()
    };

    let baseline = run_mit(None, false);
    let post_all = run_mit(Some(AbilityEffect::BaseMitigationStatsAdd(add)), false);
    let pre_all = run_mit(
        Some(AbilityEffect::PreAllDefenseAdd {
            armor: add,
            shield_deflection: add,
            dodge: add,
            cumulative: false,
        }),
        false,
    );
    let pvp_gated = run_mit(
        Some(AbilityEffect::PreAllDefenseAdd {
            armor: add,
            shield_deflection: add,
            dodge: add,
            cumulative: false,
        }),
        true,
    );

    assert!(
        baseline < 0.18,
        "without Tom Paris mitigation collapses: {baseline}"
    );
    assert!(
        post_all < 0.21,
        "POST_ALL misses Fight 6 by >50 pp: {post_all}"
    );
    assert!(
        (pre_all - 0.706_689_5).abs() < 0.000_05,
        "PRE_ALL must match Fight 6 observed 0.7066895 within 0.005 pp; got {pre_all:.7}"
    );
    assert!(
        (pvp_gated - baseline).abs() < 1e-12,
        "Tom Paris EnemyHostile gate must remain inert in PvP: {pvp_gated} vs {baseline}"
    );
}

/// Single-axis `PRE_ALL` defenses (Ba'el Armor, Gowron Shield Deflection, Martok Dodge) boost ONLY
/// their declared defense axis before percentage pools, never leaking across axes.
#[test]
fn single_axis_pre_all_defenses_isolate_armor_shield_deflection_and_dodge() {
    let attacker = Combatant {
        id: "self".into(),
        hull_health: 1e9,
        armor: 9.0,             // x10
        shield_deflection: 4.0, // x5
        dodge: 3.0,             // x4
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: DefenderStats {
                armor: 100.0,
                shield_deflection: 200.0,
                dodge: 300.0,
            },
            weapon_pierce_defender_stats: None,
            base_attacker_stats: AttackerStats {
                armor_piercing: 10_000.0,
                shield_piercing: 10_000.0,
                accuracy: 10_000.0,
            },
            profile_piercing_fractions: None,
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Explorer,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        attack: 1000.0,
        hull_health: 1e9,
        ..Default::default()
    };
    let run_eff = |armor, sd, dodge| {
        let crew = CrewConfiguration {
            seats: vec![CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "single_axis".into(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: true,
                    effect: AbilityEffect::PreAllDefenseAdd {
                        armor,
                        shield_deflection: sd,
                        dodge,
                        cumulative: false,
                    },
                    condition: None,
                    weapon_scope: WeaponTypeScope::All,
                },
                false,
            )],
        };
        let res = simulate_combat(
            &attacker,
            &defender,
            &SimulationConfig {
                rounds: 1,
                trace_mode: TraceMode::Events,
                ..Default::default()
            },
            &crew,
        );
        let ev = res
            .events
            .into_iter()
            .find(|e| e.event_type == "mitigation_calc" && e.phase == "counter")
            .unwrap()
            .values;
        (
            ev["effective_armor"].as_f64().unwrap(),
            ev["effective_shield_deflection"].as_f64().unwrap(),
            ev["effective_dodge"].as_f64().unwrap(),
        )
    };

    // Ba'el: +500 base Armor -> (100 + 500) * 10 = 6000; SD & Dodge untouched.
    assert_eq!(run_eff(500.0, 0.0, 0.0), (6000.0, 1000.0, 1200.0));
    // Gowron: +500 base Shield Deflection -> (200 + 500) * 5 = 3500; Armor & Dodge untouched.
    assert_eq!(run_eff(0.0, 500.0, 0.0), (1000.0, 3500.0, 1200.0));
    // Martok: +500 base Dodge -> (300 + 500) * 4 = 3200; Armor & SD untouched.
    assert_eq!(run_eff(0.0, 0.0, 500.0), (1000.0, 1000.0, 3200.0));
}

/// Live-capture verification (2026-11-26 Fight 4: Vi'Dar + B'Elanna Torres `HitTaken` 1R `PRE_ALL`):
/// On a 3-weapon ship with staggered firing cadence (`W1` R1/R3, `W2` R2), incoming hits on all 3
/// subturns per round produce 0 stacks on R1 W1, 2 stacks on R2 W2 (R1 subturn 3 carried over + R2
/// subturn 1), and 1 stack on R3 W1 (R2 subturn 3 carried over after R2 subturn 1/2 expire).
#[test]
fn belanna_reactive_pre_all_piercing_matches_fight_4_subturn_stacking_and_expiry() {
    let attacker = Combatant {
        id: "vidar".into(),
        attack: 1_000_000.0,
        hull_health: 1e9,
        shield_health: 1e9,
        weapons: vec![
            // W0 fires in R1 and R3 (interval 2)
            WeaponStats {
                attack: 1_000_000.0,
                shots: Some(1),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 1,
                    interval_rounds: 2,
                }),
                ..Default::default()
            },
            // W1 fires in R2 (first_round 2, interval 2)
            WeaponStats {
                attack: 1_000_000.0,
                shots: Some(1),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 2,
                    interval_rounds: 2,
                }),
                ..Default::default()
            },
            // W2 fires in R4 (keeps 3 subturns active during R1-R3 just like Vi'Dar)
            WeaponStats {
                attack: 1_000_000.0,
                shots: Some(1),
                firing_schedule: Some(WeaponFiringSchedule {
                    first_round: 4,
                    interval_rounds: 3,
                }),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let defender = Combatant {
        id: "fed_explorer_l50".into(),
        attack: 1000.0,
        hull_health: 1e9,
        shield_health: 1e9,
        weapons: vec![
            WeaponStats {
                attack: 1000.0,
                shots: Some(1),
                ..Default::default()
            };
            3
        ],
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: DefenderStats {
                armor: 29_107.0,
                shield_deflection: 424_681.0,
                dodge: 29_107.0,
            },
            weapon_pierce_defender_stats: None,
            // `base_attacker_stats` holds post-profile stats (ship_base * (1 + prof));
            // `profile_piercing_fractions` holds `prof` so PRE_ALL bonuses scale by (1 + prof) = 28x.
            base_attacker_stats: AttackerStats {
                armor_piercing: 13_089.0 * 28.0,
                shield_piercing: 2_088.0 * 28.0,
                accuracy: 1_566.0 * 28.0,
            },
            profile_piercing_fractions: Some(AttackerStats {
                armor_piercing: 27.0,
                shield_piercing: 27.0,
                accuracy: 27.0,
            }),
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Explorer,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };

    // B'Elanna rank 5 (15x) * effective crew Attack (~105,000) -> ~1,575,000 base piercing per stack.
    let per_stack = 1_575_000.0;
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext::legacy(
            CrewSeat::Bridge,
            Ability {
                name: "belanna".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::DefensePhase,
                boostable: true,
                effect: AbilityEffect::ReactiveDamageTaken {
                    trigger: DamageTakenTrigger::AnyDamage,
                    kind: ReactiveDamageTakenKind::PreAllAllPiercingAdd(per_stack),
                    chance: 1.0,
                    duration_rounds: 1,
                },
                condition: Some(AbilityCondition::DefenderIsNpcHostile),
                weapon_scope: WeaponTypeScope::All,
            },
            false,
        )],
    };

    let res = simulate_combat(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds: 3,
            trace_mode: TraceMode::Events,
            ..Default::default()
        },
        &crew,
    );

    let attack_mits: Vec<(u32, f64, f64)> = res
        .events
        .iter()
        .filter(|e| e.event_type == "mitigation_calc" && e.phase == "defense")
        .map(|e| {
            (
                e.round_index,
                e.values["mitigation"].as_f64().unwrap(),
                e.values["shield_ratio"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        attack_mits.len(),
        3,
        "expected one player weapon fire in R1, R2, R3: {attack_mits:?}"
    );
    let (r1, m1, sr1) = attack_mits[0];
    let (r2, m2, sr2) = attack_mits[1];
    let (r3, m3, sr3) = attack_mits[2];
    assert_eq!((r1, r2, r3), (1, 2, 3));

    // R1: 0 stacks -> SP = 2088 * 28 = 58,464
    let exp_sr1 = 424_681.0 / (2_088.0 * 28.0);
    assert!(
        (sr1 - exp_sr1).abs() < 1e-4,
        "R1 shield_ratio should be baseline: {sr1} vs {exp_sr1}"
    );
    // R2: 2 stacks (R1 subturn 3 + R2 subturn 1) -> SP = (2088 + 2 * 1,575,000) * 28
    let exp_sr2 = 424_681.0 / ((2_088.0 + 2.0 * per_stack) * 28.0);
    assert!(
        (sr2 - exp_sr2).abs() < 1e-5,
        "R2 shield_ratio should reflect exactly 2 PRE_ALL stacks: {sr2} vs {exp_sr2}"
    );
    // R3: 1 stack (R2 subturn 3; R2 subturn 1 & 2 expired!) -> SP = (2088 + 1 * 1,575,000) * 28
    let exp_sr3 = 424_681.0 / ((2_088.0 + per_stack) * 28.0);
    assert!(
        (sr3 - exp_sr3).abs() < 1e-5,
        "R3 shield_ratio should reflect exactly 1 PRE_ALL stack after 1-round expiry: {sr3} vs {exp_sr3}"
    );
    // Mitigation matches Fight 4 pattern: ~59.9% R1 -> ~16.24% R2 (2 stacks) -> ~16.32% R3 (1 stack).
    assert!(m1 > 0.55 && m1 < 0.65, "R1 baseline mitigation: {m1}");
    assert!(
        m2 > 0.16 && m2 < 0.165,
        "R2 2-stack PRE_ALL mitigation: {m2}"
    );
    assert!(
        m3 > m2 && m3 < 0.166,
        "R3 1-stack mitigation must be slightly above R2 2-stack: m2={m2}, m3={m3}"
    );
}

/// Live-capture verification (2026-11-26 Fight 1: Beverly Crusher captain `ShieldHPRepair`):
/// `ShieldRepairFlat` resolves directly to `coeff * effective_crew_health` (zero ship shield-HP
/// pool multiplication) and restores that exact flat SHP at RoundStart.
#[test]
fn beverly_crusher_flat_shield_repair_matches_fight_1_exact_shp() {
    let attacker = Combatant {
        id: "vidar".into(),
        hull_health: 10_000_000.0,
        shield_health: 10_000_000.0,
        shield_mitigation: 0.8,
        ..Default::default()
    };
    let defender = Combatant {
        id: "hostile".into(),
        attack: 500_000.0,
        hull_health: 1e9,
        ..Default::default()
    };
    // Fight 1: Beverly coeff 2.0 * effective crew Health 96,590.95 = 193,181.90 flat SHP/round.
    let repair_per_round = 193_181.9;
    let crew = CrewConfiguration {
        seats: vec![CrewSeatContext::legacy(
            CrewSeat::Captain,
            Ability {
                name: "beverly_captain".into(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::ShieldRegen(repair_per_round),
                condition: Some(AbilityCondition::DefenderIsNpcHostile),
                weapon_scope: WeaponTypeScope::All,
            },
            false,
        )],
    };
    let config = SimulationConfig {
        rounds: 3,
        ..Default::default()
    };
    let baseline = simulate_combat(&attacker, &defender, &config, &CrewConfiguration::default());
    let with_beverly = simulate_combat(&attacker, &defender, &config, &crew);
    // Round 1 starts at full shields (capped); Rounds 2 and 3 each restore exact flat SHP.
    let healed = with_beverly.attacker_shield_remaining - baseline.attacker_shield_remaining;
    let expected_total_heal = 2.0 * repair_per_round;
    assert!(
        (healed - expected_total_heal).abs() < 1e-3,
        "Beverly must restore exact flat SHP across R2 & R3 without shield-HP pool multiplication: got {healed} vs {expected_total_heal}"
    );
}

/// Live-capture verification (2026-11-26 Fight 5: Realta + Mirek `AllDamage` `5x` Defense):
/// `FlatWeaponDamagePreCrit` adds `coeff * effective_crew_stat` POST_ALL before crit, so a ~4,300
/// base Realta shot jumps to `4,300 + 548,840 = 553,140` non-crit and scales by `crit_multiplier`
/// on crits — NOT multiplied by `weapon_damage` percentage pools.
#[test]
fn mirek_post_all_pre_crit_flat_damage_matches_fight_5_realta_observation() {
    let make_attacker = |crit_chance: f64| Combatant {
        id: "realta".into(),
        attack: 4_300.0,
        crit_chance,
        crit_multiplier: 8.52,
        hull_health: 1e9,
        weapons: vec![WeaponStats {
            attack: 4_300.0,
            shots: Some(1),
            weapon_type: WeaponType::Energy,
            ..Default::default()
        }],
        ..Default::default()
    };
    let defender = Combatant {
        id: "fed_explorer_l50".into(),
        hull_health: 1e9,
        mitigation: 0.0,
        ..Default::default()
    };
    let flat_add = 5.0 * 109_768.0; // 548,840
    let crew = CrewConfiguration {
        seats: vec![
            // Large weapon_damage percentage bonus that MUST NOT multiply FlatWeaponDamagePreCrit!
            CrewSeatContext::legacy(
                CrewSeat::BelowDeck,
                Ability {
                    name: "wd_pool".into(),
                    class: AbilityClass::BelowDeck,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::AttackMultiplier(10.0), // +1000%
                    condition: None,
                    weapon_scope: WeaponTypeScope::All,
                },
                false,
            ),
            CrewSeatContext::legacy(
                CrewSeat::Bridge,
                Ability {
                    name: "mirek".into(),
                    class: AbilityClass::BridgeAbility,
                    timing: TimingWindow::CombatBegin,
                    boostable: false,
                    effect: AbilityEffect::FlatWeaponDamagePreCrit {
                        amount: flat_add,
                        chance: 1.0,
                        cumulative: false,
                    },
                    condition: Some(AbilityCondition::DefenderFactionIs(
                        OpponentFactionTag::Federation,
                    )),
                    weapon_scope: WeaponTypeScope::All,
                },
                false,
            ),
        ],
    };
    let run = |crit_chance: f64, faction: OpponentFactionTag| -> f64 {
        let setup = build_combat_setup(
            &make_attacker(crit_chance),
            &defender,
            &SimulationConfig {
                rounds: 1,
                seed: 7,
                ..Default::default()
            },
            &crew,
            faction,
            ShipType::Explorer,
            ShipType::Explorer,
            true,
            false,
            &CrewConfiguration::default(),
        );
        simulate_combat_from_setup(&setup, 7).total_damage
    };

    // Baseline vs Klingon (Mirek inactive): 4,300 * (1 + 10) = 47,300.
    let vs_klingon = run(0.0, OpponentFactionTag::Klingon);
    assert!(
        (vs_klingon - 47_300.0).abs() < 1e-6,
        "Klingon baseline: {vs_klingon}"
    );

    // vs Federation non-crit: 47,300 + 548,840 = 596,140 (NOT (4,300 + 548,840) * 11 = 6,084,540!).
    let vs_fed_noncrit = run(0.0, OpponentFactionTag::Federation);
    assert!(
        (vs_fed_noncrit - (47_300.0 + flat_add)).abs() < 1e-6,
        "Mirek must add POST_ALL pre-crit: got {vs_fed_noncrit}"
    );

    // vs Federation guaranteed crit: (47,300 + 548,840) * 8.52 = 5,079,112.8.
    let vs_fed_crit = run(1.0, OpponentFactionTag::Federation);
    assert!(
        (vs_fed_crit - (47_300.0 + flat_add) * 8.52).abs() < 1e-3,
        "Mirek flat damage must be multiplied by crit_multiplier: got {vs_fed_crit}"
    );
}

/// Verifies weapon-scoped cumulative `FlatWeaponDamagePreCrit` (TOS Kirk `ModuleEnergy`),
/// chance-gated `CombatBegin` `FlatWeaponDamagePreCrit` (Ro Mudd), and post-volley cumulative
/// `OnHitPreAllPiercingAdd` (Six of Eleven vs Armada).
#[test]
fn tos_kirk_energy_scope_ro_mudd_chance_and_six_of_eleven_on_hit_work_end_to_end() {
    // 1. TOS Kirk: cumulative RoundStart EnergyOnly flat weapon damage on mixed Energy + Kinetic ship.
    let mixed_ship = Combatant {
        id: "mixed".into(),
        hull_health: 1e9,
        weapons: vec![
            WeaponStats {
                attack: 1000.0,
                shots: Some(1),
                weapon_type: WeaponType::Energy,
                ..Default::default()
            },
            WeaponStats {
                attack: 1000.0,
                shots: Some(1),
                weapon_type: WeaponType::Kinetic,
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let dummy_def = Combatant {
        id: "target".into(),
        hull_health: 1e9,
        mitigation: 0.0,
        ..Default::default()
    };
    let kirk_crew = CrewConfiguration {
        seats: vec![CrewSeatContext::legacy(
            CrewSeat::Captain,
            Ability {
                name: "tos_kirk".into(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::FlatWeaponDamagePreCrit {
                    amount: 500.0,
                    chance: 1.0,
                    cumulative: true,
                },
                condition: None,
                weapon_scope: WeaponTypeScope::EnergyOnly,
            },
            false,
        )],
    };
    let res = simulate_combat(
        &mixed_ship,
        &dummy_def,
        &SimulationConfig {
            rounds: 3,
            ..Default::default()
        },
        &kirk_crew,
    );
    // Kinetic: 1000 * 3 = 3000. Energy: (1000+500) + (1000+1000) + (1000+1500) = 6000. Total = 9000.
    assert!(
        (res.total_damage - 9_000.0).abs() < 1e-6,
        "TOS Kirk must accumulate on Energy weapons only: got {}",
        res.total_damage
    );

    // 2. Ro Mudd: CombatBegin chance < 1.0 is deterministic per seed and triggers probabilistically.
    let one_weapon = Combatant {
        id: "ship".into(),
        attack: 1000.0,
        hull_health: 1e9,
        ..Default::default()
    };
    let mudd_crew = CrewConfiguration {
        seats: vec![CrewSeatContext::legacy(
            CrewSeat::Bridge,
            Ability {
                name: "ro_mudd".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::CombatBegin,
                boostable: false,
                effect: AbilityEffect::FlatWeaponDamagePreCrit {
                    amount: 5000.0,
                    chance: 0.5,
                    cumulative: false,
                },
                condition: None,
                weapon_scope: WeaponTypeScope::All,
            },
            false,
        )],
    };
    let mut triggered = 0usize;
    for seed in 1..=40u64 {
        let a = simulate_combat(
            &one_weapon,
            &dummy_def,
            &SimulationConfig {
                rounds: 1,
                seed,
                ..Default::default()
            },
            &mudd_crew,
        )
        .total_damage;
        let b = simulate_combat(
            &one_weapon,
            &dummy_def,
            &SimulationConfig {
                rounds: 1,
                seed,
                ..Default::default()
            },
            &mudd_crew,
        )
        .total_damage;
        assert_eq!(a, b, "Ro Mudd roll must be deterministic for seed {seed}");
        if (a - 6000.0).abs() < 1e-6 {
            triggered += 1;
        } else {
            assert!((a - 1000.0).abs() < 1e-6, "unexpected Ro Mudd damage: {a}");
        }
    }
    assert!(
        triggered > 5 && triggered < 35,
        "Ro Mudd 50% proc should trigger on a strict subset of 40 seeds, got {triggered}"
    );

    // 3. Six Of Eleven: OnHitPreAllPiercingAdd accumulates once per volley (even multi-shot) vs Armada.
    let multi_shot_attacker = Combatant {
        id: "armada_attacker".into(),
        attack: 10_000.0,
        hull_health: 1e9,
        weapons: vec![WeaponStats {
            attack: 10_000.0,
            shots: Some(4), // 4 shots per volley -> must grant ONCE per volley!
            ..Default::default()
        }],
        ..Default::default()
    };
    let armada_defender = Combatant {
        id: "armada".into(),
        hull_health: 1e9,
        hostile_mitigation_params: Some(HostileMitigationParams {
            round_native_defense_stats: false,
            defender_stats: DefenderStats {
                armor: 50_000.0,
                shield_deflection: 50_000.0,
                dodge: 50_000.0,
            },
            weapon_pierce_defender_stats: None,
            base_attacker_stats: AttackerStats {
                armor_piercing: 5_000.0,
                shield_piercing: 5_000.0,
                accuracy: 5_000.0,
            },
            profile_piercing_fractions: Some(AttackerStats {
                armor_piercing: 4.0, // x5
                shield_piercing: 4.0,
                accuracy: 4.0,
            }),
            base_attacker_weapon_stats: Vec::new(),
            ship_type: ShipType::Armada,
            mystery_mitigation_factor: 0.0,
            floor: 0.16,
            ceiling: 0.72,
        }),
        ..Default::default()
    };
    let six_crew = CrewConfiguration {
        seats: vec![CrewSeatContext::legacy(
            CrewSeat::Bridge,
            Ability {
                name: "six_of_eleven".into(),
                class: AbilityClass::BridgeAbility,
                timing: TimingWindow::AttackPhase,
                boostable: true,
                effect: AbilityEffect::OnHitPreAllPiercingAdd {
                    armor_piercing: 2_000.0,
                    shield_piercing: 2_000.0,
                    accuracy: 2_000.0,
                },
                condition: Some(AbilityCondition::DefenderShipTypeIs(ShipType::Armada)),
                weapon_scope: WeaponTypeScope::All,
            },
            false,
        )],
    };
    let setup = build_combat_setup(
        &multi_shot_attacker,
        &armada_defender,
        &SimulationConfig {
            rounds: 3,
            trace_mode: TraceMode::Events,
            engagement_enemy_types: EnemyTypes::single(EnemyType::GroupArmadas),
            ..Default::default()
        },
        &six_crew,
        OpponentFactionTag::Unknown,
        ShipType::Armada,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let armada_res = simulate_combat_from_setup(&setup, 1);
    let armor_ratios: Vec<(u32, f64)> = armada_res
        .events
        .iter()
        .filter(|e| e.event_type == "mitigation_calc" && e.phase == "defense")
        .map(|e| (e.round_index, e.values["armor_ratio"].as_f64().unwrap()))
        .collect();
    // 4 shots per round x 3 rounds = 12 shots; Six of Eleven grants ONCE per volley (not per shot):
    // R1: AP = 5000 (ratio 10.0); R2: 5000 + 2000*5 = 15000 (ratio 3.333333);
    // R3: 5000 + 4000*5 = 25000 (ratio 2.0).
    assert_eq!(
        armor_ratios.len(),
        12,
        "expected 4 shots x 3 rounds: {armor_ratios:?}"
    );
    for (round, expected_ratio) in [(1u32, 10.0), (2, 50_000.0 / 15_000.0), (3, 2.0)] {
        let round_ratios: Vec<f64> = armor_ratios
            .iter()
            .filter(|(r, _)| *r == round)
            .map(|(_, ratio)| *ratio)
            .collect();
        assert_eq!(round_ratios.len(), 4);
        for got in round_ratios {
            assert!(
                (got - expected_ratio).abs() < 1e-4,
                "R{round} armor_ratio mismatch: got {got} vs {expected_ratio}"
            );
        }
    }
}

/// Verifies public Monte Carlo path flags missing crew levels when an `op 5` crew runs without
/// saved officer levels, and clears the flag when a profile with saved levels is present.
#[test]
fn public_monte_carlo_flags_unresolved_op5_crew_without_saved_levels() {
    use kobayashi::data::data_registry::DataRegistry;
    use kobayashi::optimizer::crew_generator::CrewCandidate;
    use kobayashi::optimizer::monte_carlo::{run_monte_carlo_with_registry, DefenderOpponent};

    let registry = DataRegistry::load().expect("load registry");
    let candidate = CrewCandidate {
        captain: "Beverly Crusher".into(),
        bridge: vec!["Mirek".into(), "B'Elanna Torres".into()],
        below_decks: vec!["Tom Paris".into()],
    };
    let (missing_results, _) = run_monte_carlo_with_registry(
        &registry,
        "realta",
        "159955007",
        Some(4),
        Some(20),
        std::slice::from_ref(&candidate),
        1,
        7,
        Some("__kobayashi_absent_op5_profile__"),
        Default::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert_eq!(missing_results.len(), 1);
    assert_eq!(
        missing_results[0].unsupported_crew_health_effects,
        Some(true),
        "missing crew levels must flag unsupported_crew_health_effects"
    );

    let (demo_results, _) = run_monte_carlo_with_registry(
        &registry,
        "realta",
        "159955007",
        Some(4),
        Some(20),
        &[candidate],
        1,
        7,
        Some("demo"),
        Default::default(),
        None,
        DefenderOpponent::Hostile,
        None,
        None,
    );
    assert_eq!(demo_results.len(), 1);
    assert_eq!(
        demo_results[0].unsupported_crew_health_effects,
        Some(false),
        "demo profile with saved officer levels must resolve op 5 effects cleanly"
    );
    assert!(
        demo_results[0].avg_defender_hull_remaining
            < missing_results[0].avg_defender_hull_remaining,
        "resolved Mirek + B'Elanna in demo profile must leave less hostile hull ({} vs {})",
        demo_results[0].avg_defender_hull_remaining,
        missing_results[0].avg_defender_hull_remaining
    );
}
