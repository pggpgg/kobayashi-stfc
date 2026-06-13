//! Golden-master guard for the engine decomposition (roadmap task 12, step 0).
//!
//! The other safety nets cannot police a bit-identical refactor: calibration drift fixtures
//! assert tolerance *bands*, and the same-seed property test compares a binary against itself —
//! neither detects a refactor that deterministically changes behavior. This harness runs a fixed
//! battery of scenarios × seeds through the public combat entry point and compares every
//! [`SimulationResult`] field against a **committed** fixture captured before the refactor began.
//!
//! Regenerate intentionally (e.g. after a deliberate engine behavior change):
//! ```sh
//! KOBAYASHI_BLESS=1 cargo test --test engine_golden_master_tests
//! ```
//! …then review the fixture diff like any other behavior change.
//!
//! Floats are compared with 1e-9 relative tolerance instead of bitwise equality: `powf` may
//! differ by ULPs across libm implementations (macOS/ARM vs CI/x86), and `round_f64` (6 decimals)
//! does not fully quantize large magnitudes. 1e-9 is far below any real behavior change (an RNG
//! reorder or formula edit shifts values by orders of magnitude more) while absorbing platform
//! noise. Integers, bools, and strings must match exactly.

use std::path::PathBuf;

use kobayashi::combat::{
    simulate_combat_with_defender_faction_and_defender_crew, Ability, AbilityClass, AbilityEffect,
    AttackerStats, Combatant, CrewConfiguration, CrewSeat, CrewSeatContext, DefenderStats,
    HostileMitigationParams, OpponentFactionTag, ShipType, SimulationConfig, TimingWindow,
    TraceMode, WeaponStats, NO_EXPLICIT_CONTRIBUTION_BATCH,
};
use serde_json::{json, Value};

const SEEDS: [u64; 4] = [7, 42, 1337, 9_007_199_254_740_993];
const FIXTURE_RELATIVE_PATH: &str = "tests/fixtures/golden_master/engine_results.json";
const FLOAT_REL_TOLERANCE: f64 = 1e-9;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_RELATIVE_PATH)
}

// ── scenario building blocks ──

fn base_attacker() -> Combatant {
    Combatant {
        id: "gm-attacker".to_string(),
        attack: 1_200.0,
        mitigation: 0.35,
        armor: 800.0,
        shield_deflection: 700.0,
        dodge: 600.0,
        pierce: 0.12,
        crit_chance: 0.2,
        crit_multiplier: 1.5,
        proc_chance: 0.1,
        proc_multiplier: 1.2,
        hull_health: 60_000.0,
        shield_health: 24_000.0,
        shield_mitigation: 0.8,
        ..Combatant::default()
    }
}

fn base_defender() -> Combatant {
    Combatant {
        id: "gm-defender".to_string(),
        attack: 900.0,
        mitigation: 0.4,
        pierce: 0.08,
        crit_chance: 0.1,
        crit_multiplier: 1.4,
        proc_chance: 0.05,
        proc_multiplier: 1.1,
        hull_health: 45_000.0,
        shield_health: 18_000.0,
        shield_mitigation: 0.8,
        ..Combatant::default()
    }
}

fn config(rounds: u32, seed: u64) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed,
        trace_mode: TraceMode::Off,
        ..SimulationConfig::default()
    }
}

fn no_crew() -> CrewConfiguration {
    CrewConfiguration { seats: Vec::new() }
}

fn seat(
    seat: CrewSeat,
    name: &str,
    timing: TimingWindow,
    effect: AbilityEffect,
) -> CrewSeatContext {
    CrewSeatContext {
        seat,
        ability: Ability {
            name: name.to_string(),
            class: AbilityClass::CaptainManeuver,
            timing,
            boostable: true,
            effect,
            condition: None,
        },
        boosted: false,
        officer_id: None,
        contribution_batch: NO_EXPLICIT_CONTRIBUTION_BATCH,
    }
}

fn crew(seats: Vec<CrewSeatContext>) -> CrewConfiguration {
    CrewConfiguration { seats }
}

/// One golden scenario: everything the entry point consumes, under a stable name.
struct Scenario {
    name: &'static str,
    attacker: Combatant,
    defender: Combatant,
    rounds: u32,
    attacker_crew: CrewConfiguration,
    defender_crew: CrewConfiguration,
    defender_faction: OpponentFactionTag,
    defender_ship_type: ShipType,
    attacker_ship_type: ShipType,
    defender_is_npc_hostile: bool,
    defender_is_player_ship: bool,
    config_overrides: fn(&mut SimulationConfig),
}

