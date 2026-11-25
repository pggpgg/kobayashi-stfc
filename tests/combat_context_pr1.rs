//! End-to-end integration tests for PR 1: Combat Context Wiring & Live Bug Fixes.
//!
//! Covers:
//! - Additive Wave Defense context (`[RedMovingSpace, Waves]` and `[GroupArmadas, Waves]`)
//! - Multi-tag officer eligibility (Wave Defense + Armada / Hostile officers retained together)
//! - Solo Armada context + `OfficerStatConditionContext` multi-word key fix (`Dezoc`, `Jadzia Dax`)
//! - `resolve_enemy_type` / `infer_base_hostile_enemy_type` dead-branch fixes (Q-Trials, Outposts,
//!   Mission Bosses)
//! - Caleb Mir verified canonical gates (`EnemyNonArmadaHostileOrOutpost`, `EnemyOutpostOrRetaliator`)
//! - Newly mapped canonical conditions (`EnemySentinel`, `TargetNotPlayerStation`, `SelfAtAssault2`,
//!   `TargetIsInvadingEntity`, `TargetIsArmadaOrInvadingEntity`)
//! - Armada Control Center (`weapon_damage_vs_armada_hostile`) & Outpost (`weapon_damage_vs_outpost`)
//!   additive weapon-damage pool fix
//! - HTTP API validation (`enemy_type` HTTP 400 on unknown strings, `"auto"`/`""` accepted),
//!   `/api/optimize/replay-seed` & `/api/crew/counterfactual` context propagation, and
//!   `optimize_fingerprint::scenario_fingerprint` canonical equivalence + Wave Defense invalidation.

use kobayashi::combat::{Combatant, EnemyType, OpponentKind, ShipType};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::officer_eligibility::{
    is_eligible_for_optimization_for_tags, parse_optional_enemy_type, resolve_combat_context,
    seat_best_verdict_for_tags, EligibilityVerdict, Q_CONTINUUM_FACTION_ID,
};
use kobayashi::data::optimize_fingerprint::{scenario_fingerprint, ReuseScenarioInputs};
use kobayashi::data::profile::{
    apply_profile_to_attacker_with_health_pool_and_target_weapon_bonus, OfficerStatRuntimeBonus,
    PlayerProfile, StaticHealthPoolBonus,
};
use kobayashi::data::support_buffs::SupportBuffScenarioRequest;
use kobayashi::optimizer::crew_generator::{CrewCandidate, NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS};
use kobayashi::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry_and_context, DefenderOpponent,
    ScenarioEngagementOverride,
};
use kobayashi::server::api::{
    compare_crews_payload, crew_counterfactual_payload, fleet_allocate_payload,
    optimize_estimate_payload, parse_optimize_request_body, replay_optimize_seed_payload,
    sensitivity_payload, simulate_payload, validate_request,
};

const SHIP_ID: &str = "uss_enterprise_d";
const NORMAL_EXPLORER_HOSTILE: &str = "3752796"; // Federation Patrol (L35, explorer)
const QTRIAL_HOSTILE: &str = "1063917329"; // Borg Polygon 1.2 (ust=1, Q-Continuum)
const OUTPOST_HOSTILE: &str = "2970424496"; // Independent Solo Outpost (L51, ust=1, is_outpost=true)
const GROUP_ARMADA_HOSTILE: &str = "103246151"; // Romulan Admiral (ust=1, !is_outpost)
const BORG_ARMADA_HOSTILE: &str = "1051490285"; // MEGA CUBE TRACTOR BEAM (faction 2943562711)

fn replay_with_context(
    registry: &DataRegistry,
    hostile: &str,
    crew: &CrewCandidate,
    defender_opponent: DefenderOpponent,
    engagement_override: ScenarioEngagementOverride,
) -> kobayashi::optimizer::monte_carlo::MonteCarloSeedReplay {
    replay_optimize_iteration_with_registry_and_context(
        registry,
        SHIP_ID,
        hostile,
        Some(12),
        Some(60),
        crew,
        42,
        0,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        500,
        SupportBuffScenarioRequest::default(),
        defender_opponent,
        None,
        None,
        engagement_override,
    )
}

