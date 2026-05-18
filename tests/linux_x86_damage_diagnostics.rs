//! Diagnostic harness for the Linux-x86_64-only `total_damage=0` regression.
//!
//! Background: multiple tests (`tests/calibration_drift_tests.rs::drift_*`,
//! `tests/combat_round_timing_tests.rs`) report `total_damage=0` on the Ubuntu CI runner
//! while passing locally on macOS (dev + release). The failures share a pattern: outbound
//! attacker damage is zero across all rounds, despite valid weapons / attacker.attack and
//! a defender with finite hull and no instant-loss tags. See FIXMEs in those files.
//!
//! Strategy: build up the outbound-damage call chain from the smallest possible synthetic
//! setup, asserting at each step. Each test panic message embeds the full
//! `SimulationResult` so the CI log reveals where macOS and Linux diverge.
//!
//! Run order on CI runs every test in this file (none are `#[ignore]`d). Tests are named
//! `linux_diag_<step>_<what>` so they run alphabetically in a known order.

use kobayashi::combat::{
    effective_shots_for_weapon, serialize_events_json, simulate_combat,
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, OpponentFactionTag, ShipType,
    SimulationConfig, SimulationResult, TimingWindow, TraceMode, WeaponStats,
    NO_EXPLICIT_CONTRIBUTION_BATCH,
};

fn bare_attacker(attack: f64, weapons: Vec<WeaponStats>) -> Combatant {
    Combatant {
        id: "diag_attacker".to_string(),
        attack,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 10_000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons,
        hostile_mitigation_params: None,
    }
}

fn bare_defender(hull: f64) -> Combatant {
    Combatant {
        id: "diag_defender".to_string(),
        attack: 0.0,
        mitigation: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
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
        weapons: vec![],
        hostile_mitigation_params: None,
    }
}

fn bare_config(rounds: u32, seed: u64) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed,
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
        emit_state_snapshots: false,
    }
}

fn fmt_result(label: &str, r: &SimulationResult) -> String {
    format!(
        "{label}: total_damage={td} rounds={rs} attacker_won={aw} \
         attacker_hull_remaining={ahr} defender_hull_remaining={dhr} \
         defender_shield_remaining={dsr}",
        td = r.total_damage,
        rs = r.rounds_simulated,
        aw = r.attacker_won,
        ahr = r.attacker_hull_remaining,
        dhr = r.defender_hull_remaining,
        dsr = r.defender_shield_remaining,
    )
}

// ============================================================================
// Step 10: Single weapon, single round, no crew, no NPC tags. Minimum
// outbound-damage smoke test.
// ============================================================================

#[test]
fn linux_diag_10_single_weapon_one_shot_kills_defender() {
    let a = bare_attacker(
        0.0,
        vec![WeaponStats {
            attack: 1000.0,
            shots: Some(1),
            ..Default::default()
        }],
    );
    let d = bare_defender(500.0);
    let cfg = bare_config(1, 1);
    let r = simulate_combat(&a, &d, &cfg, &CrewConfiguration::default());
    assert!(
        r.total_damage >= 500.0,
        "Single 1000-atk shot vs 500 hull, no mitigation — expected total_damage>=500. {}",
        fmt_result("step_10", &r)
    );
}

#[test]
fn linux_diag_11_single_weapon_multiple_shots_per_weapon() {
    let a = bare_attacker(
        0.0,
        vec![WeaponStats {
            attack: 100.0,
            shots: Some(3),
            ..Default::default()
        }],
    );
    let d = bare_defender(1_000_000.0); // way bigger than 1 round of damage
    let cfg = bare_config(1, 1);
    let r = simulate_combat(&a, &d, &cfg, &CrewConfiguration::default());
    assert!(
        r.total_damage >= 290.0,
        "1 weapon * 100 atk * 3 shots in 1 round — expected ~300. {}",
        fmt_result("step_11", &r)
    );
}

#[test]
fn linux_diag_12_two_weapons_one_shot_each() {
    let a = bare_attacker(
        0.0,
        vec![
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                ..Default::default()
            },
            WeaponStats {
                attack: 100.0,
                shots: Some(1),
                ..Default::default()
            },
        ],
    );
    let d = bare_defender(1_000_000.0);
    let cfg = bare_config(1, 1);
    let r = simulate_combat(&a, &d, &cfg, &CrewConfiguration::default());
    assert!(
        r.total_damage >= 190.0,
        "2 weapons * 100 atk * 1 shot — expected ~200. {}",
        fmt_result("step_12", &r)
    );
}

// ============================================================================
// Step 20: Empty-weapons path: attacker.attack as base. This is what
// drift_conqueror_borg_beam_suppressed exercises.
// ============================================================================

