use std::env;
use std::fmt::Write as _;

use crate::combat::{simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode};
use crate::data::import::import_spocks_export;
use crate::data::validate::validate_officer_dataset;
use crate::optimizer::optimize_crew;
use crate::server;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Serve,
    Simulate,
    Optimize,
    Import,
    Validate,
}

pub fn parse_command(args: &[String]) -> Option<Command> {
    match args.get(1).map(String::as_str) {
        Some("serve") => Some(Command::Serve),
        Some("simulate") => Some(Command::Simulate),
        Some("optimize") => Some(Command::Optimize),
        Some("import") => Some(Command::Import),
        Some("validate") => Some(Command::Validate),
        _ => None,
    }
}

pub fn run_with_args(args: &[String]) -> i32 {
    match parse_command(args) {
        Some(Command::Serve) => handle_serve(),
        Some(Command::Simulate) => handle_simulate(args),
        Some(Command::Optimize) => handle_optimize(args),
        Some(Command::Import) => handle_import(args),
        Some(Command::Validate) => handle_validate(args),
        None => {
            eprintln!("usage: kobayashi <serve|simulate|optimize|import|validate>");
            2
        }
    }
}

fn handle_serve() -> i32 {
    let bind_addr = env::var("KOBAYASHI_BIND").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    match server::run_server(&bind_addr) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("server error: {err}");
            1
        }
    }
}

fn handle_simulate(args: &[String]) -> i32 {
    let rounds = parse_u32_arg(args.get(2), "rounds", 3);
    let seed = parse_u64_arg(args.get(3), "seed", 7);
    let as_table = args.iter().any(|arg| arg == "--table");

    let attacker = Combatant {
        id: "player".to_string(),
        attack: 120.0,
        mitigation: 0.1,
        pierce: 0.15,
    };
    let defender = Combatant {
        id: "hostile".to_string(),
        attack: 10.0,
        mitigation: 0.35,
        pierce: 0.0,
    };

    let result = simulate_combat(
        &attacker,
        &defender,
        SimulationConfig {
            rounds,
            seed,
            trace_mode: TraceMode::Events,
        },
        &CrewConfiguration::default(),
    );

    if as_table {
        println!("rounds\tseed\ttotal_damage\tevent_count");
        println!(
            "{}\t{}\t{:.6}\t{}",
            rounds,
            seed,
            result.total_damage,
            result.events.len()
        );
    } else {
        match serde_json::to_string_pretty(&result) {
            Ok(payload) => println!("{payload}"),
            Err(err) => {
                eprintln!("failed to serialize simulation result: {err}");
                return 1;
            }
        }
    }

    0
}

fn handle_optimize(args: &[String]) -> i32 {
    let ship = args.get(2).map(String::as_str).unwrap_or("enterprise");
    let hostile = args.get(3).map(String::as_str).unwrap_or("swarm");
    let sims = parse_u32_arg(args.get(4), "sim_count", 250);

    let ranked = optimize_crew(ship, hostile, sims);
    match serde_json::to_string_pretty(&ranked) {
        Ok(payload) => {
            println!("{payload}");
            0
        }
        Err(err) => {
            eprintln!("failed to serialize optimization result: {err}");
            1
        }
    }
}

fn handle_import(args: &[String]) -> i32 {
    let Some(path) = args.get(2) else {
        eprintln!("usage: kobayashi import <path-to-export.json>");
        return 2;
    };

    match import_spocks_export(path) {
        Ok(report) => {
            println!(
                "import complete: records={}, source='{}'",
                report.record_count, report.source_path
            );
            0
        }
        Err(err) => {
            eprintln!("import failed: {err}");
            1
        }
    }
}

fn handle_validate(args: &[String]) -> i32 {
    let path = args
        .get(2)
        .map(String::as_str)
        .unwrap_or("data/officers/officers.canonical.json");

    match validate_officer_dataset(path) {
        Ok(()) => {
            println!("validation passed: {path}");
            0
        }
        Err(issues) => {
            eprintln!("validation failed: {} issue(s)", issues.len());
            for issue in issues {
                eprintln!("- {issue}");
            }
            1
        }
    }
}

fn parse_u32_arg(raw: Option<&String>, name: &str, default: u32) -> u32 {
    raw.and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_else(|| {
            if let Some(value) = raw {
                eprintln!("invalid {name} '{value}', defaulting to {default}");
            }
            default
        })
}

fn parse_u64_arg(raw: Option<&String>, name: &str, default: u64) -> u64 {
    raw.and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_else(|| {
            if let Some(value) = raw {
                let mut msg = String::new();
                let _ = write!(
                    &mut msg,
                    "invalid {name} '{value}', defaulting to {default}"
                );
                eprintln!("{msg}");
            }
            default
        })
}