#[test]
fn resolve_combat_context_fixes_qtrial_outpost_and_mission_boss_dead_branches() {
    let registry = DataRegistry::load().expect("registry");

    // Q-Continuum hostile with upstream_ship_type == 1 must infer QTrial, NOT GroupArmadas.
    let qtrial = registry
        .resolve_hostile(QTRIAL_HOSTILE)
        .expect("Q-Trial hostile");
    assert_eq!(qtrial.upstream_ship_type, 1);
    assert_eq!(
        qtrial.faction.as_ref().map(|f| f.id),
        Some(Q_CONTINUUM_FACTION_ID)
    );
    let q_ctx = resolve_combat_context(None, false, false, Some(&qtrial), None);
    assert_eq!(q_ctx.primary_enemy_type, EnemyType::QTrial);
    assert_eq!(q_ctx.engagement_enemy_types.0, vec![EnemyType::QTrial]);
    assert_eq!(q_ctx.opponent_kind, OpponentKind::NpcHostile);

    // Outpost hostile with upstream_ship_type == 1 must infer OutpostArmadas, NOT GroupArmadas.
    let outpost = registry
        .resolve_hostile(OUTPOST_HOSTILE)
        .expect("Outpost hostile");
    assert_eq!(outpost.upstream_ship_type, 1);
    assert!(outpost.is_outpost);
    let o_ctx = resolve_combat_context(None, false, false, Some(&outpost), None);
    assert_eq!(o_ctx.primary_enemy_type, EnemyType::OutpostArmadas);
    assert!(o_ctx.defender_is_outpost);
    assert!(o_ctx.defender_is_armada_hostile);
    assert_eq!(o_ctx.combat_defender_ship_type, Some(ShipType::Armada));

    // Non-outpost, non-Q-Continuum upstream_ship_type == 1 still infers GroupArmadas.
    let armada = registry
        .resolve_hostile(GROUP_ARMADA_HOSTILE)
        .expect("Group armada hostile");
    let a_ctx = resolve_combat_context(None, false, false, Some(&armada), None);
    assert_eq!(a_ctx.primary_enemy_type, EnemyType::GroupArmadas);
    assert!(!a_ctx.defender_is_outpost);
    assert!(a_ctx.defender_is_armada_hostile);

    // Mission bosses (upstream_ship_type 3 or 6) infer MissionBosses.
    for ust in [3u32, 6u32] {
        let mut boss = armada.clone();
        boss.upstream_ship_type = ust;
        boss.is_outpost = false;
        boss.faction = None;
        boss.engagement_enemy_types = None;
        let b_ctx = resolve_combat_context(None, false, false, Some(&boss), None);
        assert_eq!(b_ctx.primary_enemy_type, EnemyType::MissionBosses);
    }

    // Overriding a normal hostile to SoloArmadas reconciles ShipType::Armada and armada flags.
    let normal = registry
        .resolve_hostile(NORMAL_EXPLORER_HOSTILE)
        .expect("Normal explorer hostile");
    let solo_ctx = resolve_combat_context(
        Some(EnemyType::SoloArmadas),
        false,
        false,
        Some(&normal),
        None,
    );
    assert_eq!(solo_ctx.primary_enemy_type, EnemyType::SoloArmadas);
    assert_eq!(solo_ctx.combat_defender_ship_type, Some(ShipType::Armada));
    assert!(solo_ctx.defender_is_armada_hostile);

    // Overriding an armada hostile to RedMovingSpace restores its underlying hull class.
    let non_armada_ctx = resolve_combat_context(
        Some(EnemyType::RedMovingSpace),
        false,
        false,
        Some(&armada),
        None,
    );
    assert_eq!(
        non_armada_ctx.combat_defender_ship_type,
        Some(armada.ship_type())
    );
    assert!(!non_armada_ctx.defender_is_armada_hostile);
}