impl Scenario {
    fn new(name: &'static str) -> Self {
        Scenario {
            name,
            attacker: base_attacker(),
            defender: base_defender(),
            rounds: 3,
            attacker_crew: no_crew(),
            defender_crew: no_crew(),
            defender_faction: OpponentFactionTag::Unknown,
            defender_ship_type: ShipType::Battleship,
            attacker_ship_type: ShipType::Battleship,
            defender_is_npc_hostile: true,
            defender_is_player_ship: false,
            config_overrides: |_| {},
        }
    }
}

fn scenarios() -> Vec<Scenario> {
    let mut all = Vec::new();

    // 1. Plain scalar-attack exchange, the shortest common path.
    all.push(Scenario::new("scalar_baseline"));

    // 2. Multi-weapon on both sides: weapon sub-round loop + per-weapon overrides + counter-fire
    //    weapon indexing.
    let mut s = Scenario::new("multi_weapon_both_sides");
    s.attacker.weapons = vec![
        WeaponStats {
            attack: 500.0,
            shots: Some(2),
            pierce: Some(0.2),
            crit_chance: Some(0.3),
            ..WeaponStats::default()
        },
        WeaponStats {
            attack: 800.0,
            shots: Some(1),
            crit_multiplier: Some(2.0),
            ..WeaponStats::default()
        },
        WeaponStats {
            attack: 300.0,
            shots: Some(3),
            proc_chance: Some(0.4),
            proc_multiplier: Some(1.5),
            ..WeaponStats::default()
        },
    ];
    s.defender.weapons = vec![
        WeaponStats {
            attack: 450.0,
            shots: Some(2),
            ..WeaponStats::default()
        },
        WeaponStats {
            attack: 600.0,
            shots: Some(1),
            pierce: Some(0.15),
            ..WeaponStats::default()
        },
    ];
    s.rounds = 5;
    all.push(s);

    // 3. Crit/proc heavy: exercises the crit resolver and proc rolls densely (many RNG draws).
    let mut s = Scenario::new("crit_proc_heavy");
    s.attacker.crit_chance = 0.85;
    s.attacker.crit_multiplier = 3.5;
    s.attacker.proc_chance = 0.7;
    s.attacker.proc_multiplier = 2.0;
    s.defender.crit_chance = 0.6;
    s.defender.proc_chance = 0.5;
    s.rounds = 6;
    all.push(s);

    // 4. Shieldless defender: shield/hull split overflow path from round 1.
    let mut s = Scenario::new("shieldless_defender");
    s.defender.shield_health = 0.0;
    all.push(s);

    // 5. Dynamic hostile mitigation (per-shot mitigation_for_hostile + powf curve) with a morale
    //    crew so the morale-adjusted piercing branch fires.
    let mut s = Scenario::new("hostile_dynamic_mitigation_morale");
    s.defender.hostile_mitigation_params = Some(HostileMitigationParams {
        defender_stats: DefenderStats {
            armor: 1_500.0,
            shield_deflection: 1_200.0,
            dodge: 900.0,
        },
        base_attacker_stats: AttackerStats {
            armor_piercing: 1_000.0,
            shield_piercing: 1_100.0,
            accuracy: 950.0,
        },
        ship_type: ShipType::Explorer,
        mystery_mitigation_factor: 0.1,
        floor: 0.16,
        ceiling: 0.72,
    });
    s.attacker_ship_type = ShipType::Explorer;
    s.attacker_crew = crew(vec![seat(
        CrewSeat::Captain,
        "gm-morale",
        TimingWindow::RoundStart,
        AbilityEffect::Morale(1.0),
    )]);
    s.rounds = 5;
    all.push(s);

    // 6. Burning crew: status application, per-round burn tick, duration decrement.
    let mut s = Scenario::new("burning_crew");
    s.attacker_crew = crew(vec![seat(
        CrewSeat::Captain,
        "gm-burn",
        TimingWindow::AttackPhase,
        AbilityEffect::Burning {
            chance: 0.6,
            duration_rounds: 2,
        },
    )]);
    s.rounds = 6;
    all.push(s);

    // 7. Hull breach + attack multiplier crew: breach roll, crit-damage amplification window,
    //    and a static damage modifier through the accumulator.
    let mut s = Scenario::new("hull_breach_attack_mult_crew");
    s.attacker_crew = crew(vec![
        seat(
            CrewSeat::Captain,
            "gm-breach",
            TimingWindow::AttackPhase,
            AbilityEffect::HullBreach {
                chance: 0.5,
                duration_rounds: 2,
                requires_critical: false,
            },
        ),
        seat(
            CrewSeat::Bridge,
            "gm-attack-mult",
            TimingWindow::RoundStart,
            AbilityEffect::AttackMultiplier(0.25),
        ),
    ]);
    s.attacker.crit_chance = 0.5;
    s.rounds = 6;
    all.push(s);

    // 8. Regen crew: round-start shield regen + previous-round hull regen fraction (the
    //    gross-damage-last-round bookkeeping).
    let mut s = Scenario::new("regen_crew");
    s.attacker_crew = crew(vec![
        seat(
            CrewSeat::Captain,
            "gm-shield-regen",
            TimingWindow::RoundStart,
            AbilityEffect::ShieldRegen(0.05),
        ),
        seat(
            CrewSeat::Bridge,
            "gm-hull-prev-regen",
            TimingWindow::RoundStart,
            AbilityEffect::HullRegenPrevRoundFraction(0.3),
        ),
    ]);
    s.rounds = 8;
    all.push(s);

    // 9. Isolytic + apex stats on both sides plus end-of-round damage.
    let mut s = Scenario::new("isolytic_apex_end_of_round");
    s.attacker.isolytic_damage = 0.3;
    s.attacker.apex_shred = 500.0;
    s.attacker.end_of_round_damage = 250.0;
    s.defender.isolytic_defense = 0.15;
    s.defender.apex_barrier = 2_000.0;
    s.defender.isolytic_damage = 0.1;
    s.rounds = 5;
    all.push(s);

    // 10. Pierce bonus + proc-gated attack multiplier crew through the accumulator stacking path.
    let mut s = Scenario::new("pierce_proc_crew");
    s.attacker_crew = crew(vec![
        seat(
            CrewSeat::Captain,
            "gm-pierce",
            TimingWindow::AttackPhase,
            AbilityEffect::PierceBonus(0.15),
        ),
        seat(
            CrewSeat::Bridge,
            "gm-proc-mult",
            TimingWindow::AttackPhase,
            AbilityEffect::ProcAttackMultiplier {
                chance: 0.5,
                multiplier: 1.8,
            },
        ),
    ]);
    s.rounds = 5;
    all.push(s);

    // 11. Long fight: 100-round horizon, low damage both ways (round-limit termination path).
    let mut s = Scenario::new("long_soak_100_rounds");
    s.attacker.attack = 40.0;
    s.defender.attack = 30.0;
    s.attacker.hull_health = 500_000.0;
    s.defender.hull_health = 400_000.0;
    s.rounds = 100;
    all.push(s);

    // 12. Chain carry-over: pre-damaged attacker + temporary incoming shield mitigation bonus.
    let mut s = Scenario::new("chain_carry_over");
    s.config_overrides = |c| {
        c.initial_attacker_hull_damage = 20_000.0;
        c.incoming_shield_mitigation_bonus = 0.1;
        c.incoming_shield_mitigation_bonus_rounds = 2;
    };
    s.rounds = 6;
    all.push(s);

    // 13. PvP-shaped: defender is a player ship (not an NPC hostile), explorer vs interceptor,
    //     defender crew active (defender-side ability path).
    let mut s = Scenario::new("pvp_defender_player_crewed");
    s.defender_is_npc_hostile = false;
    s.defender_is_player_ship = true;
    s.defender_ship_type = ShipType::Explorer;
    s.attacker_ship_type = ShipType::Interceptor;
    s.defender_faction = OpponentFactionTag::Federation;
    s.defender_crew = crew(vec![seat(
        CrewSeat::Captain,
        "gm-def-shield-mit",
        TimingWindow::RoundStart,
        AbilityEffect::ShieldMitigationBonus(0.05),
    )]);
    s.rounds = 5;
    all.push(s);

    // 14. Overkill: massive alpha strike, kill inside round 1 (early-exit + kill path).
    let mut s = Scenario::new("overkill_round_one");
    s.attacker.attack = 10_000_000.0;
    s.rounds = 3;
    all.push(s);

    // 15. Trace-events enabled: SimulationResult serializes its `events` vec, so this scenario
    //     locks down the full per-event trace payload (keys + values), not just the final tallies
    //     — the Off-trace scenarios above cannot catch a regression in event contents. Multi-weapon
    //     attacker vs a thin shield so shield-break / damage-application events fire across weapons.
    let mut s = Scenario::new("trace_events_multi_weapon_shield_break");
    s.attacker.weapons = vec![
        WeaponStats {
            attack: 600.0,
            shots: Some(2),
            ..WeaponStats::default()
        },
        WeaponStats {
            attack: 900.0,
            shots: Some(1),
            ..WeaponStats::default()
        },
    ];
    s.defender.shield_health = 400.0;
    s.rounds = 4;
    s.config_overrides = |c| c.trace_mode = TraceMode::Events;
    all.push(s);

    all
}

