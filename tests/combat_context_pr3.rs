//! End-to-end integration tests for PR 3:
//! - Verified `CombatBattleType` derivation (Red/Waves=2, MissionBoss/Q's Trial=3,
//!   GroupArmada/InvadingEntity=7, SoloArmada=8, OutpostArmada=17, OutpostRetaliator=18,
//!   Node PvP=4, Open/Station/Takeover PvP=1, Assaults=None)
//! - TMP Sulu / TMP Uhura / V'Ger Ilia (`[17, 18, 2, 5, 6, 9]`) and Data / Worf / Tasha Yar
//!   (`[2, 3, 5, 6, 7, 8, 9]` & Tasha captain `[3]`) combat gating
//! - Tactical `CombatGameContext` (`open_space`, `station`, `capture_node`, `mining_node`,
//!   `takeover`) and `SelfDefending` / `SelfAttacking` / `SelfAtStation` gating (Emp. Georgiou,
//!   Airiam, Dixon Hill, K'Bisch, TOS Chekov, Zahra, Mitchell, 718.0)
//! - Symmetric PvP defender perspective (Carol, Darwin, Lursa, Mitchell on `pvp_station`)
//! - PvP `defender_profile_id` roster isolation (`roster.imported.json` loaded per defender profile)
//! - API validation (`game_context` HTTP 400), `resolved_context` echo, and `scenario_fingerprint`
//!   invalidation across tactical context flags.

use std::path::PathBuf;
use std::sync::OnceLock;

use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, CombatGameContext, Combatant,
    CrewConfiguration, EnemyType, OpponentFactionTag, ShipType, SimulationConfig,
};
use kobayashi::data::data_registry::DataRegistry;
use kobayashi::data::officer_eligibility::{
    parse_optional_game_context, resolve_combat_context_with_tactical,
};
use kobayashi::data::optimize_fingerprint::{scenario_fingerprint, ReuseScenarioInputs};
use kobayashi::data::profile_index::profile_data_dir;
use kobayashi::data::support_buffs::SupportBuffScenarioRequest;
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    ResolveOptions,
};
use kobayashi::optimizer::crew_generator::{CrewCandidate, NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS};
use kobayashi::optimizer::monte_carlo::{
    replay_optimize_iteration_with_registry_and_context, DefenderOpponent,
    ScenarioEngagementOverride,
};
use kobayashi::server::api::{
    compare_crews_payload, crew_counterfactual_payload, parse_optimize_request_body,
    replay_optimize_seed_payload, run_optimize, simulate_payload, validate_request,
    OptimizeRequest,
};

const SHIP_ID: &str = "uss_enterprise_d";
const NORMAL_EXPLORER_HOSTILE: &str = "3752796"; // Federation Patrol (L35, explorer)
const QTRIAL_HOSTILE: &str = "1063917329"; // Borg Polygon 1.2 (Q-Continuum)
const OUTPOST_HOSTILE: &str = "2970424496"; // Independent Solo Outpost
const RETALIATOR_HOSTILE: &str = "1028117720"; // Galor-Class Warship (Outpost Retaliator)
const GROUP_ARMADA_HOSTILE: &str = "103246151"; // Romulan Admiral
const SOLO_ARMADA_HOSTILE: &str = "1081891879"; // Jem'Hadar Flagship
const DURABLE_HOSTILE: &str = "1059069490"; // Romulan Sub-Commander L37 (survives multi-round)

fn shared_registry() -> &'static DataRegistry {
    static REG: OnceLock<std::sync::Arc<DataRegistry>> = OnceLock::new();
    REG.get_or_init(|| DataRegistry::load().expect("DataRegistry::load"))
}

struct ScratchProfile {
    id: String,
}

impl ScratchProfile {
    fn new(suffix: &str) -> Self {
        let id = format!("__kobayashi_test_pr3_{}_{}__", suffix, std::process::id());
        let dir = profile_data_dir(&id);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch profile dir");
        Self { id }
    }

    fn dir(&self) -> PathBuf {
        profile_data_dir(&self.id)
    }

    fn write_file(&self, name: &str, contents: &str) {
        std::fs::write(self.dir().join(name), contents).expect("write scratch profile file");
    }
}

impl Drop for ScratchProfile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.dir());
    }
}