#[test]
fn linux_diag_20_no_weapons_uses_attacker_attack() {
    let a = bare_attacker(500.0, vec![]);
    let d = bare_defender(1_000_000.0);
    let cfg = bare_config(1, 1);
    let r = simulate_combat(&a, &d, &cfg, &CrewConfiguration::default());
    assert!(
        r.total_damage > 0.0,
        "attacker.attack=500 with empty weapons vec — expected positive damage. {}",
        fmt_result("step_20", &r)
    );
}

#[test]
fn linux_diag_21_no_weapons_four_rounds_to_kill_2000_hull_at_500_attack() {
    // Reproduces the drift_conqueror_borg_beam_suppressed shape (sans hostile tag mask):
    // 500 atk / 2000 hull => 4 rounds (assuming attacker.attack acts as single-shot per round).
    let a = bare_attacker(500.0, vec![]);
    let d = bare_defender(2000.0);
    let cfg = bare_config(50, 42);
    let r = simulate_combat(&a, &d, &cfg, &CrewConfiguration::default());
    assert!(
        r.total_damage >= 1980.0 && r.total_damage <= 2020.0,
        "500 atk vs 2000 hull, no mitigation — expected ~2000 damage in ~4 rounds. {}",
        fmt_result("step_21", &r)
    );
    assert!(
        r.rounds_simulated <= 5,
        "Should finish in ~4 rounds, not run to the 50-round limit. {}",
        fmt_result("step_21", &r)
    );
}

// ============================================================================
// Step 30: With NPC-hostile tag mask (Conqueror Borg Obliterator) and
// attacker.id="borg_sphere" (hull-identity beam suppression). This is the exact
// drift_conqueror_borg_beam_suppressed shape.
// ============================================================================

