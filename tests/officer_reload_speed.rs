//! AllReloadSpeed / AllLoadSpeed: enemy delay vs self-recharge (Uhura, Chang, Kuron, Vixis, Pon, Rom, Ortegas).

use std::collections::HashMap;
use std::sync::OnceLock;

use kobayashi::lcars::LcarsOfficer;

use kobayashi::combat::abilities::{
    Ability, AbilityClass, CrewSeat, CrewSeatContext, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use kobayashi::combat::effect_spec_compile::compile_officer_combat_spec;
use kobayashi::combat::{
    active_effects_for_timing, build_combat_setup, simulate_combat_from_setup, AbilityEffect,
    Combatant, CrewConfiguration, ShipType, SimulationConfig, TimingWindow, TraceMode, WeaponStats,
};
use kobayashi::data::combat_effect_spec::AbilityModifierSpec;
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id,
    lcars_effect_to_combat_effect_spec, resolve_crew_to_buff_set, ResolveOptions,
};

fn delay_events(events: &[kobayashi::combat::CombatEvent]) -> Vec<&kobayashi::combat::CombatEvent> {
    events
        .iter()
        .filter(|e| e.event_type == "defender_fire_delay_trigger")
        .collect()
}

fn shots_bonus_events(
    events: &[kobayashi::combat::CombatEvent],
) -> Vec<&kobayashi::combat::CombatEvent> {
    events
        .iter()
        .filter(|e| e.event_type == "shots_bonus_trigger")
        .collect()
}