#[test]
fn combat_battle_type_derivation_matches_verified_modes_and_leaves_assaults_unknown() {
    let reg = shared_registry();
    let normal = reg.resolve_hostile(NORMAL_EXPLORER_HOSTILE).unwrap();
    let qtrial = reg.resolve_hostile(QTRIAL_HOSTILE).unwrap();
    let outpost = reg.resolve_hostile(OUTPOST_HOSTILE).unwrap();
    let retal = reg.resolve_hostile(RETALIATOR_HOSTILE).unwrap();
    let group_armada = reg.resolve_hostile(GROUP_ARMADA_HOSTILE).unwrap();
    let solo_armada = reg.resolve_hostile(SOLO_ARMADA_HOSTILE).unwrap();

    // PvE modes
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            false,
            Some(&normal),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(2)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            true,
            false,
            Some(&normal),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(2)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            Some(EnemyType::MissionBosses),
            false,
            false,
            Some(&normal),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(3)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            false,
            Some(&qtrial),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(3)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            false,
            Some(&group_armada),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(7)
    );
    // Additive wave_defense on a Group Armada preserves battle_type 7 (Armada precedence over Waves)
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            true,
            false,
            Some(&group_armada),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(7)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            Some(EnemyType::InvadingEntities),
            false,
            false,
            Some(&normal),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(7)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            false,
            Some(&solo_armada),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(8)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            false,
            Some(&outpost),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(17)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            false,
            Some(&retal),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(18)
    );

    // PvP modes + game_context interaction
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            true,
            None,
            Some(ShipType::Interceptor),
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        Some(1)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            true,
            None,
            Some(ShipType::Interceptor),
            CombatGameContext::CaptureNode,
            false,
            false
        )
        .combat_battle_type_id,
        Some(4)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            true,
            None,
            Some(ShipType::Interceptor),
            CombatGameContext::MiningNode,
            false,
            false
        )
        .combat_battle_type_id,
        Some(4)
    );
    assert_eq!(
        resolve_combat_context_with_tactical(
            None,
            false,
            true,
            None,
            Some(ShipType::Interceptor),
            CombatGameContext::Takeover,
            false,
            false
        )
        .combat_battle_type_id,
        Some(1)
    );

    // pvp_station keeps battle_type 1; explicit Station context sets attacker_at_station=true
    let station_raid_ctx = resolve_combat_context_with_tactical(
        Some(EnemyType::PvpStation),
        false,
        true,
        None,
        Some(ShipType::Interceptor),
        CombatGameContext::OpenSpace,
        false,
        false,
    );
    assert_eq!(station_raid_ctx.combat_battle_type_id, Some(1));
    assert!(!station_raid_ctx.attacker_at_station);
    let at_station_ctx = resolve_combat_context_with_tactical(
        Some(EnemyType::PvpSpace),
        false,
        true,
        None,
        Some(ShipType::Interceptor),
        CombatGameContext::Station,
        false,
        false,
    );
    assert_eq!(at_station_ctx.combat_battle_type_id, Some(1));
    assert!(at_station_ctx.attacker_at_station);

    // Assaults remains unverified -> None (fails closed with diagnostic)
    assert_eq!(
        resolve_combat_context_with_tactical(
            Some(EnemyType::Assaults),
            false,
            false,
            Some(&normal),
            None,
            CombatGameContext::OpenSpace,
            false,
            false
        )
        .combat_battle_type_id,
        None
    );
}

#[test]
fn tmp_sulu_uhura_ilia_activate_on_red_outpost_retaliator_and_stay_inert_on_armada_qtrial_pvp() {
    let reg = shared_registry();

    // TMP Hikaru Sulu (cap: ApexBarrier [17,18,2,5,6,9], max_level=81)
    // TMP Nyota Uhura (bda: IsolyticCascadeDamage [17,18,2,5,6,9])
    // V'Ger Ilia (bda: CritDamage [17,18,2,5,6,9])
    let tmp_crew = CrewCandidate {
        captain: "TMP Hikaru Sulu".into(),
        bridge: vec!["Cadet Kirk".into(), "Cadet McCoy".into()],
        below_decks: vec!["TMP Nyota Uhura".into(), "V'Ger Ilia".into()],
    };

    let run = |enemy_type: EnemyType, opp: DefenderOpponent| {
        replay_optimize_iteration_with_registry_and_context(
            reg,
            SHIP_ID,
            DURABLE_HOSTILE,
            Some(5),
            Some(25),
            &tmp_crew,
            42,
            0,
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            500,
            SupportBuffScenarioRequest::default(),
            opp,
            None,
            None,
            ScenarioEngagementOverride {
                enemy_type: Some(enemy_type),
                ..Default::default()
            },
        )
    };

    let red = run(EnemyType::RedMovingSpace, DefenderOpponent::Hostile);
    let outpost = run(EnemyType::OutpostArmadas, DefenderOpponent::Hostile);
    let retal = run(
        EnemyType::OutpostRetaliationAttackers,
        DefenderOpponent::Hostile,
    );
    let group_arm = run(EnemyType::GroupArmadas, DefenderOpponent::Hostile);
    let solo_arm = run(EnemyType::SoloArmadas, DefenderOpponent::Hostile);
    let boss = run(EnemyType::MissionBosses, DefenderOpponent::Hostile);
    let qtrial = run(EnemyType::QTrial, DefenderOpponent::Hostile);
    let pvp = run(EnemyType::PvpSpace, DefenderOpponent::Player);

    // Active modes: TMP Uhura BDA produces real isolytic cascade damage > 0, and TMP Sulu's
    // Apex Barrier reduces incoming counter-fire compared to inactive modes.
    for (label, res) in [
        ("red_moving_space", &red),
        ("outpost_armadas", &outpost),
        ("outpost_retaliation_attackers", &retal),
    ] {
        assert!(
            res.total_isolytic_damage > 1e-3,
            "TMP Uhura BDA should produce isolytic cascade damage on {label}, got {}",
            res.total_isolytic_damage
        );
    }

    // Inactive modes: battle_types [17,18,2,5,6,9] must NOT match 7, 8, 3, or 1.
    for (label, res) in [
        ("group_armadas", &group_arm),
        ("solo_armadas", &solo_arm),
        ("mission_bosses", &boss),
        ("q_trial", &qtrial),
        ("pvp_space", &pvp),
    ] {
        assert!(
            res.total_isolytic_damage < 1e-6,
            "TMP Uhura BDA must stay inert on {label}, got {}",
            res.total_isolytic_damage
        );
    }

    // TMP Sulu Apex Barrier keeps more hull on red space vs Q's Trial (same non-armada hull class!)
    assert!(
        red.attacker_hull_remaining > qtrial.attacker_hull_remaining + 1.0,
        "TMP Sulu Apex Barrier must reduce incoming damage on Red (bt=2) vs Q's Trial (bt=3): red_hull={}, qtrial_hull={}",
        red.attacker_hull_remaining,
        qtrial.attacker_hull_remaining
    );
}

