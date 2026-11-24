//! Update 96 integration regressions:
//! - Deterministic forbidden/chaos tech catalog + conditional routing for `Translocator`,
//!   `Silicon-based Parasites`, and `[PRO] Infusion Modulator`.
//! - All 7 new officers (Trelane, Rose Harper, Toryn, Planetfall Kirk/Ortegas/Scotty/Uhura)
//!   resolving from `officers.canonical.json` + `id_registry.json` through LCARS.
//! - Critical Mitigation rating (`CritMitigationRating`) additive rating stacking, symmetric PvP,
//!   and Polaron bypass.
//! - Infusion Damage ∄ (`InfusionCapacitors` + `InfusionCapacitorSpike`) bypassing Standard
//!   Mitigation and Shields directly to hull, including >100% scaling and Inertial Diverters.
//! - Polaron Weapons ∃ (`weapon_type == 6` → `WeaponType::Polaron`) bypassing Standard Mitigation,
//!   Apex Barrier, Critical Mitigation, and Shields; mitigated solely by Polaron Mitigation ∃
//!   (`1 / (1 + PM / 50_000)`); and Polaron Prototype hostile abilities (`Experimental Defense Array`
//!   + `Polaron Weaponry`).

use std::collections::HashMap;
use std::path::Path;

use kobayashi::combat::abilities::{
    Ability, AbilityClass, AbilityCondition, AbilityEffect, CrewSeat, CrewSeatContext,
    WeaponTypeScope, YUKI_OFFICER_LCARS_ID,
};
use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Combatant, CrewConfiguration,
    EnemyType, EnemyTypes, OpponentFactionTag, ShipType, SimulationConfig, TimingWindow, TraceMode,
    WeaponStats, WeaponType,
};
use kobayashi::data::forbidden_chaos::load_forbidden_chaos;
use kobayashi::data::hostile_ability_resolve::{
    hostile_abilities_to_defender_crew, hostile_ability_catalog_for_default_path,
};
use kobayashi::data::import::ForbiddenTechEntry;
use kobayashi::data::loader::resolve_hostile;
use kobayashi::data::profile::{
    infusion_modulator_forbidden_tech_seats, merge_tech_fids_into_profile_with_level_tier,
    silicon_based_parasites_chaos_tech_seats, PlayerProfile, INFUSION_MODULATOR_FORBIDDEN_TECH_FID,
    SILICON_BASED_PARASITES_CHAOS_TECH_FID,
};
use kobayashi::lcars::{
    build_officer_model_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    LcarsOfficer, ResolveOptions,
};

fn lcars_index() -> HashMap<String, LcarsOfficer> {
    index_lcars_officers_by_id(build_officer_model_default().expect("default officer model"))
}

fn base_combatant(id: &str) -> Combatant {
    Combatant {
        id: id.to_string(),
        attack: 1_000.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.5,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 100_000.0,
        shield_health: 50_000.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: Vec::new(),
        hostile_mitigation_params: None,
    }
}

fn seat(
    crew_seat: CrewSeat,
    class: AbilityClass,
    timing: TimingWindow,
    effect: AbilityEffect,
    condition: Option<AbilityCondition>,
) -> CrewSeatContext {
    CrewSeatContext::legacy(
        crew_seat,
        Ability {
            name: "test_seat".to_string(),
            class,
            timing,
            boostable: false,
            effect,
            condition,
            weapon_scope: WeaponTypeScope::All,
        },
        false,
    )
}