#[test]
fn linux_diag_30_borg_sphere_hull_id_suppresses_obliterator_instant_loss() {
    let mut a = bare_attacker(500.0, vec![]);
    a.id = "borg_sphere".to_string();
    a.hull_health = 5000.0;
    let d = bare_defender(2000.0);
    let mut cfg = bare_config(50, 42);
    cfg.defender_hostile_tag_mask = 4; // HOSTILE_TAG_MASK_CONQUEROR_BORG_OBLITERATOR

    // Explicit defender_is_npc_hostile=true (matches drift fixture).
    let r = simulate_combat_with_defender_faction_and_defender_crew(
        &a,
        &d,
        &cfg,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    assert!(
        r.total_damage >= 1980.0,
        "borg_sphere hull-id should suppress Obliterator instant-loss; combat should proceed and deal damage. {}",
        fmt_result("step_30", &r)
    );
}

#[test]
fn linux_diag_31_non_borg_sphere_id_does_get_instant_lossed_by_obliterator() {
    // Sanity check the opposite: a non-borg-sphere attacker SHOULD instant-loss vs Obliterator.
    let a = bare_attacker(500.0, vec![]);
    let d = bare_defender(2000.0);
    let mut cfg = bare_config(50, 42);
    cfg.defender_hostile_tag_mask = 4;
    let r = simulate_combat_with_defender_faction_and_defender_crew(
        &a,
        &d,
        &cfg,
        &CrewConfiguration::default(),
        OpponentFactionTag::Unknown,
        ShipType::Battleship,
        ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    // Instant loss: rounds_simulated should be 0 and attacker should NOT win.
    assert_eq!(
        r.rounds_simulated,
        0,
        "non-borg-sphere should be instant-lossed vs Obliterator. {}",
        fmt_result("step_31", &r)
    );
    assert!(
        !r.attacker_won,
        "non-borg-sphere should not win vs Obliterator. {}",
        fmt_result("step_31", &r)
    );
}

// ============================================================================
// Step 40: Synthetic crew effects. This is what the combat_round_timing tests
// exercise.
// ============================================================================

fn crew_with_round_start_attack_multiplier(delta: f64) -> CrewConfiguration {
    CrewConfiguration {
        seats: vec![CrewSeatContext {
            seat: CrewSeat::Captain,
            ability: Ability {
                name: "diag_round_start_atk_mult".to_string(),
                class: AbilityClass::CaptainManeuver,
                timing: TimingWindow::RoundStart,
                boostable: false,
                effect: AbilityEffect::AttackMultiplier(delta),
                condition: None,
            },
            boosted: false,
            officer_id: None,
            contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
        }],
    }
}

#[test]
fn linux_diag_40_round_start_attack_multiplier_increases_damage() {
    let a = bare_attacker(
        0.0,
        vec![WeaponStats {
            attack: 100.0,
            shots: Some(1),
            ..Default::default()
        }],
    );
    let d = bare_defender(1_000_000.0);
    let cfg = bare_config(1, 1);
    let baseline = simulate_combat(&a, &d, &cfg, &CrewConfiguration::default());
    let boosted = simulate_combat(&a, &d, &cfg, &crew_with_round_start_attack_multiplier(1.0));
    assert!(
        boosted.total_damage > baseline.total_damage + 50.0,
        "RoundStart AttackMultiplier(+1.0) should ~double damage. baseline: {} boosted: {}",
        fmt_result("step_40_baseline", &baseline),
        fmt_result("step_40_boosted", &boosted),
    );
}

// ============================================================================
// Step 50: Trace dumps. simulate_combat returns total_damage=0 on Linux but
// rounds_simulated=1 — the loop runs but nothing happens. Capture trace events
// to see WHICH internal phases fire on Linux vs not.
// ============================================================================

#[test]
fn linux_diag_50_trace_single_weapon_one_shot() {
    let a = bare_attacker(
        0.0,
        vec![WeaponStats {
            attack: 1000.0,
            shots: Some(1),
            ..Default::default()
        }],
    );
    let d = bare_defender(500.0);
    let mut cfg = bare_config(1, 1);
    cfg.trace_mode = TraceMode::Events;
    let r = simulate_combat(&a, &d, &cfg, &CrewConfiguration::default());

    // Always-panic test: dump event stream so the CI log shows exactly what fires.
    let events_json = serialize_events_json(&r.events).unwrap_or_else(|e| format!("ERR: {e}"));
    let event_types: Vec<String> = r
        .events
        .iter()
        .map(|e| format!("{}@{}", e.event_type, e.phase))
        .collect();
    panic!(
        "ALWAYS-DUMP step_50: {result}\nevent_types={event_types:?}\nevents_json={events_json}",
        result = fmt_result("step_50", &r),
    );
}

// ============================================================================
// Step 60: Exported damage helpers. If these return wrong values on Linux, the
// bug is below the engine — in the basic math.
// ============================================================================

#[test]
fn linux_diag_60_effective_shots_for_weapon() {
    let r0 = effective_shots_for_weapon(1, 0.0);
    let r1 = effective_shots_for_weapon(3, 0.0);
    let r2 = effective_shots_for_weapon(2, 0.5);
    panic!("ALWAYS-DUMP step_60: effective_shots(1, 0.0)={r0}, (3, 0.0)={r1}, (2, 0.5)={r2}");
}

// ============================================================================
// Step 70: Walk the structs that simulate_combat receives — confirm what Linux
// sees vs what we pass in. (Linux build could be deserialising serde defaults
// or padding the struct differently somehow.)
// ============================================================================

#[test]
fn linux_diag_70_combatant_field_observability() {
    let a = bare_attacker(
        500.0,
        vec![WeaponStats {
            attack: 1000.0,
            shots: Some(3),
            pierce: Some(0.1),
            crit_chance: Some(0.5),
            crit_multiplier: Some(2.0),
            proc_chance: Some(0.2),
            proc_multiplier: Some(3.0),
        }],
    );
    let w = &a.weapons[0];
    panic!(
        "ALWAYS-DUMP step_70: \
         attacker.attack={atk}, weapons.len={wlen}, \
         w0.attack={watk}, w0.shots={ws:?}, w0.pierce={wp:?}, \
         w0.crit_chance={wcc:?}, w0.crit_multiplier={wcm:?}, \
         w0.proc_chance={wpc:?}, w0.proc_multiplier={wpm:?}",
        atk = a.attack,
        wlen = a.weapons.len(),
        watk = w.attack,
        ws = w.shots,
        wp = w.pierce,
        wcc = w.crit_chance,
        wcm = w.crit_multiplier,
        wpc = w.proc_chance,
        wpm = w.proc_multiplier,
    );
}

#[test]
fn linux_diag_41_round_start_attack_multiplier_with_empty_weapons() {
    // The drift_research_weapon_damage_* fixtures use attacker.attack=0 with a single weapon.
    // The combat_round_timing apex_shred test uses attacker.attack=200 with empty weapons.
    // Cover both combos.
    let a = bare_attacker(200.0, vec![]);
    let d = bare_defender(1_000_000.0);
    let cfg = bare_config(1, 1);
    let baseline = simulate_combat(&a, &d, &cfg, &CrewConfiguration::default());
    let boosted = simulate_combat(&a, &d, &cfg, &crew_with_round_start_attack_multiplier(1.0));
    assert!(
        baseline.total_damage > 0.0,
        "baseline w/ attacker.attack=200, empty weapons — expected positive damage. {}",
        fmt_result("step_41_baseline", &baseline),
    );
    assert!(
        boosted.total_damage > baseline.total_damage + 50.0,
        "+1.0 attack mult should ~double damage. baseline: {} boosted: {}",
        fmt_result("step_41_baseline", &baseline),
        fmt_result("step_41_boosted", &boosted),
    );
}
