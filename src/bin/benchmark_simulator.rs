//! Run simulator benchmark and optionally append one line to a log file for trend tracking.
//!
//! Usage:
//!   cargo run --release --bin benchmark_simulator
//!   cargo run --release --bin benchmark_simulator -- --log
//!
//! --log  Append one row to benchmark_log.csv (date, combats_per_sec, combats_per_min, rounds_per_sec, rounds_per_combat).

use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;

use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode,
};

fn main() {
    let log = std::env::args().any(|a| a == "--log");

    let attacker = Combatant {
        id: "attacker".to_string(),
        attack: 500.0,
        mitigation: 0.0,
        pierce: 200.0,
        crit_chance: 0.1,
        crit_multiplier: 1.5,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let defender = Combatant {
        id: "defender".to_string(),
        attack: 0.0,
        mitigation: 300.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 800.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let rounds_per_combat = 100u32;
    let config = SimulationConfig {
        rounds: rounds_per_combat,
        seed: 7,
        trace_mode: TraceMode::Off,
    };
    let crew = CrewConfiguration::default();

    // Run for at least this long or this many combats
    const MIN_DURATION_MS: u64 = 2000;
    const MIN_COMBATS: u32 = 500;

    let start = Instant::now();
    let mut combats: u32 = 0;
    while start.elapsed().as_millis() < MIN_DURATION_MS as u128 || combats < MIN_COMBATS {
        let _ = simulate_combat(&attacker, &defender, config, &crew);
        combats += 1;
    }
    let elapsed_secs = start.elapsed().as_secs_f64();

    let combats_per_sec = combats as f64 / elapsed_secs;
    let combats_per_min = combats_per_sec * 60.0;
    let rounds_per_sec = combats_per_sec * (rounds_per_combat as f64);

    println!("Simulator benchmark ({} rounds/combat):", rounds_per_combat);
    println!("  Combats:     {}", combats);
    println!("  Duration:    {:.2} s", elapsed_secs);
    println!("  Combats/s:   {:.2}", combats_per_sec);
    println!("  Combats/min: {:.2}", combats_per_min);
    println!("  Rounds/s:    {:.2}", rounds_per_sec);

    if log {
        let date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
        let line = format!(
            "{},{:.4},{:.4},{:.4},{}\n",
            date, combats_per_sec, combats_per_min, rounds_per_sec, rounds_per_combat
        );
        let path = "benchmark_log.csv";
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open benchmark_log.csv for append");
        if file.metadata().map(|m| m.len() == 0).unwrap_or(true) {
            let _ = file.write_all(
                b"date,combats_per_sec,combats_per_min,rounds_per_sec,rounds_per_combat\n",
            );
        }
        file.write_all(line.as_bytes())
            .expect("write benchmark_log.csv");
        file.flush().expect("flush benchmark_log.csv");
        println!("Appended to {}", path);
    }
}