#[test]
fn update96_officers_resolve_through_lcars_and_id_registry() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let reg_raw = std::fs::read_to_string(manifest.join("data/officers/id_registry.json"))
        .expect("id_registry.json");
    let reg: HashMap<String, String> = serde_json::from_str(&reg_raw).expect("parse id_registry");
    let officers = lcars_index();

    let expected = [
        ("4190420561", "trelane-96b872", "Trelane"),
        ("1384069720", "rose-harper-3b4b1d", "Rose Harper"),
        ("1578897011", "toryn-818f4f", "Toryn"),
        ("1843096541", "planetfall-kirk-0def07", "Planetfall Kirk"),
        (
            "337016775",
            "planetfall-ortegas-408465",
            "Planetfall Ortegas",
        ),
        (
            "2817471102",
            "planetfall-scotty-5a60c7",
            "Planetfall Scotty",
        ),
        ("3594355267", "planetfall-uhura-f1ba9e", "Planetfall Uhura"),
    ];
    for (source_id, canonical_id, name) in expected {
        assert_eq!(
            reg.get(source_id).map(String::as_str),
            Some(canonical_id),
            "id_registry must bind {source_id} -> {canonical_id}"
        );
        let officer = officers
            .get(canonical_id)
            .unwrap_or_else(|| panic!("officer {canonical_id} missing from LCARS model"));
        assert_eq!(officer.name, name);
    }

    // Verify Planetfall crew resolves exact Rank 5 values.
    let buffs = resolve_crew_to_buff_set(
        "planetfall-kirk-0def07",
        &[
            "planetfall-ortegas-408465".to_string(),
            "planetfall-scotty-5a60c7".to_string(),
        ],
        &["planetfall-uhura-f1ba9e".to_string()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    );
    assert!(buffs.crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::PolaronMitigationMultiplier(v) if (v - 0.2).abs() < 1e-9
    )));
    assert!(buffs.crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::InfusionCapacitors(v) if (v - 150_000.0).abs() < 1e-6
    )));
    assert!(buffs.crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::PolaronMitigation(v) if (v - 500_000.0).abs() < 1e-6
    )));
    assert!(buffs.crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::InfusionCapacitorSpike(v) if (v - 1.0).abs() < 1e-9
    )));
}

#[test]
fn trelane_gates_on_wave_defense_and_tal_not_on_bridge() {
    let officers = lcars_index();
    let buffs_no_tal = resolve_crew_to_buff_set(
        "trelane-96b872",
        &[],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    );
    let buffs_with_tal = resolve_crew_to_buff_set(
        "trelane-96b872",
        &["tal-c3e4eb".to_string()],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    );

    let mut attacker = base_combatant("uss_enterprise");
    attacker.attack = 100.0;
    attacker.shield_mitigation = 0.8;
    let mut defender = base_combatant("wave_hostile");
    defender.attack = 10_000.0;

    // Default red hostile: Trelane is inert (0.8 shield mitigation, 0 apex barrier -> 2,000 hull taken).
    let cfg_default = SimulationConfig {
        rounds: 1,
        seed: 7,
        trace_mode: TraceMode::Off,
        ..Default::default()
    };
    let res_default = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg_default,
        &buffs_no_tal.crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_default.attacker_hull_remaining - 98_000.0).abs() < 1e-6,
        "Trelane must stay inert on default red hostiles; got {}",
        res_default.attacker_hull_remaining
    );

    // Wave Defense without Tal: +10% shield mitigation (0.9) and +50,000 Apex Barrier (1/6 damage).
    let mut cfg_wave = cfg_default.clone();
    cfg_wave.engagement_enemy_types = EnemyType::Waves.into();
    let res_wave_no_tal = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg_wave,
        &buffs_no_tal.crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    // 10,000 / (1 + 50,000 / 10,000) = 10,000 / 6 = 1666.6667 total; 10% hull = 166.6667 hull dmg.
    let expected_no_tal_hull = 100_000.0 - (10_000.0 / 6.0) * 0.1;
    assert!(
        (res_wave_no_tal.attacker_hull_remaining - expected_no_tal_hull).abs() < 1e-3,
        "expected {expected_no_tal_hull}, got {}",
        res_wave_no_tal.attacker_hull_remaining
    );

    // Wave Defense WITH Tal on bridge: Child's Play (+50,000 Apex Barrier) disabled, CM (+10% shield mit) still active.
    let res_wave_with_tal = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg_wave,
        &buffs_with_tal.crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    // Without Apex Barrier, 10,000 total; 0.9 shield mit -> 1,000 hull damage.
    assert!(
        (res_wave_with_tal.attacker_hull_remaining - 99_000.0).abs() < 1e-3,
        "Tal on bridge must disable Child's Play Apex Barrier while keeping General of Gothos; got {}",
        res_wave_with_tal.attacker_hull_remaining
    );
}