#[test]
fn wave_defense_is_additive_in_combat_and_multi_tag_eligibility() {
    let registry = DataRegistry::load().expect("registry");

    // Academy Doctor (bridge): IsolyticCascadeDamage with [SelfAtWaveDefenseChallenge, EnemyHostile, EnemyExplorer]
    // Phlox (bridge): CritChance with [TargetNotPlayerStation, TargetNotArmada, TargetNotASB]
    let crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Academy Doctor".into(), "Phlox".into()],
        below_decks: vec!["Laliari".into()],
    };

    let without_wave = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride {
            enemy_type: None,
            wave_defense: false,
        },
    );
    let with_wave_flag = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride {
            enemy_type: None,
            wave_defense: true,
        },
    );
    let with_wave_enemy_type = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::Waves),
            wave_defense: false,
        },
    );

    // Wave Defense fires Academy Doctor's IsolyticCascadeDamage (producing isolytic damage > 0
    // where baseline has 0) while preserving non-armada hostile context!
    assert!(
        with_wave_flag.total_isolytic_damage > without_wave.total_isolytic_damage + 1e-6,
        "Wave Defense flag must activate Academy Doctor's Wave+Hostile+Explorer cascade (without={}, with={})",
        without_wave.total_isolytic_damage,
        with_wave_flag.total_isolytic_damage
    );
    // `enemy_type: Some(Waves)` and `wave_defense: true` on a normal hostile produce identical results.
    assert!(
        (with_wave_flag.total_damage - with_wave_enemy_type.total_damage).abs() < 1e-9
            && (with_wave_flag.total_isolytic_damage - with_wave_enemy_type.total_isolytic_damage)
                .abs()
                < 1e-9,
        "enemy_type=waves and wave_defense=true on normal hostile must match identically"
    );

    // Multi-tag eligibility: [GroupArmadas, Waves] retains BOTH Wave-only and Armada-only officers.
    let matrix = registry.eligibility_matrix().expect("eligibility matrix");
    let officer_index = registry.officer_index();
    let academy_doc = officer_index
        .get(&kobayashi::data::officer::normalize_officer_lookup_key(
            "Academy Doctor",
        ))
        .expect("Academy Doctor");
    let solo_or_wave = [EnemyType::GroupArmadas, EnemyType::Waves];
    assert!(
        is_eligible_for_optimization_for_tags(
            academy_doc,
            "officer",
            &solo_or_wave,
            Some(matrix),
            false
        ),
        "Wave Defense officer must remain eligible when [GroupArmadas, Waves] is active"
    );
    let verdict = seat_best_verdict_for_tags(matrix, academy_doc, "officer", &solo_or_wave);
    assert!(
        matches!(
            verdict,
            Some((EligibilityVerdict::Works | EligibilityVerdict::Conditional, _))
        ),
        "expected Works or Conditional for Academy Doctor in [GroupArmadas, Waves], got {verdict:?}"
    );
}

#[test]
fn solo_armada_context_fires_bashir_gossa_and_dezoc_officer_stat_all() {
    let registry = DataRegistry::load().expect("registry");

    // Julian Bashir (captain): CritDamage with SelfAtSoloArmada
    // Gossa (bridge): CritDamage with [SelfAtSoloArmada, EnemyHullFaction(2943562711)]
    // Dezoc (bridge): OfficerStatAll with [SelfAtSoloArmada, EnemyHullFaction(2943562711)]
    // Without the `enemy_type_to_key` fix, `OfficerStatConditionContext` produced `"soloarmadas"`
    // instead of `"solo_armadas"`, silently suppressing Dezoc's OfficerStatAll!
    let solo_crew = CrewCandidate {
        captain: "Julian Bashir".into(),
        bridge: vec!["Gossa".into(), "Cadet McCoy".into()],
        below_decks: vec!["Cadet Scotty".into()],
    };

    let group_replay = replay_with_context(
        &registry,
        BORG_ARMADA_HOSTILE,
        &solo_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::GroupArmadas),
            wave_defense: false,
        },
    );
    let solo_replay = replay_with_context(
        &registry,
        BORG_ARMADA_HOSTILE,
        &solo_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::SoloArmadas),
            wave_defense: false,
        },
    );

    assert!(
        solo_replay.total_damage > group_replay.total_damage + 1e-3,
        "Solo Armada context must activate Julian Bashir + Gossa (group={}, solo={})",
        group_replay.total_damage,
        solo_replay.total_damage
    );

    // Isolate Dezoc alone on Saladin T9 L45 (low officer_bonus breakpoints: 140, 280, 455, 630...)
    // vs L25 Borg Cube (1846271075, faction 2943562711, 128k DPR) so Saladin survives many rounds
    // and Dezoc's SelfAtSoloArmada + EnemyHullFaction(2943562711) OfficerStatAll crosses a
    // crew-stat breakpoint in solo_armadas vs group_armadas.
    let dezoc_only = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Dezoc".into(), "Cadet McCoy".into()],
        below_decks: vec!["Cadet Uhura".into(), "Cadet Scotty".into(), "Spock".into()],
    };
    let dezoc_group = replay_optimize_iteration_with_registry_and_context(
        &registry,
        "uss_saladin",
        "1846271075",
        Some(9),
        Some(45),
        &dezoc_only,
        42,
        0,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        500,
        SupportBuffScenarioRequest::default(),
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::GroupArmadas),
            wave_defense: false,
        },
    );
    let dezoc_solo = replay_optimize_iteration_with_registry_and_context(
        &registry,
        "uss_saladin",
        "1846271075",
        Some(9),
        Some(45),
        &dezoc_only,
        42,
        0,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        500,
        SupportBuffScenarioRequest::default(),
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::SoloArmadas),
            wave_defense: false,
        },
    );
    assert!(
        (dezoc_solo.total_damage - dezoc_group.total_damage).abs() > 1e-6
            || (dezoc_solo.attacker_hull_remaining - dezoc_group.attacker_hull_remaining).abs()
                > 1e-6,
        "Dezoc's SelfAtSoloArmada OfficerStatAll must measurably change combat stats in solo_armadas vs group_armadas (group_dmg={}, solo_dmg={}, group_hull={}, solo_hull={})",
        dezoc_group.total_damage,
        dezoc_solo.total_damage,
        dezoc_group.attacker_hull_remaining,
        dezoc_solo.attacker_hull_remaining,
    );
}

