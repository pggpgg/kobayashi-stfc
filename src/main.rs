use std::env;
use std::process;

use kobayashi::combat::{
    simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode,
};
use kobayashi::data::import::{import_roster_csv, import_spocks_export};
use kobayashi::data::validate::{validate_officer_dataset, ValidationSeverity};
use kobayashi::server;

#[derive(Debug, Clone, Copy)]
enum Command {
    Serve,
    Simulate,
    Optimize,
    Import,
    Validate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OptimizeCliArgs {
    ship: String,
    hostile: String,
    sims: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct SimulateCliArgs {
    attacker_id: String,
    attacker_attack: f64,
    attacker_pierce: f64,
    defender_id: String,
    defender_mitigation: f64,
    rounds: u32,
    seed: u64,
    trace_events: bool,
}

fn parse_command() -> Option<Command> {
    match env::args().nth(1).as_deref() {
        Some("serve") => Some(Command::Serve),
        Some("simulate") => Some(Command::Simulate),
        Some("optimize") => Some(Command::Optimize),
        Some("import") => Some(Command::Import),
        Some("validate") => Some(Command::Validate),
        _ => None,
    }
}

fn parse_optimize_args(args: &[String]) -> Result<OptimizeCliArgs, String> {
    if args.len() == 3 && !args[0].starts_with("--") {
        return Ok(OptimizeCliArgs {
            ship: args[0].clone(),
            hostile: args[1].clone(),
            sims: args[2]
                .parse::<u32>()
                .map_err(|_| "sims must be a positive integer".to_string())?,
        });
    }

    let mut ship = "saladin".to_string();
    let mut hostile = "explorer_30".to_string();
    let mut sims: u32 = 5_000;

    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--ship" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --ship".to_string())?;
                ship = value.clone();
                idx += 2;
            }
            "--hostile" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --hostile".to_string())?;
                hostile = value.clone();
                idx += 2;
            }
            "--sims" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --sims".to_string())?;
                sims = value
                    .parse::<u32>()
                    .map_err(|_| "--sims must be a positive integer".to_string())?;
                idx += 2;
            }
            unknown => return Err(format!("unknown optimize argument: {unknown}")),
        }
    }

    Ok(OptimizeCliArgs {
        ship,
        hostile,
        sims,
    })
}

fn parse_simulate_args(args: &[String]) -> Result<SimulateCliArgs, String> {
    if args.len() == 2 && !args[0].starts_with("--") {
        return Ok(SimulateCliArgs {
            attacker_id: "attacker".to_string(),
            attacker_attack: 120.0,
            attacker_pierce: 0.15,
            defender_id: "defender".to_string(),
            defender_mitigation: 0.35,
            rounds: args[0]
                .parse::<u32>()
                .map_err(|_| "rounds must be a positive integer".to_string())?,
            seed: args[1]
                .parse::<u64>()
                .map_err(|_| "seed must be a positive integer".to_string())?,
            trace_events: true,
        });
    }

    let mut parsed = SimulateCliArgs {
        attacker_id: "attacker".to_string(),
        attacker_attack: 120.0,
        attacker_pierce: 0.15,
        defender_id: "defender".to_string(),
        defender_mitigation: 0.35,
        rounds: 3,
        seed: 7,
        trace_events: false,
    };

    let mut idx = 0;
    while idx < args.len() {
        match args[idx].as_str() {
            "--attacker-id" => {
                parsed.attacker_id = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --attacker-id".to_string())?
                    .clone();
                idx += 2;
            }
            "--attacker-attack" => {
                parsed.attacker_attack = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --attacker-attack".to_string())?
                    .parse::<f64>()
                    .map_err(|_| "--attacker-attack must be a number".to_string())?;
                idx += 2;
            }
            "--attacker-pierce" => {
                parsed.attacker_pierce = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --attacker-pierce".to_string())?
                    .parse::<f64>()
                    .map_err(|_| "--attacker-pierce must be a number".to_string())?;
                idx += 2;
            }
            "--defender-id" => {
                parsed.defender_id = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --defender-id".to_string())?
                    .clone();
                idx += 2;
            }
            "--defender-mitigation" => {
                parsed.defender_mitigation = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --defender-mitigation".to_string())?
                    .parse::<f64>()
                    .map_err(|_| "--defender-mitigation must be a number".to_string())?;
                idx += 2;
            }
            "--rounds" => {
                parsed.rounds = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --rounds".to_string())?
                    .parse::<u32>()
                    .map_err(|_| "--rounds must be a positive integer".to_string())?;
                idx += 2;
            }
            "--seed" => {
                parsed.seed = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --seed".to_string())?
                    .parse::<u64>()
                    .map_err(|_| "--seed must be a positive integer".to_string())?;
                idx += 2;
            }
            "--trace-events" => {
                parsed.trace_events = true;
                idx += 1;
            }
            unknown => return Err(format!("unknown simulate argument: {unknown}")),
        }
    }

    Ok(parsed)
}