#[test]
fn rose_harper_and_toryn_pvp_gates_and_crit_mitigation_stacking() {
    let officers = lcars_index();
    let toryn_buffs = resolve_crew_to_buff_set(
        "kirk-1323b6",
        &[],
        &["toryn-818f4f".to_string()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    );
    let rose_buffs = resolve_crew_to_buff_set(
        "kirk-1323b6",
        &[],
        &["rose-harper-3b4b1d".to_string()],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    );

    let mut attacker = base_combatant("uss_enterprise");
    attacker.attack = 100.0;
    attacker.shield_health = 0.0;
    let mut pvp_defender = base_combatant("pvp_opponent");
    pvp_defender.attack = 1_000.0;
    pvp_defender.crit_chance = 1.0;
    pvp_defender.crit_multiplier = 2.0;
    pvp_defender.isolytic_damage = 1.0;

    let mut cfg = SimulationConfig {
        rounds: 1,
        seed: 11,
        trace_mode: TraceMode::Off,
        attacker_owner_faction: OpponentFactionTag::Federation,
        ..Default::default()
    };

    // Toryn R5 = 50,000 Critical Mitigation -> reduction = 50,000 / 100,000 = 50% on crits vs players.
    let mut pvp_def_no_iso = pvp_defender.clone();
    pvp_def_no_iso.isolytic_damage = 0.0;
    let res_pvp = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &pvp_def_no_iso,
        &cfg,
        &toryn_buffs.crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Explorer,
        false,
        true,
        &CrewConfiguration::default(),
    );
    // 1,000 * 2.0 * (1 - 0.5) = 1,000 damage -> 99,000 hull remaining.
    assert!(
        (res_pvp.attacker_hull_remaining - 99_000.0).abs() < 1e-6,
        "Toryn R5 must cut PvP crit damage in half; got {}",
        res_pvp.attacker_hull_remaining
    );
    // Inert vs NPC hostiles: full 2,000 damage -> 98,000 hull remaining.
    let res_pve = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &pvp_def_no_iso,
        &cfg,
        &toryn_buffs.crew,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_pve.attacker_hull_remaining - 98_000.0).abs() < 1e-6,
        "Toryn must stay inert vs NPC hostiles; got {}",
        res_pve.attacker_hull_remaining
    );

    // Two 25,000 Critical Mitigation seats must stack in rating space (50,000 -> 50% reduction, NOT 55.56%).
    let two_cm = CrewConfiguration {
        seats: vec![
            seat(
                CrewSeat::BelowDeck,
                AbilityClass::BelowDeck,
                TimingWindow::CombatBegin,
                AbilityEffect::CritMitigationRating(25_000.0),
                Some(AbilityCondition::DefenderIsPlayerShip),
            ),
            seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::CritMitigationRating(25_000.0),
                Some(AbilityCondition::DefenderIsPlayerShip),
            ),
        ],
    };
    let res_stacked = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &pvp_def_no_iso,
        &cfg,
        &two_cm,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Explorer,
        false,
        true,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_stacked.attacker_hull_remaining - 99_000.0).abs() < 1e-6,
        "25k + 25k CM ratings must sum to 50k -> 50% reduction; got {}",
        res_stacked.attacker_hull_remaining
    );

    // Symmetric PvP: defender running Toryn reduces attacker's outbound crits by 50%.
    let mut crit_att = base_combatant("pvp_attacker");
    crit_att.attack = 1_000.0;
    crit_att.crit_chance = 1.0;
    crit_att.crit_multiplier = 2.0;
    let mut plain_def = base_combatant("pvp_defender");
    plain_def.attack = 0.0;
    plain_def.shield_health = 0.0;
    let res_sym = simulate_combat_with_defender_faction_and_defender_crew(
        &crit_att,
        &plain_def,
        &cfg,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Explorer,
        false,
        true,
        &toryn_buffs.crew,
    );
    assert!(
        (res_sym.defender_hull_remaining - 99_000.0).abs() < 1e-6,
        "PvP defender with Toryn R5 must cut attacker outbound crit by 50%; got {}",
        res_sym.defender_hull_remaining
    );

    // Rose Harper R5 (+10.0 Isolytic Defense) active on Federation vs player, inert on non-FKR ship.
    let mut iso_def = base_combatant("pvp_iso");
    iso_def.attack = 1_000.0;
    iso_def.crit_chance = 0.0;
    iso_def.isolytic_damage = 1.0;
    let res_rose_fed = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &iso_def,
        &cfg,
        &rose_buffs.crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Explorer,
        false,
        true,
        &CrewConfiguration::default(),
    );
    cfg.attacker_owner_faction = OpponentFactionTag::Unknown;
    let res_rose_non_fkr = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &iso_def,
        &cfg,
        &rose_buffs.crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Explorer,
        false,
        true,
        &CrewConfiguration::default(),
    );
    assert!(
        res_rose_fed.attacker_hull_remaining > res_rose_non_fkr.attacker_hull_remaining + 500.0,
        "Rose Harper Isolytic Defense must protect FKR ships in PvP ({} vs {})",
        res_rose_fed.attacker_hull_remaining,
        res_rose_non_fkr.attacker_hull_remaining
    );
    // U.S.S. Vengeance exception works even when owner faction is Unknown.
    let mut vengeance = attacker.clone();
    vengeance.id = "uss_vengeance".to_string();
    let res_rose_veng = simulate_combat_with_defender_faction_and_defender_crew(
        &vengeance,
        &iso_def,
        &cfg,
        &rose_buffs.crew,
        OpponentFactionTag::Unknown,
        ShipType::Explorer,
        ShipType::Interceptor,
        false,
        true,
        &CrewConfiguration::default(),
    );
    assert!(
        (res_rose_veng.attacker_hull_remaining - res_rose_fed.attacker_hull_remaining).abs() < 1e-6,
        "Rose Harper must also activate on U.S.S. Vengeance"
    );
}