#[test]
fn caleb_mir_bridge_and_bda_gates_match_verified_upstream_behavior() {
    let registry = DataRegistry::load().expect("registry");

    // Caleb Mir BDA (1768055716): AllDamage with [EnemyOutpostOrRetaliator]
    // Must fire on OutpostArmadas and OutpostRetaliationAttackers, NOT on normal RedMovingSpace
    // or non-outpost GroupArmadas.
    let bda_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Cadet McCoy".into(), "Cadet Uhura".into()],
        below_decks: vec!["Caleb Mir".into()],
    };
    let control_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Cadet McCoy".into(), "Cadet Uhura".into()],
        below_decks: vec!["Cadet Scotty".into()],
    };

    let normal_caleb = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &bda_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride::default(),
    );
    let normal_ctrl = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &control_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride::default(),
    );
    let outpost_caleb = replay_with_context(
        &registry,
        OUTPOST_HOSTILE,
        &bda_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride::default(),
    );
    let outpost_ctrl = replay_with_context(
        &registry,
        OUTPOST_HOSTILE,
        &control_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride::default(),
    );
    let retaliator_caleb = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &bda_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::OutpostRetaliationAttackers),
            wave_defense: false,
        },
    );
    let retaliator_ctrl = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &control_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::OutpostRetaliationAttackers),
            wave_defense: false,
        },
    );

    // On Outpost & Retaliator, Caleb Mir BDA (+1000%..+2750% weapon damage!) massively boosts damage.
    assert!(
        outpost_caleb.total_damage > outpost_ctrl.total_damage * 1.5,
        "Caleb Mir BDA must fire strongly vs Outpost (caleb={}, ctrl={})",
        outpost_caleb.total_damage,
        outpost_ctrl.total_damage
    );
    assert!(
        retaliator_caleb.total_damage > retaliator_ctrl.total_damage * 1.5,
        "Caleb Mir BDA must fire strongly vs Outpost Retaliator (caleb={}, ctrl={})",
        retaliator_caleb.total_damage,
        retaliator_ctrl.total_damage
    );
    // On normal hostile, Caleb Mir BDA must NOT trigger huge +1000% damage.
    assert!(
        normal_caleb.total_damage < normal_ctrl.total_damage * 1.25,
        "Caleb Mir BDA must NOT fire vs normal RedMovingSpace hostile (caleb={}, ctrl={})",
        normal_caleb.total_damage,
        normal_ctrl.total_damage
    );

    // Caleb Mir Bridge (3851967764): ShieldHPRepair with [EnemyNonArmadaHostileOrOutpost]
    // Must fire on normal hostiles AND Outposts, NOT on regular GroupArmadas.
    // Use Saladin T9 L45 vs L25 Borg Cube (1846271075, 38.5M hull, 128k DPR) so shields break
    // across rounds and RoundStart shield regeneration measurably increases surviving hull.
    let bridge_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Caleb Mir".into(), "Cadet McCoy".into()],
        below_decks: vec![],
    };
    let run_borg_cube = |override_ctx: ScenarioEngagementOverride| {
        replay_optimize_iteration_with_registry_and_context(
            &registry,
            "uss_saladin",
            "1846271075",
            Some(9),
            Some(45),
            &bridge_crew,
            42,
            0,
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            500,
            SupportBuffScenarioRequest::default(),
            DefenderOpponent::Hostile,
            None,
            None,
            override_ctx,
        )
    };
    let on_normal = run_borg_cube(ScenarioEngagementOverride {
        enemy_type: Some(EnemyType::RedMovingSpace),
        wave_defense: false,
    });
    let on_outpost = run_borg_cube(ScenarioEngagementOverride {
        enemy_type: Some(EnemyType::OutpostArmadas),
        wave_defense: false,
    });
    let on_group_armada = run_borg_cube(ScenarioEngagementOverride {
        enemy_type: Some(EnemyType::GroupArmadas),
        wave_defense: false,
    });
    assert!(
        on_normal.total_damage > on_group_armada.total_damage + 1e-3
            || on_normal.attacker_hull_remaining > on_group_armada.attacker_hull_remaining + 1e-3,
        "Caleb Mir bridge shield repair must extend survival vs normal hostiles and NOT vs GroupArmadas (normal_dmg={}, normal_hull={}, normal_rounds={}, group_dmg={}, group_hull={}, group_rounds={})",
        on_normal.total_damage,
        on_normal.attacker_hull_remaining,
        on_normal.rounds_simulated,
        on_group_armada.total_damage,
        on_group_armada.attacker_hull_remaining,
        on_group_armada.rounds_simulated
    );
    assert!(
        on_outpost.total_damage > on_group_armada.total_damage + 1e-3
            || on_outpost.attacker_hull_remaining > on_group_armada.attacker_hull_remaining + 1e-3,
        "Caleb Mir bridge shield repair must extend survival vs OutpostArmadas and NOT vs GroupArmadas (outpost_dmg={}, outpost_hull={}, outpost_rounds={}, group_dmg={}, group_hull={}, group_rounds={})",
        on_outpost.total_damage,
        on_outpost.attacker_hull_remaining,
        on_outpost.rounds_simulated,
        on_group_armada.total_damage,
        on_group_armada.attacker_hull_remaining,
        on_group_armada.rounds_simulated
    );
}