#[test]
fn tasha_yar_data_worf_honor_exact_battle_type_boundaries_including_qtrial_and_outpost() {
    let reg = shared_registry();

    // 1. Tasha Yar bridge (AllDamage [2,3,5,6,7,8,9]) + Data bridge (AllPiercing [2,3,5,6,7,8,9])
    let tng_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Tasha Yar".into(), "Data".into()],
        below_decks: vec![],
    };

    let run = |enemy_type: EnemyType, opp: DefenderOpponent| {
        replay_optimize_iteration_with_registry_and_context(
            reg,
            SHIP_ID,
            DURABLE_HOSTILE,
            Some(5),
            Some(25),
            &tng_crew,
            42,
            0,
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
            500,
            SupportBuffScenarioRequest::default(),
            opp,
            None,
            None,
            ScenarioEngagementOverride {
                enemy_type: Some(enemy_type),
                ..Default::default()
            },
        )
    };

    let on_red = run(EnemyType::RedMovingSpace, DefenderOpponent::Hostile);
    let on_qtrial = run(EnemyType::QTrial, DefenderOpponent::Hostile);
    let on_boss = run(EnemyType::MissionBosses, DefenderOpponent::Hostile);
    let on_retal = run(
        EnemyType::OutpostRetaliationAttackers,
        DefenderOpponent::Hostile,
    );
    let on_group = run(EnemyType::GroupArmadas, DefenderOpponent::Hostile);
    let on_solo = run(EnemyType::SoloArmadas, DefenderOpponent::Hostile);
    let on_outpost = run(EnemyType::OutpostArmadas, DefenderOpponent::Hostile);

    // Same non-armada ship class comparison: Red(2), QTrial(3), Boss(3) beat OutpostRetaliator(18)
    for (label, res) in [("red", &on_red), ("qtrial", &on_qtrial), ("boss", &on_boss)] {
        assert!(
            res.total_damage > on_retal.total_damage + 1.0,
            "Tasha Yar + Data bridge must boost damage on {label} vs OutpostRetaliator (bt=18): {label}={}, retal={}",
            res.total_damage,
            on_retal.total_damage
        );
    }
    // Same armada ship class comparison: Group(7) and Solo(8) beat OutpostArmada(17)
    for (label, res) in [("group_armada", &on_group), ("solo_armada", &on_solo)] {
        assert!(
            res.total_damage > on_outpost.total_damage + 1.0,
            "Tasha Yar + Data bridge must boost damage on {label} vs OutpostArmada (bt=17): {label}={}, outpost={}",
            res.total_damage,
            on_outpost.total_damage
        );
    }

    // 2. Tasha Yar captain (CritChance Sub on EnemyShip, battle_types=[3] ONLY!)
    // Deterministic synthetic combatant with 100% enemy crit_chance (2.0x crit):
    // When battle_type == 3 (Mission Bosses / Q's Trial), Tasha captain lowers enemy crit_chance
    // below 100%, reducing damage taken across multiple rounds vs battle_type == 2 or 7!
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let tasha_cap = resolve_crew_to_buff_set(
        "tasha-yar-b9300c",
        &[],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    )
    .crew;
    let player = Combatant {
        id: "player".into(),
        hull_health: 500_000.0,
        attack: 10.0,
        ..Default::default()
    };
    let boss_enemy = Combatant {
        id: "boss".into(),
        hull_health: 500_000.0,
        attack: 1_000.0,
        crit_chance: 1.0,
        crit_multiplier: 3.0,
        ..Default::default()
    };
    let sim_bt = |bt: Option<u32>| {
        let cfg = SimulationConfig {
            rounds: 20,
            seed: 7,
            combat_battle_type_id: bt,
            ..Default::default()
        };
        simulate_combat_with_defender_faction_and_defender_crew(
            &player,
            &boss_enemy,
            &cfg,
            &tasha_cap,
            OpponentFactionTag::Unknown,
            ShipType::Battleship,
            ShipType::Battleship,
            true,
            false,
            &CrewConfiguration::default(),
        )
    };
    let res_bt3 = sim_bt(Some(3));
    let res_bt2 = sim_bt(Some(2));
    let res_bt7 = sim_bt(Some(7));
    assert!(
        res_bt3.attacker_hull_remaining > res_bt2.attacker_hull_remaining + 1.0,
        "Tasha Yar captain ([3]) must reduce enemy crit damage on battle_type 3 (MissionBoss/QTrial) vs 2: bt3={}, bt2={}",
        res_bt3.attacker_hull_remaining,
        res_bt2.attacker_hull_remaining
    );
    assert!(
        (res_bt2.attacker_hull_remaining - res_bt7.attacker_hull_remaining).abs() < 1e-6,
        "Tasha Yar captain ([3]) must stay inert on both battle_type 2 and 7: bt2={}, bt7={}",
        res_bt2.attacker_hull_remaining,
        res_bt7.attacker_hull_remaining
    );
}