#[test]
fn infusion_capacitors_and_spike_math_with_trace_parity() {
    let mut attacker = base_combatant("uss_enterprise");
    attacker.attack = 10_000.0;
    let mut defender = base_combatant("fkr_target");
    defender.attack = 0.0;
    defender.hull_health = 100_000.0;
    defender.shield_health = 50_000.0;
    defender.shield_mitigation = 0.8;
    defender.mitigation = 0.7; // 30% standard damage through

    // Worked tooltip example: 5,000 capacitors + 100% spike = 10,000 -> 10% Infusion Damage ∄.
    // Standard leg: 90% of 10,000 * 0.3 mitigation = 2,700 (80% shield = 2,160, 20% hull = 540).
    // Infusion leg: 10% of 10,000 = 1,000 (bypasses mitigation & shields -> +1,000 hull).
    // Total hull damage = 1,540 -> hull remaining = 98,460; shield remaining = 47,840.
    let crew_10pct = CrewConfiguration {
        seats: vec![
            seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::InfusionCapacitors(5_000.0),
                None,
            ),
            seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::InfusionCapacitorSpike(1.0),
                None,
            ),
        ],
    };

    let cfg_off = SimulationConfig {
        rounds: 1,
        seed: 19,
        trace_mode: TraceMode::Off,
        ..Default::default()
    };
    let mut cfg_events = cfg_off.clone();
    cfg_events.trace_mode = TraceMode::Events;

    let res_off = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg_off,
        &crew_10pct,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let res_events = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg_events,
        &crew_10pct,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );

    assert!((res_off.defender_hull_remaining - 98_460.0).abs() < 1e-6);
    assert!((res_off.defender_shield_remaining - 47_840.0).abs() < 1e-6);
    assert_eq!(
        res_off.defender_hull_remaining,
        res_events.defender_hull_remaining
    );
    assert_eq!(
        res_off.defender_shield_remaining,
        res_events.defender_shield_remaining
    );
    assert!(res_events.events.iter().any(|e| {
        e.event_type == "damage_application"
            && e.values
                .get("infusion_component")
                .and_then(|v| v.as_f64())
                .is_some_and(|x| (x - 1_000.0).abs() < 1e-6)
    }));

    // >100% scaling: 150,000 capacitors + 100% spike = 300,000 -> 300% Infusion Damage directly to hull,
    // 0% unconverted standard damage -> 0 shield damage, 30,000 hull damage!
    let crew_300pct = CrewConfiguration {
        seats: vec![
            seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::InfusionCapacitors(150_000.0),
                None,
            ),
            seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::InfusionCapacitorSpike(1.0),
                None,
            ),
        ],
    };
    let res_300 = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg_off,
        &crew_300pct,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!((res_300.defender_hull_remaining - 70_000.0).abs() < 1e-6);
    assert!((res_300.defender_shield_remaining - 50_000.0).abs() < 1e-6);
}