#[test]
fn newly_mapped_pvp_station_invading_entity_and_assault_gates_fire_exclusively_in_mode() {
    let registry = DataRegistry::load().expect("registry");

    // 1. EnemySentinel: Joachim (officer AllDamage [TargetNotASB, EnemySentinel])
    let joachim_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Joachim".into(), "Cadet McCoy".into()],
        below_decks: vec![],
    };
    let joachim_station = replay_with_context(
        &registry,
        GROUP_ARMADA_HOSTILE,
        &joachim_crew,
        DefenderOpponent::Player,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::PvpStation),
            wave_defense: false,
        },
    );
    let joachim_space = replay_with_context(
        &registry,
        GROUP_ARMADA_HOSTILE,
        &joachim_crew,
        DefenderOpponent::Player,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::PvpSpace),
            wave_defense: false,
        },
    );
    assert!(
        joachim_station.total_damage > joachim_space.total_damage + 1e-3,
        "Joachim (EnemySentinel) must boost damage in pvp_station vs pvp_space (station={}, space={})",
        joachim_station.total_damage,
        joachim_space.total_damage
    );

    // 2. TargetNotPlayerStation: Phlox (officer CritChance [TargetNotPlayerStation, TargetNotArmada, TargetNotASB])
    // And Demarco (officer IsolyticCascadeDamage [TargetNotPlayerStation, TargetIsArmadaOrInvadingEntity, TargetNotASB])
    let demarco_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Demarco".into(), "Cadet McCoy".into()],
        below_decks: vec![],
    };
    let demarco_ie = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &demarco_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::InvadingEntities),
            wave_defense: false,
        },
    );
    let demarco_normal = replay_with_context(
        &registry,
        NORMAL_EXPLORER_HOSTILE,
        &demarco_crew,
        DefenderOpponent::Hostile,
        ScenarioEngagementOverride::default(),
    );
    let demarco_station = replay_with_context(
        &registry,
        GROUP_ARMADA_HOSTILE,
        &demarco_crew,
        DefenderOpponent::Player,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::PvpStation),
            wave_defense: false,
        },
    );
    assert!(
        demarco_ie.total_isolytic_damage > demarco_normal.total_isolytic_damage + 1e-6,
        "Demarco (TargetIsArmadaOrInvadingEntity) must fire in invading_entities (ie={}, normal={})",
        demarco_ie.total_isolytic_damage,
        demarco_normal.total_isolytic_damage
    );
    assert!(
        demarco_station.total_isolytic_damage < 1e-6,
        "Demarco (TargetNotPlayerStation) must NOT fire in pvp_station (got {})",
        demarco_station.total_isolytic_damage
    );

    // 3. TargetIsInvadingEntity: Masriad Vael (officer ApexBarrier)
    // Use Enterprise-D T1 L1 vs Romulan Sub-Commander L37 (1059069490) so attacker takes real
    // hull damage and survives, cleanly exposing Apex Barrier reduction in invading_entities.
    let masriad_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Masriad Vael".into(), "Cadet McCoy".into()],
        below_decks: vec![],
    };
    let masriad_ie = replay_optimize_iteration_with_registry_and_context(
        &registry,
        SHIP_ID,
        "1059069490",
        Some(5),
        Some(25),
        &masriad_crew,
        42,
        0,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        500,
        SupportBuffScenarioRequest::default(),
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::InvadingEntities),
            wave_defense: false,
        },
    );
    let masriad_normal = replay_optimize_iteration_with_registry_and_context(
        &registry,
        SHIP_ID,
        "1059069490",
        Some(5),
        Some(25),
        &masriad_crew,
        42,
        0,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        500,
        SupportBuffScenarioRequest::default(),
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::RedMovingSpace),
            wave_defense: false,
        },
    );
    assert!(
        masriad_ie.attacker_hull_remaining > masriad_normal.attacker_hull_remaining + 1e-3,
        "Masriad Vael (TargetIsInvadingEntity) must increase surviving hull via Apex Barrier in invading_entities (ie_hull={}, normal_hull={})",
        masriad_ie.attacker_hull_remaining,
        masriad_normal.attacker_hull_remaining
    );

    // 4. SelfAtAssault2: Kira Nerys (officer ShieldMitigation Sub on EnemyShip)
    // Reduces the *defender's* shield mitigation, sending more damage straight through shields
    // onto defender hull (lower defender_hull_remaining and higher defender_shield_remaining).
    let kira_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Kira Nerys".into(), "Cadet McCoy".into()],
        below_decks: vec![],
    };
    let kira_assault = replay_optimize_iteration_with_registry_and_context(
        &registry,
        SHIP_ID,
        BORG_ARMADA_HOSTILE,
        Some(12),
        Some(60),
        &kira_crew,
        42,
        0,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        500,
        SupportBuffScenarioRequest::default(),
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::Assaults),
            wave_defense: false,
        },
    );
    let kira_normal = replay_optimize_iteration_with_registry_and_context(
        &registry,
        SHIP_ID,
        BORG_ARMADA_HOSTILE,
        Some(12),
        Some(60),
        &kira_crew,
        42,
        0,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        500,
        SupportBuffScenarioRequest::default(),
        DefenderOpponent::Hostile,
        None,
        None,
        ScenarioEngagementOverride {
            enemy_type: Some(EnemyType::RedMovingSpace),
            wave_defense: false,
        },
    );
    assert!(
        kira_assault.defender_hull_remaining < kira_normal.defender_hull_remaining - 1e-3,
        "Kira Nerys (SelfAtAssault2) must reduce EnemyShip shield mitigation so defender takes more hull damage in assaults (assault_dhull={}, normal_dhull={})",
        kira_assault.defender_hull_remaining,
        kira_normal.defender_hull_remaining
    );
}