fn optimize_command(args: &[String]) -> Result<(), String> {
    let parsed = parse_optimize_args(args)?;
    let body = serde_json::json!({
        "ship": parsed.ship,
        "hostile": parsed.hostile,
        "sims": parsed.sims,
    })
    .to_string();

    let payload = server::api::optimize_payload(&body)
        .map_err(|err| format!("failed to build optimize response: {err}"))?;
    let response: serde_json::Value =
        serde_json::from_str(&payload).map_err(|err| format!("invalid optimize payload: {err}"))?;

    println!(
        "{}",
        serde_json::to_string_pretty(&response["recommendations"])
            .map_err(|err| format!("failed to serialize recommendations: {err}"))?
    );
    Ok(())
}

fn simulate_command(args: &[String]) -> Result<(), String> {
    let parsed = parse_simulate_args(args)?;
    let attacker = Combatant {
        id: parsed.attacker_id,
        attack: parsed.attacker_attack,
        mitigation: 0.0,
        pierce: parsed.attacker_pierce,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
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
        id: parsed.defender_id,
        attack: 0.0,
        mitigation: parsed.defender_mitigation,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
    };
    let config = SimulationConfig {
        rounds: parsed.rounds,
        seed: parsed.seed,
        trace_mode: if parsed.trace_events {
            TraceMode::Events
        } else {
            TraceMode::Off
        },
    };

    let result = simulate_combat(&attacker, &defender, config, &CrewConfiguration::default());
    println!(
        "{}",
        serde_json::to_string_pretty(&result)
            .map_err(|err| format!("failed to serialize simulation result: {err}"))?
    );
    Ok(())
}

/// Roster files live here; a bare filename is resolved as rosters/<filename>.
const ROSTERS_DIR: &str = "rosters";

fn handle_import(args: &[String]) -> i32 {
    if args.len() != 1 {
        eprintln!("usage: kobayashi import <path>");
        eprintln!("  use a .txt file for your roster (comma-separated: name,tier,level), or a .json file for Spocks export");
        eprintln!("  roster files are usually in the '{ROSTERS_DIR}/' folder; a bare filename (e.g. my_roster.txt) is looked up there");
        return 2;
    }
    let raw = &args[0];
    let path = if raw.contains('/') || raw.contains('\\') {
        raw.clone()
    } else {
        format!("{ROSTERS_DIR}/{raw}")
    };
    let result = if path.ends_with(".txt") {
        import_roster_csv(&path)
    } else if path.ends_with(".json") {
        import_spocks_export(&path)
    } else {
        eprintln!("import expects a .txt file (roster) or .json file (Spocks export); got: {path}");
        return 2;
    };

    match result {
        Ok(report) => {
            println!(
                "import summary: source='{}' output='{}' total={} matched={} unmatched={} ambiguous={} duplicates={} conflicts={}",
                report.source_path,
                report.output_path,
                report.total_records,
                report.matched_records,
                report.unmatched_records,
                report.ambiguous_records,
                report.duplicate_records,
                report.conflict_records
            );

            if !report.unresolved.is_empty() {
                println!("\nunresolved entries:");
                for entry in &report.unresolved {
                    println!(
                        "- record[{}] name='{}' normalized='{}': {}",
                        entry.record_index, entry.input_name, entry.normalized_name, entry.reason
                    );
                }
            }

            if !report.conflicts.is_empty() {
                println!("\nconflicting imported states:");
                for conflict in &report.conflicts {
                    println!(
                        "- officer='{}' first_record={} conflicting_record={}",
                        conflict.canonical_officer_id,
                        conflict.first_record_index,
                        conflict.conflicting_record_index
                    );
                }
            }

            if report.has_critical_failures() {
                eprintln!(
                    "import failed with critical issues: unresolved={} conflicts={}",
                    report.unresolved.len(),
                    report.conflicts.len()
                );
                1
            } else {
                println!(
                    "import complete: persisted {} canonical roster entries",
                    report.roster_entries_written
                );
                0
            }
        }
        Err(err) => {
            eprintln!("import failed: {err}");
            1
        }
    }
}