#[test]
fn polaron_weapon_bypasses_all_standard_defenses_and_uses_polaron_mitigation() {
    assert!(!WeaponTypeScope::KineticOnly.matches(WeaponType::Polaron));
    assert!(!WeaponTypeScope::EnergyOnly.matches(WeaponType::Polaron));
    assert!(WeaponTypeScope::All.matches(WeaponType::Polaron));

    let mut attacker = base_combatant("uss_enterprise");
    attacker.attack = 100.0;
    attacker.hull_health = 100_000.0;
    attacker.shield_health = 50_000.0;
    attacker.shield_mitigation = 0.9;
    attacker.mitigation = 0.7;
    attacker.apex_barrier = 100_000.0; // would reduce normal damage by >90%

    let mut defender = base_combatant("polaron_shooter");
    defender.weapons = vec![WeaponStats {
        attack: 10_000.0,
        shots: Some(1),
        crit_chance: Some(1.0),
        crit_multiplier: Some(2.0),
        weapon_type: WeaponType::Polaron,
        ..Default::default()
    }];

    // Crew with Toryn (50k Critical Mitigation — must be BYPASSED by Polaron!)
    let cm_only_crew = CrewConfiguration {
        seats: vec![seat(
            CrewSeat::BelowDeck,
            AbilityClass::BelowDeck,
            TimingWindow::CombatBegin,
            AbilityEffect::CritMitigationRating(50_000.0),
            None,
        )],
    };
    let cfg = SimulationConfig {
        rounds: 1,
        seed: 23,
        trace_mode: TraceMode::Off,
        attacker_owner_faction: OpponentFactionTag::Federation,
        ..Default::default()
    };
    let res_unmitigated = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &cm_only_crew,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    // 10,000 * 2.0 crit = 20,000 directly to hull; shields untouched!
    assert!(
        (res_unmitigated.attacker_hull_remaining - 80_000.0).abs() < 1e-6,
        "Polaron weapon must bypass Standard Mitigation, Apex Barrier, Critical Mitigation, and Shields; got {}",
        res_unmitigated.attacker_hull_remaining
    );
    assert!(
        (res_unmitigated.attacker_shield_remaining - 50_000.0).abs() < 1e-6,
        "Polaron weapon must leave shields untouched; got {}",
        res_unmitigated.attacker_shield_remaining
    );

    // With Ortegas R5 (500,000 PM) + Kirk CM (+20% PM multiplier) = 600,000 effective PM:
    // factor = 1 / (1 + 600,000 / 50,000) = 1 / 13. Damage = 20,000 / 13 = 1538.4615...
    let pm_crew = CrewConfiguration {
        seats: vec![
            seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::PolaronMitigation(500_000.0),
                None,
            ),
            seat(
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                TimingWindow::CombatBegin,
                AbilityEffect::PolaronMitigationMultiplier(0.2),
                None,
            ),
        ],
    };
    let mut cfg_events = cfg.clone();
    cfg_events.trace_mode = TraceMode::Events;
    let res_pm_off = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg,
        &pm_crew,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let res_pm_events = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &cfg_events,
        &pm_crew,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    let expected_hull = 100_000.0 - (20_000.0 / 13.0);
    assert!(
        (res_pm_off.attacker_hull_remaining - expected_hull).abs() < 1e-4,
        "expected {expected_hull}, got {}",
        res_pm_off.attacker_hull_remaining
    );
    assert_eq!(
        res_pm_off.attacker_hull_remaining,
        res_pm_events.attacker_hull_remaining
    );
}