#[test]
fn armada_and_outpost_target_weapon_damage_bonus_increases_attacker_attack() {
    let mut profile = PlayerProfile::default();
    profile.bonuses.insert("weapon_damage".to_string(), 0.50);
    let base_attacker = Combatant {
        id: "test_ship".into(),
        attack: 1_000.0,
        hull_health: 10_000.0,
        shield_health: 5_000.0,
        ..Combatant::default()
    };

    let without_bonus = apply_profile_to_attacker_with_health_pool_and_target_weapon_bonus(
        base_attacker.clone(),
        &profile,
        None,
        OfficerStatRuntimeBonus::default(),
        StaticHealthPoolBonus::default(),
        0.0,
    );
    let with_armada_bonus = apply_profile_to_attacker_with_health_pool_and_target_weapon_bonus(
        base_attacker,
        &profile,
        None,
        OfficerStatRuntimeBonus::default(),
        StaticHealthPoolBonus::default(),
        0.40,
    );

    assert!(
        (without_bonus.attack - 1_500.0).abs() < 1e-6,
        "base weapon_damage=0.50 should yield 1500, got {}",
        without_bonus.attack
    );
    assert!(
        (with_armada_bonus.attack - 1_900.0).abs() < 1e-6,
        "weapon_damage=0.50 + target bonus 0.40 should yield 1900, got {}",
        with_armada_bonus.attack
    );
}