#[test]
fn pvp_tactical_contexts_capture_node_mining_node_and_station_defense() {
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let opts = ResolveOptions {
        tier: Some(5),
        ..Default::default()
    };
    let ship = Combatant {
        id: "pvp_ship".into(),
        hull_health: 100_000.0,
        shield_health: 50_000.0,
        shield_mitigation: 0.80,
        attack: 500.0,
        ..Default::default()
    };
    let empty = CrewConfiguration::default();

    let run_pvp = |att_crew: &CrewConfiguration,
                   def_crew: &CrewConfiguration,
                   enemy_type: EnemyType,
                   game_context: CombatGameContext,
                   attacker_is_defending: bool,
                   attacker_at_station: bool| {
        let bt = match (enemy_type, game_context) {
            (
                EnemyType::PvpSpace,
                CombatGameContext::CaptureNode | CombatGameContext::MiningNode,
            ) => Some(4),
            _ => Some(1),
        };
        let cfg = SimulationConfig {
            rounds: 4,
            seed: 42,
            engagement_enemy_types: kobayashi::combat::EnemyTypes(vec![enemy_type]),
            combat_battle_type_id: bt,
            combat_game_context: game_context,
            attacker_is_defending,
            attacker_at_station,
            ..Default::default()
        };
        simulate_combat_with_defender_faction_and_defender_crew(
            &ship,
            &ship,
            &cfg,
            att_crew,
            OpponentFactionTag::Federation,
            ShipType::Explorer,
            ShipType::Explorer,
            false,
            true,
            def_crew,
        )
    };

    // 1. Emp. Georgiou bridge (ShotsPerAttack: battle_types=[4], game_context=capture_node, SelfAttacking)
    let georgiou =
        resolve_crew_to_buff_set("", &["emp-georgiou-1564b5".into()], &[], &officers, &opts).crew;
    let g_capture_att = run_pvp(
        &georgiou,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::CaptureNode,
        false,
        false,
    );
    let g_capture_def = run_pvp(
        &georgiou,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::CaptureNode,
        true,
        false,
    );
    let g_open_att = run_pvp(
        &georgiou,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::OpenSpace,
        false,
        false,
    );
    let g_mining_att = run_pvp(
        &georgiou,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::MiningNode,
        false,
        false,
    );
    assert!(
        g_capture_att.total_damage > g_open_att.total_damage + 1.0,
        "Emp. Georgiou ShotsPerAttack must fire when attacking a Capture Node (capture={}, open={})",
        g_capture_att.total_damage,
        g_open_att.total_damage
    );
    assert!((g_capture_def.total_damage - g_open_att.total_damage).abs() < 1e-6);
    assert!((g_mining_att.total_damage - g_open_att.total_damage).abs() < 1e-6);

    // 2. K'Bisch bridge (OpponentWeaponDamageReduction: battle_types=[4], game_context=mining_node, SelfDefending)
    let kbisch =
        resolve_crew_to_buff_set("", &["k-bisch-b6c84c".into()], &[], &officers, &opts).crew;
    let kb_mining_def = run_pvp(
        &kbisch,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::MiningNode,
        true,
        false,
    );
    let kb_mining_att = run_pvp(
        &kbisch,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::MiningNode,
        false,
        false,
    );
    let kb_capture_def = run_pvp(
        &kbisch,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::CaptureNode,
        true,
        false,
    );
    let kb_as_defender = run_pvp(
        &empty,
        &kbisch,
        EnemyType::PvpSpace,
        CombatGameContext::MiningNode,
        false,
        false,
    );
    let baseline = run_pvp(
        &empty,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::MiningNode,
        false,
        false,
    );
    assert!(
        kb_mining_def.attacker_hull_remaining > kb_mining_att.attacker_hull_remaining + 1.0,
        "K'Bisch must reduce incoming weapon damage when defending a Mining Node (def={}, att={})",
        kb_mining_def.attacker_hull_remaining,
        kb_mining_att.attacker_hull_remaining
    );
    assert!(
        (kb_capture_def.attacker_hull_remaining - kb_mining_att.attacker_hull_remaining).abs()
            < 1e-6
    );
    assert!(
        kb_as_defender.total_damage < baseline.total_damage - 1.0,
        "K'Bisch on PvP defender_crew must reduce attacker damage on Mining Node (kb_def={}, base={})",
        kb_as_defender.total_damage,
        baseline.total_damage
    );

    // 3. TOS Chekov bridge (OpponentWeaponDamageReduction: battle_types=[4], EnemyPlayer, SelfDefending)
    let chekov =
        resolve_crew_to_buff_set("", &["tos-chekov-0f158d".into()], &[], &officers, &opts).crew;
    let ch_cap_def = run_pvp(
        &chekov,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::CaptureNode,
        true,
        false,
    );
    let ch_mine_def = run_pvp(
        &chekov,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::MiningNode,
        true,
        false,
    );
    let ch_open_def = run_pvp(
        &chekov,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::OpenSpace,
        true,
        false,
    );
    let ch_cap_att = run_pvp(
        &chekov,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::CaptureNode,
        false,
        false,
    );
    assert!(ch_cap_def.attacker_hull_remaining > ch_open_def.attacker_hull_remaining + 1.0);
    assert!(ch_mine_def.attacker_hull_remaining > ch_open_def.attacker_hull_remaining + 1.0);
    assert!(
        (ch_cap_att.attacker_hull_remaining - ch_open_def.attacker_hull_remaining).abs() < 1e-6
    );

    // 4. Dixon Hill bridge (ShieldMitigation Sub: game_context=mining_node, SelfDefending)
    let dixon =
        resolve_crew_to_buff_set("", &["dixon-hill-b7ea10".into()], &[], &officers, &opts).crew;
    let dx_mine_def = run_pvp(
        &dixon,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::MiningNode,
        true,
        false,
    );
    let dx_open_def = run_pvp(
        &dixon,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::OpenSpace,
        true,
        false,
    );
    assert!(
        dx_mine_def.defender_hull_remaining < dx_open_def.defender_hull_remaining - 1.0,
        "Dixon Hill must lower opponent shield mitigation when defending a Mining Node (mine_dhull={}, open_dhull={})",
        dx_mine_def.defender_hull_remaining,
        dx_open_def.defender_hull_remaining
    );

    // 5. Airiam bridge (Burning proc: battle_types=[4], game_context=capture_node, SelfDefending)
    let airiam =
        resolve_crew_to_buff_set("", &["airiam-9265fc".into()], &[], &officers, &opts).crew;
    let ar_cap_def = run_pvp(
        &airiam,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::CaptureNode,
        true,
        false,
    );
    let ar_open_def = run_pvp(
        &airiam,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::OpenSpace,
        true,
        false,
    );
    assert!(
        ar_cap_def.defender_hull_remaining < ar_open_def.defender_hull_remaining - 1.0,
        "Airiam Burning must deal extra hull burn when defending a Capture Node (cap_dhull={}, open_dhull={})",
        ar_cap_def.defender_hull_remaining,
        ar_open_def.defender_hull_remaining
    );

    // 6. Zahra (SelfAtStation: captain HullHPMax + bridge AllDamage)
    // Fires when attacker_at_station=true AND automatically on defender_crew in pvp_station!
    let zahra = resolve_crew_to_buff_set("zahra-e3f002", &[], &[], &officers, &opts).crew;
    let z_at_station = run_pvp(
        &zahra,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::Station,
        false,
        true,
    );
    let z_open = run_pvp(
        &zahra,
        &empty,
        EnemyType::PvpSpace,
        CombatGameContext::OpenSpace,
        false,
        false,
    );
    assert!(
        z_at_station.total_damage > z_open.total_damage + 1.0,
        "Zahra SelfAtStation must boost weapon damage when attacker_at_station=true (station={}, open={})",
        z_at_station.total_damage,
        z_open.total_damage
    );
    let z_def_pvp_station = run_pvp(
        &empty,
        &zahra,
        EnemyType::PvpStation,
        CombatGameContext::Station,
        false,
        false,
    );
    let z_def_pvp_space = run_pvp(
        &empty,
        &zahra,
        EnemyType::PvpSpace,
        CombatGameContext::OpenSpace,
        false,
        false,
    );
    assert!(
        z_def_pvp_station.attacker_hull_remaining < z_def_pvp_space.attacker_hull_remaining - 1.0,
        "Zahra on defender_crew must automatically activate SelfAtStation in pvp_station raids (station_ahull={}, space_ahull={})",
        z_def_pvp_station.attacker_hull_remaining,
        z_def_pvp_space.attacker_hull_remaining
    );
}