fn handle_validate(args: &[String]) -> i32 {
    let path = args
        .first()
        .map(String::as_str)
        .unwrap_or("data/officers/officers.canonical.json");

    let report = match validate_officer_dataset(path) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("validation failed: {err}");
            return 1;
        }
    };

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == ValidationSeverity::Error)
        .collect();
    let warnings: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == ValidationSeverity::Warning)
        .collect();
    let infos: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == ValidationSeverity::Info)
        .collect();

    if !errors.is_empty() {
        eprintln!(
            "validation failed: errors={}, warnings={}, info={}",
            errors.len(),
            warnings.len(),
            infos.len()
        );
    } else {
        println!(
            "validation summary: errors={}, warnings={}, info={}",
            errors.len(),
            warnings.len(),
            infos.len()
        );
    }

    for (label, diagnostics) in [("error", errors), ("warning", warnings), ("info", infos)] {
        if diagnostics.is_empty() {
            continue;
        }

        println!("\n[{label}]");
        for diagnostic in diagnostics {
            println!("- {}: {}", diagnostic.context, diagnostic.message);
        }
    }

    if report.has_errors() {
        1
    } else {
        0
    }
}

fn print_usage() {
    eprintln!(
        "usage: kobayashi <serve|simulate|optimize|import|validate> [args]\n\
simulate: kobayashi simulate <rounds> <seed>\n\
  or kobayashi simulate --attacker-id <id> --attacker-attack <f64> --attacker-pierce <f64>\n\
                       --defender-id <id> --defender-mitigation <f64> --rounds <u32> --seed <u64> [--trace-events]\n\
optimize: kobayashi optimize <ship> <hostile> <sims>\n\
  or kobayashi optimize --ship <id> --hostile <id> --sims <u32>"
    );
}

fn main() {
    let command_args: Vec<String> = env::args().skip(2).collect();
    let mut exit_code = 0;

    match parse_command() {
        Some(Command::Serve) => {
            let bind_addr =
                env::var("KOBAYASHI_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
            if let Err(err) = server::run_server(&bind_addr) {
                eprintln!("server error: {err}");
                exit_code = 1;
            }
        }
        Some(Command::Simulate) => {
            if let Err(err) = simulate_command(&command_args) {
                eprintln!("simulate error: {err}");
                print_usage();
                exit_code = 2;
            }
        }
        Some(Command::Optimize) => {
            if let Err(err) = optimize_command(&command_args) {
                eprintln!("optimize error: {err}");
                print_usage();
                exit_code = 2;
            }
        }
        Some(Command::Import) => {
            exit_code = handle_import(&command_args);
        }
        Some(Command::Validate) => {
            exit_code = handle_validate(&command_args);
        }
        None => {
            print_usage();
            exit_code = 2;
        }
    }

    if exit_code != 0 {
        process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_optimize_args, parse_simulate_args};

    #[test]
    fn parse_optimize_args_reads_explicit_values() {
        let args = vec![
            "enterprise".to_string(),
            "swarm_32".to_string(),
            "9000".to_string(),
        ];
        let parsed = parse_optimize_args(&args).expect("parse should succeed");
        assert_eq!(parsed.ship, "enterprise");
        assert_eq!(parsed.hostile, "swarm_32");
        assert_eq!(parsed.sims, 9000);
    }

    #[test]
    fn parse_simulate_args_enables_trace_flag() {
        let args = vec!["5".to_string(), "99".to_string()];
        let parsed = parse_simulate_args(&args).expect("parse should succeed");
        assert_eq!(parsed.rounds, 5);
        assert_eq!(parsed.seed, 99);
        assert!(parsed.trace_events);
    }
}