#[test]
fn api_endpoints_validate_enemy_type_and_propagate_wave_defense() {
    let registry = DataRegistry::load().expect("registry");

    // 1. parse_optional_enemy_type accepts None, "", "auto" and rejects typos.
    assert_eq!(parse_optional_enemy_type(None), Ok(None));
    assert_eq!(parse_optional_enemy_type(Some("")), Ok(None));
    assert_eq!(parse_optional_enemy_type(Some("auto")), Ok(None));
    assert_eq!(
        parse_optional_enemy_type(Some("solo_armadas")),
        Ok(Some(EnemyType::SoloArmadas))
    );
    assert!(parse_optional_enemy_type(Some("solo_armada")).is_err());

    // 2. Optimize request validation rejects unknown enemy_type with field="enemy_type".
    let bad_opt = parse_optimize_request_body(&format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","enemy_type":"solo_armada"}}"#
    ))
    .expect("json parse");
    let err = validate_request(&registry, &bad_opt, 100).unwrap_err();
    assert!(
        err.to_string().contains("enemy_type"),
        "expected enemy_type validation error, got {err}"
    );

    // "auto" is accepted by validate_request.
    let auto_opt = parse_optimize_request_body(&format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","enemy_type":"auto","wave_defense":true}}"#
    ))
    .expect("json parse");
    assert!(validate_request(&registry, &auto_opt, 100).is_ok());

    // 3. Simulate rejects unknown enemy_type.
    let bad_sim = format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","crew":{{"captain":"Cadet Kirk","bridge":["Cadet McCoy","Cadet Uhura"]}},"enemy_type":"not_a_mode"}}"#
    );
    assert!(simulate_payload(
        &registry,
        &bad_sim,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)
    )
    .is_err());

    // Simulate with wave_defense=true does NOT emit false does_not_work for Academy Doctor.
    let wave_sim = format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","ship_tier":5,"ship_level":25,"num_sims":10,"seed":1,"wave_defense":true,"crew":{{"captain":"Cadet Kirk","bridge":["Academy Doctor","Phlox"],"below_deck":["Laliari"]}}}}"#
    );
    let wave_sim_json: serde_json::Value = serde_json::from_str(
        &simulate_payload(
            &registry,
            &wave_sim,
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        )
        .expect("wave simulate ok"),
    )
    .expect("json");
    let notes = wave_sim_json["eligibility_notes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        !notes.iter().any(|n| {
            n["officer"].as_str() == Some("Academy Doctor")
                && n["verdict"].as_str() == Some("does_not_work")
        }),
        "Academy Doctor must not be flagged does_not_work when wave_defense=true: {notes:?}"
    );

    // 4. Replay-seed rejects unknown enemy_type and reflects wave_defense=true in combat result.
    let bad_replay = format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","sim_index":0,"enemy_type":"bogus","crew":{{"captain":"Cadet Kirk","bridge":["Academy Doctor","Phlox"]}}}}"#
    );
    assert!(replay_optimize_seed_payload(
        &registry,
        &bad_replay,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)
    )
    .is_err());

    let replay_no_wave: serde_json::Value = serde_json::from_str(
        &replay_optimize_seed_payload(
            &registry,
            &format!(
                r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","ship_tier":5,"ship_level":25,"seed":42,"sim_index":0,"wave_defense":false,"crew":{{"captain":"Cadet Kirk","bridge":["Academy Doctor","Phlox"]}}}}"#
            ),
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        )
        .expect("replay no wave"),
    )
    .expect("json");
    let replay_wave: serde_json::Value = serde_json::from_str(
        &replay_optimize_seed_payload(
            &registry,
            &format!(
                r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","ship_tier":5,"ship_level":25,"seed":42,"sim_index":0,"wave_defense":true,"crew":{{"captain":"Cadet Kirk","bridge":["Academy Doctor","Phlox"]}}}}"#
            ),
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        )
        .expect("replay wave"),
    )
    .expect("json");
    assert!(
        replay_wave["summary"]["total_isolytic_damage"]
            .as_f64()
            .unwrap_or(0.0)
            > replay_no_wave["summary"]["total_isolytic_damage"]
                .as_f64()
                .unwrap_or(0.0)
                + 1e-6,
        "/api/optimize/replay-seed must propagate wave_defense=true into combat"
    );

    // 5. Compare-crews, counterfactual, fleet, sensitivity, and optimize/estimate reject unknown enemy_type.
    let bad_compare = format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","enemy_type":"bogus","crews":[{{"captain":"Cadet Kirk","bridge":["Cadet McCoy","Cadet Uhura"]}},{{"captain":"Cadet Kirk","bridge":["Cadet McCoy","Phlox"]}}]}}"#
    );
    assert!(compare_crews_payload(
        &registry,
        &bad_compare,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)
    )
    .is_err());

    let bad_cf = format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","enemy_type":"bogus","crew":{{"captain":"Cadet Kirk","bridge":["Academy Doctor","Phlox"]}}}}"#
    );
    assert!(crew_counterfactual_payload(
        &registry,
        &bad_cf,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)
    )
    .is_err());

    let bad_fleet = format!(
        r#"{{"slots":[{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","enemy_type":"bogus"}},{{"ship":"uss_saladin","hostile":"{NORMAL_EXPLORER_HOSTILE}"}}]}}"#
    );
    assert!(fleet_allocate_payload(
        &registry,
        &bad_fleet,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)
    )
    .is_err());

    let bad_sens = format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","captain":"Cadet Kirk","bridge":["Cadet McCoy","Cadet Uhura"],"enemy_type":"bogus"}}"#
    );
    assert!(sensitivity_payload(
        &registry,
        &bad_sens,
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS)
    )
    .is_err());

    assert!(
        optimize_estimate_payload(
            &registry,
            &format!("/api/optimize/estimate?ship={SHIP_ID}&hostile={NORMAL_EXPLORER_HOSTILE}&enemy_type=bogus"),
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        )
        .is_err()
    );
    assert!(
        optimize_estimate_payload(
            &registry,
            &format!("/api/optimize/estimate?ship={SHIP_ID}&hostile={NORMAL_EXPLORER_HOSTILE}&wave_defense=true"),
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        )
        .is_ok()
    );
}