#[test]
fn pvp_defender_symmetry_profile_roster_isolation_and_defender_osr() {
    let reg = shared_registry();

    // 1. Symmetric Darwin (SelfDefending) and Lursa (SelfAttacking) in combat engine
    let officers = index_lcars_officers_by_id(build_officer_model_file_default().unwrap().officers);
    let opts = ResolveOptions {
        tier: Some(5),
        ..Default::default()
    };
    let ship = Combatant {
        id: "pvp_ship".into(),
        hull_health: 50_000.0,
        attack: 500.0,
        ..Default::default()
    };
    let empty = CrewConfiguration::default();
    let cfg = SimulationConfig {
        rounds: 3,
        seed: 42,
        combat_battle_type_id: Some(1),
        ..Default::default()
    };

    let darwin = resolve_crew_to_buff_set("darwin-b8dc0a", &[], &[], &officers, &opts).crew;
    let darwin_att = simulate_combat_with_defender_faction_and_defender_crew(
        &ship,
        &ship,
        &cfg,
        &darwin,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &empty,
    );
    let darwin_def = simulate_combat_with_defender_faction_and_defender_crew(
        &ship,
        &ship,
        &cfg,
        &empty,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &darwin,
    );
    assert!(
        (darwin_att.total_damage - 1_500.0).abs() < 1e-6,
        "Darwin SelfDefending inert on attacker"
    );
    assert!(
        darwin_def.attacker_hull_remaining < 50_000.0 - 1_500.0 - 1.0,
        "Darwin SelfDefending must boost PvP defender counter-fire (ahull={})",
        darwin_def.attacker_hull_remaining
    );

    let lursa = resolve_crew_to_buff_set("", &["lursa-57a544".into()], &[], &officers, &opts).crew;
    let lursa_att = simulate_combat_with_defender_faction_and_defender_crew(
        &ship,
        &ship,
        &cfg,
        &lursa,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &empty,
    );
    let lursa_def = simulate_combat_with_defender_faction_and_defender_crew(
        &ship,
        &ship,
        &cfg,
        &empty,
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        false,
        true,
        &lursa,
    );
    assert!(
        lursa_att.attacker_hull_remaining > 50_000.0 - 1_500.0 + 1.0,
        "Lursa SelfAttacking must reduce defender counter-fire when attacking (ahull={})",
        lursa_att.attacker_hull_remaining
    );
    assert!(
        (lursa_def.total_damage - 1_500.0).abs() < 1e-6,
        "Lursa SelfAttacking must stay inert on PvP defender_crew (dmg={})",
        lursa_def.total_damage
    );

    // 2. End-to-end PvP defender_profile_id roster isolation via replay_optimize_seed_payload
    // Attacker profile has TOS Chekov at Tier 1; def_low has TOS Chekov at Tier 1; def_high has
    // TOS Chekov at Tier 5 (strictly stronger OpponentWeaponDamageReduction on Capture Node!).
    let att_prof = ScratchProfile::new("att_roster");
    let def_low = ScratchProfile::new("def_low_roster");
    let def_high = ScratchProfile::new("def_high_roster");

    let roster_json = |chekov_tier: u32, chekov_level: u32| {
        serde_json::json!({
            "officers": [
                { "canonical_officer_id": "cadet-kirk-a80563", "canonical_name": "Cadet Kirk", "tier": 3, "level": 15 },
                { "canonical_officer_id": "cadet-mccoy-13d460", "canonical_name": "Cadet McCoy", "tier": 3, "level": 15 },
                { "canonical_officer_id": "cadet-uhura-3ef15c", "canonical_name": "Cadet Uhura", "tier": 3, "level": 15 },
                { "canonical_officer_id": "tos-chekov-0f158d", "canonical_name": "TOS Chekov", "tier": chekov_tier, "level": chekov_level },
                { "canonical_officer_id": "mitchell-0217f7", "canonical_name": "Mitchell", "tier": 5, "level": 30 },
                { "canonical_officer_id": "spock-c04738", "canonical_name": "Spock", "tier": 5, "level": 30 },
                { "canonical_officer_id": "kirk-1323b6", "canonical_name": "Kirk", "tier": 5, "level": 30 }
            ]
        })
        .to_string()
    };
    att_prof.write_file("roster.imported.json", &roster_json(1, 1));
    def_low.write_file("roster.imported.json", &roster_json(1, 1));
    def_high.write_file("roster.imported.json", &roster_json(5, 30));

    let att_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["Cadet McCoy".into(), "Cadet Uhura".into()],
        below_decks: vec![],
    };
    let def_chekov_crew = CrewCandidate {
        captain: "Cadet Kirk".into(),
        bridge: vec!["TOS Chekov".into(), "Cadet McCoy".into()],
        below_decks: vec![],
    };

    let run_pvp_profile =
        |def_pid: &str, def_crew: &CrewCandidate, enemy_type_slug: &str, game_ctx: &str| {
            let body = serde_json::json!({
                "ship": "uss_enterprise_d",
                "hostile": "",
                "ship_tier": 5,
                "ship_level": 25,
                "defender_ship": "uss_enterprise_d",
                "defender_ship_tier": 5,
                "defender_ship_level": 25,
                "defender_profile_id": def_pid,
                "seed": 42,
                "sim_index": 0,
                "enemy_type": enemy_type_slug,
                "game_context": game_ctx,
                "crew": {
                    "captain": att_crew.captain,
                    "bridge": att_crew.bridge,
                    "below_decks": att_crew.below_decks
                },
                "defender_crew": {
                    "captain": def_crew.captain,
                    "bridge": def_crew.bridge,
                    "below_deck": def_crew.below_decks
                }
            })
            .to_string();
            let raw = replay_optimize_seed_payload(reg, &body, Some(&att_prof.id))
                .expect("replay_optimize_seed_payload pvp");
            serde_json::from_str::<serde_json::Value>(&raw).expect("parse pvp replay json")
        };

    let vs_low = run_pvp_profile(&def_low.id, &def_chekov_crew, "pvp_space", "capture_node");
    let vs_high = run_pvp_profile(&def_high.id, &def_chekov_crew, "pvp_space", "capture_node");
    let low_dmg = vs_low["summary"]["total_damage"]
        .as_f64()
        .expect("low total_damage");
    let high_dmg = vs_high["summary"]["total_damage"]
        .as_f64()
        .expect("high total_damage");
    assert!(
        high_dmg < low_dmg - 1.0,
        "PvP defender must resolve officer tiers from defender_profile_id's own roster (T5 TOS Chekov def allows less attacker dmg={} < T1 def dmg={}; low_summary={}, high_summary={}, low_events={})",
        high_dmg,
        low_dmg,
        vs_low["summary"],
        vs_high["summary"],
        vs_low["trace"]["events"],
    );

    // 3. Defender OfficerStatConditionContext: Mitchell captain (SelfBridge OfficerStatAll [SelfAtStation])
    // activates on defender_crew in pvp_station vs pvp_space!
    let def_mitchell_crew = CrewCandidate {
        captain: "Mitchell".into(),
        bridge: vec!["Spock".into(), "Kirk".into()],
        below_decks: vec![],
    };
    let mitchell_space =
        run_pvp_profile(&def_high.id, &def_mitchell_crew, "pvp_space", "open_space");
    let mitchell_station = run_pvp_profile(
        &def_high.id,
        &def_mitchell_crew,
        "pvp_station",
        "open_space",
    );
    let ms_ahull = mitchell_space["summary"]["attacker_hull_remaining"]
        .as_f64()
        .unwrap();
    let ms_dhull = mitchell_space["summary"]["defender_hull_remaining"]
        .as_f64()
        .unwrap();
    let ms_dmg = mitchell_space["summary"]["total_damage"].as_f64().unwrap();
    let st_ahull = mitchell_station["summary"]["attacker_hull_remaining"]
        .as_f64()
        .unwrap();
    let st_dhull = mitchell_station["summary"]["defender_hull_remaining"]
        .as_f64()
        .unwrap();
    let st_dmg = mitchell_station["summary"]["total_damage"]
        .as_f64()
        .unwrap();
    assert!(
        (st_ahull - ms_ahull).abs() > 1e-3
            || (st_dhull - ms_dhull).abs() > 1e-3
            || (st_dmg - ms_dmg).abs() > 1e-3,
        "Defender Mitchell SelfAtStation OfficerStatAll must activate in pvp_station vs pvp_space (station=({},{},{}), space=({},{},{}))",
        st_ahull,
        st_dhull,
        st_dmg,
        ms_ahull,
        ms_dhull,
        ms_dmg,
    );
}

