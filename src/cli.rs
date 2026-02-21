use std::env;
use std::fmt::Write as _;

use crate::combat::{simulate_combat, Combatant, CrewConfiguration, SimulationConfig, TraceMode};
use crate::data::import::import_spocks_export;
use crate::data::validate::{validate_officer_dataset, ValidationSeverity};
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
                "import summary: total={} matched={} unresolved={} conflicts={} output='{}'",
                report.total_records,
                report.matched_records,
                report.unresolved.len(),
                report.conflict_records,
                report.output_path
            );
            if report.has_critical_failures() {
                eprintln!(
                    "import failed with critical issues: unresolved={} conflicts={}",
                    report.unresolved.len(),
                    report.conflicts.len()
                );
                1
            } else {
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
        .get(2)
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