fn event_bool(ev: &kobayashi::combat::CombatEvent, key: &str) -> bool {
    ev.values
        .get(key)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn event_phase(ev: &kobayashi::combat::CombatEvent) -> &str {
    ev.phase.as_str()
}

fn breaking_attacker(crit_chance: f64) -> Combatant {
    Combatant {
        id: "att".into(),
        attack: 800.0,
        mitigation: 0.0,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.9,
        crit_chance,
        crit_multiplier: 2.0,
        crit_damage_floor: 0.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 50_000.0,
        shield_health: 5_000.0,
        shield_mitigation: 0.3,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![WeaponStats {
            attack: 2_500.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn crew_with_defender_hull_breach(mut crew: CrewConfiguration) -> CrewConfiguration {
    crew.seats.push(CrewSeatContext {
        seat: CrewSeat::Captain,
        ability: Ability {
            name: "test_defender_hull_breach".into(),
            class: AbilityClass::CaptainManeuver,
            timing: TimingWindow::RoundStart,
            boostable: false,
            effect: AbilityEffect::HullBreach {
                chance: 1.0,
                duration_rounds: 10,
                requires_critical: false,
            },
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    });
    crew
}

fn uhura_shield_break_setup(
    crew: &CrewConfiguration,
    seed: u64,
    rounds: u32,
) -> kobayashi::combat::PreCombatSetup {
    let mut attacker = breaking_attacker(0.0);
    attacker.attack = 5_000.0;
    attacker.pierce = 1.0;
    if let Some(w) = attacker.weapons.first_mut() {
        w.attack = 1_000.0;
    }
    let mut defender = shielded_defender();
    defender.shield_health = 1_500.0;
    build_combat_setup(
        &attacker,
        &defender,
        &sim_config(seed, rounds),
        crew,
        kobayashi::combat::OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    )
}

fn shielded_defender() -> Combatant {
    Combatant {
        id: "def".into(),
        attack: 50.0,
        mitigation: 0.2,
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
        hull_health: 100_000.0,
        shield_health: 8_000.0,
        shield_mitigation: 0.5,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![kobayashi::combat::WeaponStats {
            attack: 50.0,
            shots: None,
            ..Default::default()
        }],
        hostile_mitigation_params: None,
    }
}

fn sim_config(seed: u64, rounds: u32) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed,
        trace_mode: TraceMode::Events,
        defender_level: Some(50),
        ..Default::default()
    }
}

fn lcars_officers_by_id() -> &'static HashMap<String, LcarsOfficer> {
    static OFFICERS: OnceLock<HashMap<String, LcarsOfficer>> = OnceLock::new();
    OFFICERS.get_or_init(|| {
        let file = build_officer_model_file_default().expect("build officer model");
        index_lcars_officers_by_id(file.officers)
    })
}

fn resolve_crew(captain: &str, bridge: &[String], tier: u8) -> CrewConfiguration {
    let officers = lcars_officers_by_id();
    let opts = ResolveOptions {
        tier: Some(tier),
        officer_tiers: None,
        officer_levels: None,
    };
    resolve_crew_to_buff_set(captain, bridge, &[], officers, &opts).crew
}

fn setup_fight(
    crew: &CrewConfiguration,
    seed: u64,
    rounds: u32,
    attacker_ship: ShipType,
    defender_is_npc: bool,
    defender_is_player: bool,
) -> kobayashi::combat::PreCombatSetup {
    setup_fight_with_defender(
        crew,
        seed,
        rounds,
        attacker_ship,
        defender_is_npc,
        defender_is_player,
        shielded_defender(),
    )
}

fn setup_fight_with_defender(
    crew: &CrewConfiguration,
    seed: u64,
    rounds: u32,
    attacker_ship: ShipType,
    defender_is_npc: bool,
    defender_is_player: bool,
    defender: Combatant,
) -> kobayashi::combat::PreCombatSetup {
    build_combat_setup(
        &breaking_attacker(0.0),
        &defender,
        &sim_config(seed, rounds),
        crew,
        kobayashi::combat::OpponentFactionTag::Unknown,
        ShipType::Battleship,
        attacker_ship,
        defender_is_npc,
        defender_is_player,
        &CrewConfiguration::default(),
    )
}

#[test]
fn kuron_captain_resolves_combat_begin_shots_bonus() {
    let crew = resolve_crew("kuron-15cda2", &[], 1);
    let effects = active_effects_for_timing(&crew, TimingWindow::CombatBegin);
    assert!(
        effects.iter().any(|e| {
            matches!(
                e.effect,
                AbilityEffect::ShotsBonus {
                    chance,
                    bonus_pct: 1.0,
                    ..
                } if chance > 0.0
            )
        }),
        "Kuron captain should resolve AttackerWeaponRecharge as CombatBegin ShotsBonus: {:?}",
        effects
            .iter()
            .map(|e| (e.ability_name.clone(), e.effect))
            .collect::<Vec<_>>()
    );
}

#[test]
fn uhura_captain_emits_defender_fire_delay_on_shield_break() {
    let crew = resolve_crew("uhura-ea117c", &[], 1);
    assert!(
        active_effects_for_timing(&crew, TimingWindow::ShieldBreak)
            .iter()
            .any(|e| matches!(e.effect, AbilityEffect::DefenderFireDelay { .. })),
        "Uhura captain should resolve shield-break DefenderFireDelay"
    );
    let setup = uhura_shield_break_setup(&crew, 0, 4);
    assert!(
        setup
            .shield_break_effects
            .iter()
            .any(|e| matches!(e.effect, AbilityEffect::DefenderFireDelay { .. })),
        "precomputed shield_break effects should include Uhura delay"
    );
    let mut saw_delay = false;
    for seed in 0..300_u64 {
        let setup = uhura_shield_break_setup(&crew, seed, 4);
        let result = simulate_combat_from_setup(&setup, seed);
        if delay_events(&result.events)
            .iter()
            .any(|e| event_bool(e, "triggered") && event_phase(e) == "shield_break")
        {
            saw_delay = true;
            break;
        }
    }
    assert!(
        saw_delay,
        "Uhura captain should proc defender fire delay on shield break in some seeds"
    );
}

#[test]
fn chang_bridge_compiles_enemy_delay_with_requires_critical() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
    let chang = file
        .officers
        .into_iter()
        .find(|o| o.id == "chang-ecc238")
        .expect("chang");
    let bridge = chang.bridge_ability.expect("bridge");
    let effect = bridge
        .effects
        .iter()
        .find(|e| e.tag.as_deref().is_some_and(|t| t.contains("enemy_delay")))
        .expect("reload effect");
    assert_eq!(effect.trigger.as_deref(), Some("on_critical"));
    let spec = lcars_effect_to_combat_effect_spec(
        effect,
        "test:chang:reload",
        "chang-ecc238",
        &bridge.name,
        Some(5),
        None,
    )
    .expect("compile");
    assert_eq!(spec.modifier, AbilityModifierSpec::DefenderFireDelay);
    let (_, compiled, _) = compile_officer_combat_spec(&spec).expect("runtime compile");
    match compiled {
        AbilityEffect::DefenderFireDelay {
            requires_critical, ..
        } => assert!(requires_critical),
        other => panic!("expected DefenderFireDelay, got {other:?}"),
    }
}

#[test]
fn chang_bridge_delay_on_crit_when_defender_hull_breach_active() {
    let crew = crew_with_defender_hull_breach(resolve_crew(
        "kirk-1323b6",
        &["chang-ecc238".to_string()],
        5,
    ));
    let mut saw_delay = false;
    for seed in 0..500_u64 {
        let mut setup = setup_fight(&crew, seed, 6, ShipType::Battleship, true, false);
        setup.attacker.crit_chance = 1.0;
        let result = simulate_combat_from_setup(&setup, seed);
        if delay_events(&result.events).iter().any(|e| {
            event_bool(e, "triggered")
                && (event_phase(e) == "attack" || event_phase(e) == "shield_break")
        }) {
            saw_delay = true;
            break;
        }
    }
    assert!(
        saw_delay,
        "Chang should delay defender fire on crit vs breached target when it procs"
    );
}

#[test]
fn kuron_captain_combat_start_shots_bonus_increases_damage() {
    let baseline = resolve_crew("kirk-1323b6", &[], 1);
    let with_kuron = resolve_crew("kuron-15cda2", &[], 1);
    let kuron_setup = setup_fight(&with_kuron, 0, 2, ShipType::Battleship, true, false);
    assert!(
        kuron_setup
            .combat_begin_filtered
            .iter()
            .any(|e| { matches!(e.effect, AbilityEffect::ShotsBonus { .. }) }),
        "Kuron should be active at combat_begin after setup"
    );

    let mut seed = 0_u64;
    let mut saw_proc = false;
    while seed < 500 {
        let base_setup = setup_fight(&baseline, seed, 2, ShipType::Battleship, true, false);
        let kuron_setup = setup_fight(&with_kuron, seed, 2, ShipType::Battleship, true, false);
        let base = simulate_combat_from_setup(&base_setup, seed);
        let kuron = simulate_combat_from_setup(&kuron_setup, seed);
        if shots_bonus_events(&kuron.events)
            .iter()
            .any(|e| event_phase(e) == "combat_begin" && event_bool(e, "triggered"))
        {
            saw_proc = true;
            assert!(
                kuron.total_damage >= base.total_damage,
                "seed {seed}: Kuron recharge should not reduce damage (base={}, kuron={})",
                base.total_damage,
                kuron.total_damage
            );
            break;
        }
        seed += 1;
    }
    assert!(
        saw_proc,
        "expected at least one Kuron combat_begin shots_bonus proc in 500 seeds"
    );
}

#[test]
fn vixis_captain_round_start_can_delay_defender_fire() {
    let crew = resolve_crew("vixis-9eec06", &[], 1);
    let mut saw_delay = false;
    for seed in 0..500_u64 {
        let setup = setup_fight(&crew, seed, 12, ShipType::Battleship, true, false);
        let result = simulate_combat_from_setup(&setup, seed);
        if delay_events(&result.events)
            .iter()
            .any(|e| event_bool(e, "triggered") && event_phase(e) == "round_start")
        {
            saw_delay = true;
            break;
        }
    }
    assert!(
        saw_delay,
        "Vixis round-start reload delay should proc at least once in 12 rounds across seeds"
    );
}

#[test]
fn pon_captain_compiles_enemy_delay_at_combat_begin() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
    let pon = file
        .officers
        .into_iter()
        .find(|o| o.id == "pon-a2ddd4")
        .expect("pon");
    let cap = pon.captain_ability.expect("captain");
    let effect = cap
        .effects
        .iter()
        .find(|e| e.tag.as_deref().is_some_and(|t| t.contains("enemy_delay")))
        .expect("reload");
    let spec = lcars_effect_to_combat_effect_spec(
        effect,
        "test:pon",
        "pon-a2ddd4",
        &cap.name,
        Some(3),
        None,
    )
    .expect("compile");
    assert_eq!(spec.modifier, AbilityModifierSpec::DefenderFireDelay);
    let (_, compiled, _) = compile_officer_combat_spec(&spec).expect("runtime");
    match compiled {
        AbilityEffect::DefenderFireDelay { delay_rounds, .. } => assert_eq!(delay_rounds, 3),
        other => panic!("expected DefenderFireDelay, got {other:?}"),
    }
}

#[test]
fn pon_captain_fires_delay_vs_player_defender_on_explorer() {
    let crew = resolve_crew("pon-a2ddd4", &[], 3);
    let mut saw_delay = false;
    for seed in 0..300_u64 {
        let setup = setup_fight(&crew, seed, 6, ShipType::Explorer, false, true);
        let result = simulate_combat_from_setup(&setup, seed);
        if delay_events(&result.events)
            .iter()
            .any(|e| event_bool(e, "triggered") && event_phase(e) == "combat_begin")
        {
            saw_delay = true;
            break;
        }
    }
    assert!(
        saw_delay,
        "Pon (Explorer vs player defender) should proc combat_begin defender fire delay in some seeds"
    );
}

#[test]
fn rom_captain_inactive_vs_npc_hostile_attacker() {
    let crew = resolve_crew("rom-621ae3", &[], 1);
    let setup = setup_fight(&crew, 42, 4, ShipType::Battleship, true, false);
    let result = simulate_combat_from_setup(&setup, 42);
    assert!(
        delay_events(&result.events).is_empty(),
        "Rom station/sentinel gates (EnemySentinel literal_false) block delay on default hostile attack"
    );
}

#[test]
fn pon_captain_inactive_vs_npc_hostile_attacker() {
    let crew = resolve_crew("pon-a2ddd4", &[], 3);
    let setup = setup_fight(&crew, 12_345, 3, ShipType::Explorer, true, false);
    let result = simulate_combat_from_setup(&setup, 12_345);
    assert!(
        delay_events(&result.events).is_empty(),
        "Pon gates (SelfDefending, EnemyPlayer) should block delay in default hostile attack sim"
    );
}

#[test]
fn rom_captain_compiles_one_round_combat_begin_delay() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
    let rom = file
        .officers
        .into_iter()
        .find(|o| o.id == "rom-621ae3")
        .expect("rom");
    let cap = rom.captain_ability.expect("captain");
    let effect = cap
        .effects
        .iter()
        .find(|e| e.tag.as_deref().is_some_and(|t| t.contains("enemy_delay")))
        .expect("reload");
    let spec = lcars_effect_to_combat_effect_spec(
        effect,
        "test:rom",
        "rom-621ae3",
        &cap.name,
        Some(1),
        None,
    )
    .expect("compile");
    match compile_officer_combat_spec(&spec).expect("runtime").1 {
        AbilityEffect::DefenderFireDelay { delay_rounds, .. } => assert_eq!(delay_rounds, 1),
        other => panic!("expected DefenderFireDelay, got {other:?}"),
    }
}