#[test]
fn api_tactical_context_validation_echo_and_fingerprint_invalidation() {
    let reg = shared_registry();

    // 1. parse_optional_game_context validation
    assert_eq!(
        parse_optional_game_context(None),
        Ok(CombatGameContext::OpenSpace)
    );
    assert_eq!(
        parse_optional_game_context(Some("")),
        Ok(CombatGameContext::OpenSpace)
    );
    assert_eq!(
        parse_optional_game_context(Some("capture_node")),
        Ok(CombatGameContext::CaptureNode)
    );
    assert!(parse_optional_game_context(Some("capture_nodes")).is_err());

    // 2. Reject unknown game_context across all 5 endpoints
    let bad_opt = parse_optimize_request_body(&format!(
        r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","game_context":"bogus_ctx"}}"#
    ))
    .expect("json parse");
    let err = validate_request(reg, &bad_opt, 100).unwrap_err();
    assert!(
        err.to_string().contains("game_context"),
        "expected game_context validation error, got {err}"
    );

    assert!(simulate_payload(
        reg,
        &format!(
            r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","game_context":"bogus","crew":{{"captain":"Cadet Kirk","bridge":["Cadet McCoy","Cadet Uhura"]}}}}"#
        ),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .is_err());

    assert!(compare_crews_payload(
        reg,
        &format!(
            r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","game_context":"bogus","crews":[{{"captain":"Cadet Kirk","bridge":["Cadet McCoy","Cadet Uhura"]}},{{"captain":"Cadet Kirk","bridge":["Cadet McCoy","Phlox"]}}]}}"#
        ),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .is_err());

    assert!(crew_counterfactual_payload(
        reg,
        &format!(
            r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","game_context":"bogus","crew":{{"captain":"Cadet Kirk","bridge":["Cadet McCoy","Cadet Uhura"]}}}}"#
        ),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .is_err());

    assert!(replay_optimize_seed_payload(
        reg,
        &format!(
            r#"{{"ship":"{SHIP_ID}","hostile":"{NORMAL_EXPLORER_HOSTILE}","sim_index":0,"game_context":"bogus","crew":{{"captain":"Cadet Kirk","bridge":["Cadet McCoy","Cadet Uhura"]}}}}"#
        ),
        Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
    )
    .is_err());

    // 3. resolved_context echoes tactical fields across simulate, replay-seed, and optimize
    let sim_json: serde_json::Value = serde_json::from_str(
        &simulate_payload(
            reg,
            &serde_json::json!({
                "ship": "uss_saladin",
                "hostile": "",
                "defender_ship": "rotarran",
                "defender_ship_tier": 5,
                "defender_ship_level": 25,
                "defender_profile_id": "demo",
                "ship_tier": 9,
                "ship_level": 45,
                "num_sims": 2,
                "seed": 7,
                "game_context": "capture_node",
                "attacker_is_defending": true,
                "attacker_at_station": false,
                "crew": { "captain": "Cadet Kirk", "bridge": ["Airiam", "TOS Chekov"], "below_deck": [] }
            })
            .to_string(),
            Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS),
        )
        .expect("simulate pvp capture node"),
    )
    .expect("json");
    let sctx = &sim_json["resolved_context"];
    assert_eq!(sctx["combat_battle_type_id"], 4);
    assert_eq!(sctx["game_context"], "capture_node");
    assert_eq!(sctx["attacker_is_defending"], true);
    assert_eq!(sctx["attacker_at_station"], false);

    let opt_req: OptimizeRequest = serde_json::from_value(serde_json::json!({
        "ship": "uss_saladin",
        "hostile": NORMAL_EXPLORER_HOSTILE,
        "ship_tier": 9,
        "ship_level": 45,
        "sims": 2,
        "seed": 7,
        "max_candidates": 2,
        "strategy": "exhaustive",
        "game_context": "station",
        "attacker_at_station": true
    }))
    .expect("OptimizeRequest");
    let opt_res = run_optimize(reg, &opt_req, Some(NO_ROSTER_IMPORT_PROFILE_ID_FOR_TESTS))
        .expect("run_optimize");
    let octx = opt_res
        .scenario
        .resolved_context
        .expect("optimize resolved_context");
    assert_eq!(octx.combat_battle_type_id, Some(2));
    assert_eq!(octx.game_context, "station");
    assert!(!octx.attacker_is_defending);
    assert!(octx.attacker_at_station);

    // 4. scenario_fingerprint canonical equivalence & invalidation across tactical fields
    let base = ReuseScenarioInputs {
        ship: SHIP_ID,
        hostile: NORMAL_EXPLORER_HOSTILE,
        ship_tier: Some(5),
        ship_level: Some(25),
        below_decks_slots: 3,
        enemy_type: Some("pvp_space"),
        wave_defense: false,
        game_context: None,
        attacker_is_defending: false,
        attacker_at_station: false,
        defender_opponent: "player",
        support_buffs: None,
        defender_support_buffs: None,
        defender_alliance_debuffs: None,
        defender_ship: Some("rotarran"),
        defender_ship_tier: Some(5),
        defender_ship_level: Some(25),
        defender_profile_id: Some("demo"),
    };
    let fp_none = scenario_fingerprint(reg, &base);
    let fp_open = scenario_fingerprint(
        reg,
        &ReuseScenarioInputs {
            game_context: Some("open_space"),
            ..base.clone()
        },
    );
    assert_eq!(
        fp_none, fp_open,
        "omitted game_context and explicit open_space must canonicalize to the same fingerprint"
    );

    let fp_capture = scenario_fingerprint(
        reg,
        &ReuseScenarioInputs {
            game_context: Some("capture_node"),
            ..base.clone()
        },
    );
    let fp_defending = scenario_fingerprint(
        reg,
        &ReuseScenarioInputs {
            attacker_is_defending: true,
            ..base.clone()
        },
    );
    let fp_station = scenario_fingerprint(
        reg,
        &ReuseScenarioInputs {
            attacker_at_station: true,
            ..base.clone()
        },
    );
    assert_ne!(
        fp_none, fp_capture,
        "game_context change must invalidate fingerprint"
    );
    assert_ne!(
        fp_none, fp_defending,
        "attacker_is_defending change must invalidate fingerprint"
    );
    assert_ne!(
        fp_none, fp_station,
        "attacker_at_station change must invalidate fingerprint"
    );
}