fn run_scenario(s: &Scenario, seed: u64) -> Value {
    let mut cfg = config(s.rounds, seed);
    (s.config_overrides)(&mut cfg);
    let result = simulate_combat_with_defender_faction_and_defender_crew(
        &s.attacker,
        &s.defender,
        &cfg,
        &s.attacker_crew,
        s.defender_faction,
        s.defender_ship_type,
        s.attacker_ship_type,
        s.defender_is_npc_hostile,
        s.defender_is_player_ship,
        &s.defender_crew,
    );
    serde_json::to_value(&result).expect("SimulationResult serializes")
}

fn capture_all() -> Value {
    let mut entries = Vec::new();
    for s in scenarios() {
        for seed in SEEDS {
            entries.push(json!({
                "scenario": s.name,
                "seed": seed,
                "result": run_scenario(&s, seed),
            }));
        }
    }
    Value::Array(entries)
}

// ── tolerant recursive comparison ──

fn floats_close(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    let scale = a.abs().max(b.abs());
    diff <= FLOAT_REL_TOLERANCE * scale.max(1.0)
}

fn compare(path: &str, expected: &Value, actual: &Value, errors: &mut Vec<String>) {
    match (expected, actual) {
        (Value::Number(e), Value::Number(a)) => {
            let (e, a) = (
                e.as_f64().unwrap_or(f64::NAN),
                a.as_f64().unwrap_or(f64::NAN),
            );
            if !floats_close(e, a) {
                errors.push(format!("{path}: expected {e}, got {a}"));
            }
        }
        (Value::Array(e), Value::Array(a)) => {
            if e.len() != a.len() {
                errors.push(format!("{path}: length {} != {}", e.len(), a.len()));
                return;
            }
            for (i, (ev, av)) in e.iter().zip(a.iter()).enumerate() {
                compare(&format!("{path}[{i}]"), ev, av, errors);
            }
        }
        (Value::Object(e), Value::Object(a)) => {
            for key in e.keys().chain(a.keys().filter(|k| !e.contains_key(*k))) {
                match (e.get(key), a.get(key)) {
                    (Some(ev), Some(av)) => compare(&format!("{path}.{key}"), ev, av, errors),
                    (Some(_), None) => errors.push(format!("{path}.{key}: missing in actual")),
                    (None, Some(_)) => errors.push(format!("{path}.{key}: unexpected in actual")),
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => {
            if expected != actual {
                errors.push(format!("{path}: expected {expected}, got {actual}"));
            }
        }
    }
}

#[test]
fn golden_master_engine_results_match_fixture() {
    let path = fixture_path();
    let actual = capture_all();

    if std::env::var_os("KOBAYASHI_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).expect("create fixture dir");
        std::fs::write(&path, serde_json::to_string_pretty(&actual).unwrap())
            .expect("write fixture");
        eprintln!(
            "blessed {} entries into {}",
            actual.as_array().map_or(0, Vec::len),
            path.display()
        );
        return;
    }

    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "golden fixture missing at {} ({e}). Generate it from a known-good tree with:\n  \
             KOBAYASHI_BLESS=1 cargo test --test engine_golden_master_tests",
            path.display()
        )
    });
    let expected: Value = serde_json::from_str(&raw).expect("fixture parses");

    let (exp_arr, act_arr) = (
        expected.as_array().expect("fixture is array"),
        actual.as_array().expect("capture is array"),
    );
    assert_eq!(
        exp_arr.len(),
        act_arr.len(),
        "scenario/seed battery changed size — re-bless intentionally if scenarios were edited"
    );

    let mut errors = Vec::new();
    for (e, a) in exp_arr.iter().zip(act_arr.iter()) {
        let label = format!(
            "{}@seed={}",
            e.get("scenario").and_then(Value::as_str).unwrap_or("?"),
            e.get("seed").and_then(Value::as_u64).unwrap_or(0)
        );
        compare(&label, e, a, &mut errors);
    }
    assert!(
        errors.is_empty(),
        "golden-master mismatches (engine behavior changed — if intentional, re-bless and review \
         the fixture diff):\n{}",
        errors.join("\n")
    );
}

/// The battery itself must be deterministic for the fixture to be meaningful: capturing twice in
/// the same process yields identical JSON (bitwise, no tolerance).
#[test]
fn golden_master_capture_is_deterministic_in_process() {
    assert_eq!(capture_all(), capture_all());
}