#[test]
fn real_polaron_prototype_hostile_abilities_and_weapon_types() {
    let hostile = resolve_hostile("1020392011").expect("Federation Polaron Prototype ∃");
    assert_eq!(
        hostile.opponent_faction_tag(),
        OpponentFactionTag::Federation
    );
    let weapons = hostile.weapons_for_counter_attack(
        ShipType::Battleship,
        kobayashi::combat::DefenderStats::default(),
    );
    assert_eq!(weapons.len(), 4);
    assert_eq!(weapons[0].weapon_type, WeaponType::Polaron);
    assert_eq!(weapons[1].weapon_type, WeaponType::Polaron);
    assert_eq!(weapons[2].weapon_type, WeaponType::Energy);
    assert_eq!(weapons[3].weapon_type, WeaponType::Kinetic);

    let catalog = hostile_ability_catalog_for_default_path();
    let def_crew = hostile_abilities_to_defender_crew(&hostile.ability, catalog, hostile.level);
    assert!(def_crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::HostileNegatesStandardDamage
    )));
    assert!(def_crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::HostileShieldRestoreOnDeplete
    )));
    assert!(def_crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::HostilePolaronDamageBonus { .. }
    )));
    assert!(def_crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::HostileLethalUnlessAttackerFaction {
            allow_federation: true,
            allow_klingon: true,
            allow_romulan: true,
            allow_uss_vengeance: true,
        }
    )));
    assert!(def_crew.seats.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::HostileLethalIfAttackerOfficer {
            forbid_yuki: true,
            forbid_tal: false,
        }
    )));

    let mut attacker = base_combatant("uss_enterprise");
    attacker.attack = 10_000.0;
    attacker.hull_health = 10_000_000.0;
    let mut def_combatant = base_combatant("polaron_proto_mock");
    def_combatant.attack = 100.0;
    def_combatant.hull_health = 100_000.0;
    def_combatant.shield_health = 50_000.0;
    def_combatant.shield_mitigation = 1.0;

    // 1. Non-FKR / non-Vengeance ship -> Not Worthy instant loss at round 0.
    let cfg_unknown = SimulationConfig {
        rounds: 3,
        seed: 7,
        trace_mode: TraceMode::Off,
        attacker_owner_faction: OpponentFactionTag::Borg,
        ..Default::default()
    };
    let res_not_worthy = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &def_combatant,
        &cfg_unknown,
        &CrewConfiguration::default(),
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &def_crew,
    );
    assert_eq!(res_not_worthy.rounds_simulated, 0);
    assert!(!res_not_worthy.attacker_won);

    // 2. FKR ship with Yuki on board -> Thorn in My Side instant loss at round 0.
    let mut cfg_yuki = cfg_unknown.clone();
    cfg_yuki.attacker_owner_faction = OpponentFactionTag::Klingon;
    cfg_yuki.attacker_roster_officer_ids = vec![YUKI_OFFICER_LCARS_ID.to_string()];
    let res_yuki = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &def_combatant,
        &cfg_yuki,
        &CrewConfiguration::default(),
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &def_crew,
    );
    assert_eq!(res_yuki.rounds_simulated, 0);
    assert!(!res_yuki.attacker_won);

    // 3. Klingon Battleship (counter class, different faction) with no Infusion ->
    //    deals 0 damage (Inertial Diverters) and dies at round 8 (Test of Power).
    let cfg_fkr = SimulationConfig {
        rounds: 10,
        seed: 7,
        trace_mode: TraceMode::Off,
        attacker_owner_faction: OpponentFactionTag::Klingon,
        ..Default::default()
    };
    let res_no_infusion = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &def_combatant,
        &cfg_fkr,
        &CrewConfiguration::default(),
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &def_crew,
    );
    assert!((res_no_infusion.defender_hull_remaining - 100_000.0).abs() < 1e-6);
    assert_eq!(res_no_infusion.rounds_simulated, 8);
    assert!(!res_no_infusion.attacker_won);

    // 4. Same Klingon Battleship with Planetfall Kirk + Ortegas + Scotty (R5):
    //    300% Infusion Damage = 30,000/round directly to hull -> kills in 4 rounds before round 8!
    let officers = lcars_index();
    let planetfall_buffs = resolve_crew_to_buff_set(
        "planetfall-kirk-0def07",
        &[
            "planetfall-ortegas-408465".to_string(),
            "planetfall-scotty-5a60c7".to_string(),
        ],
        &[],
        &officers,
        &ResolveOptions {
            tier: Some(5),
            ..Default::default()
        },
    );
    let res_planetfall = simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &def_combatant,
        &cfg_fkr,
        &planetfall_buffs.crew,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &def_crew,
    );
    assert!(
        res_planetfall.attacker_won,
        "Planetfall crew should defeat Polaron Prototype via Infusion Damage before round 8"
    );
    assert_eq!(res_planetfall.rounds_simulated, 4);
}