#[test]
fn ortegas_bridge_compiles_round_start_enemy_delay_from_canonical() {
    let Ok(file) = build_officer_model_file_default() else {
        return;
    };
    let ortegas = file
        .officers
        .into_iter()
        .find(|o| o.id == "strike-team-ortegas-d9df30")
        .expect("ortegas");
    let bridge = ortegas.bridge_ability.expect("bridge");
    let effect = bridge
        .effects
        .iter()
        .find(|e| e.tag.as_deref().is_some_and(|t| t.contains("enemy_delay")))
        .expect("reload");
    assert_eq!(
        effect.trigger.as_deref(),
        Some("on_round_start"),
        "canonical RoundStart → on_round_start (not on_attack)"
    );
    let spec = lcars_effect_to_combat_effect_spec(
        effect,
        "test:ortegas:reload",
        "strike-team-ortegas-d9df30",
        &bridge.name,
        Some(5),
        None,
    )
    .expect("compile");
    assert_eq!(spec.modifier, AbilityModifierSpec::DefenderFireDelay);
    let (_, compiled, _) = compile_officer_combat_spec(&spec).expect("runtime");
    match compiled {
        AbilityEffect::DefenderFireDelay { delay_rounds, .. } => assert_eq!(delay_rounds, 1),
        other => panic!("expected DefenderFireDelay, got {other:?}"),
    }
}

#[test]
fn ortegas_bridge_round_start_delay_vs_player_defender() {
    let crew = resolve_crew(
        "kirk-1323b6",
        &["strike-team-ortegas-d9df30".to_string()],
        5,
    );
    let mut saw_delay = false;
    for seed in 0..200_u64 {
        let setup = setup_fight(&crew, seed, 8, ShipType::Battleship, false, true);
        let result = simulate_combat_from_setup(&setup, seed);
        if delay_events(&result.events)
            .iter()
            .any(|e| event_bool(e, "triggered") && event_phase(e) == "round_start")
        {
            saw_delay = true;
            break;
        }
    }
    assert!(
        saw_delay,
        "Ortegas bridge reload should proc vs player defender on BB in some seeds"
    );
}