#[test]
fn scenario_fingerprint_is_canonical_across_auto_and_invalidates_on_wave_defense_and_pvp() {
    let registry = DataRegistry::load().expect("registry");

    let base = |enemy_type: Option<&'static str>, wave_defense: bool| ReuseScenarioInputs {
        ship: SHIP_ID,
        hostile: GROUP_ARMADA_HOSTILE,
        ship_tier: Some(5),
        ship_level: Some(25),
        below_decks_slots: 3,
        enemy_type,
        wave_defense,
        defender_opponent: "hostile",
        support_buffs: None,
        defender_support_buffs: None,
        defender_alliance_debuffs: None,
        defender_ship: None,
        defender_ship_tier: None,
        defender_ship_level: None,
        defender_profile_id: None,
    };

    // On a Group Armada hostile, None == Some("auto") == Some("group_armadas").
    let fp_none = scenario_fingerprint(&registry, &base(None, false));
    let fp_auto = scenario_fingerprint(&registry, &base(Some("auto"), false));
    let fp_explicit = scenario_fingerprint(&registry, &base(Some("group_armadas"), false));
    assert_eq!(fp_none, fp_auto);
    assert_eq!(fp_none, fp_explicit);

    // Toggling wave_defense invalidates the scenario fingerprint!
    let fp_wave = scenario_fingerprint(&registry, &base(None, true));
    assert_ne!(
        fp_none, fp_wave,
        "toggling wave_defense must invalidate scenario_fingerprint"
    );

    // Overriding to solo_armadas invalidates the scenario fingerprint!
    let fp_solo = scenario_fingerprint(&registry, &base(Some("solo_armadas"), false));
    assert_ne!(fp_none, fp_solo);

    // PvP: defender_ship present reconciles defender_opponent to "player" regardless of raw input.
    let pvp_raw_hostile = ReuseScenarioInputs {
        defender_ship: Some("uss_saladin"),
        defender_profile_id: Some("demo"),
        defender_opponent: "hostile",
        ..base(None, false)
    };
    let pvp_raw_player = ReuseScenarioInputs {
        defender_ship: Some("uss_saladin"),
        defender_profile_id: Some("demo"),
        defender_opponent: "player",
        ..base(None, false)
    };
    assert_eq!(
        scenario_fingerprint(&registry, &pvp_raw_hostile),
        scenario_fingerprint(&registry, &pvp_raw_player),
        "PvP scenario fingerprint must canonicalize defender_opponent when defender_ship is set"
    );
}