#[test]
fn update96_forbidden_and_chaos_tech_conditional_routing_and_rank_curves() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let catalog_path = manifest.join("data/forbidden_chaos_tech.json");
    let catalog = load_forbidden_chaos(catalog_path.to_str().unwrap())
        .expect("load data/forbidden_chaos_tech.json");

    // All 3 new FIDs exist in the catalog.
    for fid in [
        1_005_637_686_i64,
        SILICON_BASED_PARASITES_CHAOS_TECH_FID,
        INFUSION_MODULATOR_FORBIDDEN_TECH_FID,
    ] {
        assert!(
            catalog.items.iter().any(|r| r.fid == Some(fid)),
            "fid {fid} missing from data/forbidden_chaos_tech.json"
        );
    }

    // Neither Silicon-based Parasites nor [PRO] Infusion Modulator may leak into flat profile bonuses!
    let imported = vec![
        ForbiddenTechEntry {
            fid: SILICON_BASED_PARASITES_CHAOS_TECH_FID,
            tier: 12,
            level: 60,
            shard_count: 0,
        },
        ForbiddenTechEntry {
            fid: INFUSION_MODULATOR_FORBIDDEN_TECH_FID,
            tier: 12,
            level: 60,
            shard_count: 0,
        },
    ];
    let effective = vec![
        SILICON_BASED_PARASITES_CHAOS_TECH_FID,
        INFUSION_MODULATOR_FORBIDDEN_TECH_FID,
    ];
    let mut profile = PlayerProfile::default();
    merge_tech_fids_into_profile_with_level_tier(
        &mut profile,
        &effective,
        &imported,
        &catalog,
        false,
    );
    assert!(
        profile.bonuses.is_empty(),
        "Update 96 conditional FT/CT must never merge unconditionally into profile.bonuses; got {:?}",
        profile.bonuses
    );

    // Silicon-based Parasites at T1/L1 = +2.5 (250%), T12/L60 = +12.0 (1200%) vs Outposts/Retaliators.
    let sp_t1 = silicon_based_parasites_chaos_tech_seats(
        &[ForbiddenTechEntry {
            fid: SILICON_BASED_PARASITES_CHAOS_TECH_FID,
            tier: 1,
            level: 1,
            shard_count: 0,
        }],
        &[SILICON_BASED_PARASITES_CHAOS_TECH_FID],
        &catalog,
    );
    assert_eq!(sp_t1.len(), 1);
    assert!(matches!(
        sp_t1[0].ability.effect,
        AbilityEffect::AttackMultiplier(v) if (v - 2.5).abs() < 1e-9
    ));
    let sp_max = silicon_based_parasites_chaos_tech_seats(&imported, &effective, &catalog);
    assert_eq!(sp_max.len(), 1);
    assert!(matches!(
        sp_max[0].ability.effect,
        AbilityEffect::AttackMultiplier(v) if (v - 12.0).abs() < 1e-9
    ));

    // Verify Silicon-based Parasites fires ONLY on FKR/Vengeance vs Outposts/Retaliators, NOT red hostiles.
    let mut att = base_combatant("uss_enterprise");
    att.attack = 1_000.0;
    let mut def = base_combatant("outpost_target");
    def.attack = 0.0;
    def.shield_health = 0.0;
    let mut cfg = SimulationConfig {
        rounds: 1,
        seed: 31,
        trace_mode: TraceMode::Off,
        attacker_owner_faction: OpponentFactionTag::Federation,
        ..Default::default()
    };
    let crew_sp = CrewConfiguration { seats: sp_max };
    let res_red = simulate_combat_with_defender_faction_and_defender_crew(
        &att,
        &def,
        &cfg,
        &crew_sp,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!((res_red.total_damage - 1_000.0).abs() < 1e-6);

    cfg.engagement_enemy_types = EnemyTypes::single(EnemyType::OutpostRetaliationAttackers);
    let res_outpost = simulate_combat_with_defender_faction_and_defender_crew(
        &att,
        &def,
        &cfg,
        &crew_sp,
        OpponentFactionTag::Federation,
        ShipType::Explorer,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!((res_outpost.total_damage - 13_000.0).abs() < 1e-6);

    // [PRO] Infusion Modulator tier unlocks:
    // T1/L1: only Infusion Capacitors (1,000); T3/L10: +Weapon Damage (0.3); T12/L60: 15,000 + 70.0 + 4.0.
    let im_t1 = infusion_modulator_forbidden_tech_seats(
        &[ForbiddenTechEntry {
            fid: INFUSION_MODULATOR_FORBIDDEN_TECH_FID,
            tier: 1,
            level: 1,
            shard_count: 0,
        }],
        &[INFUSION_MODULATOR_FORBIDDEN_TECH_FID],
        &catalog,
    );
    assert_eq!(im_t1.len(), 1);
    assert!(matches!(
        im_t1[0].ability.effect,
        AbilityEffect::InfusionCapacitors(v) if (v - 1_000.0).abs() < 1e-6
    ));

    let im_max = infusion_modulator_forbidden_tech_seats(&imported, &effective, &catalog);
    assert_eq!(im_max.len(), 3);
    assert!(im_max.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::InfusionCapacitors(v) if (v - 15_000.0).abs() < 1e-6
    )));
    assert!(im_max.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::AttackMultiplier(v) if (v - 70.0).abs() < 1e-6
    )));
    assert!(im_max.iter().any(|s| matches!(
        s.ability.effect,
        AbilityEffect::BaseCritDamageBonus(v) if (v - 4.0).abs() < 1e-6
    )));
}
