use std::env;
use std::process;

use kobayashi::combat::{
    default_percent_sensitivity_rows, format_sensitivity_tsv,
    simulate_combat_with_defender_faction, Combatant, CrewConfiguration, EnemyTypes,
    HostileMitigationBaseline, OpponentFactionTag, SimulationConfig, TraceMode, MITIGATION_CEILING,
    MITIGATION_FLOOR,
};
use kobayashi::data::import::{
    import_roster_csv_to, import_spocks_export_to, load_imported_battlelogs,
};
use kobayashi::data::loader::{defender_faction_for_cli_simulate, resolve_hostile, resolve_ship};
use kobayashi::data::profile::{apply_profile_to_attacker, load_profile, OfficerStatRuntimeBonus};
use kobayashi::data::profile_index::{
    ensure_profile_index_bootstrap, profile_data_dir, profile_path,
    prune_ephemeral_scenario_test_profiles, resolve_profile_id_for_api,
    sync_profile_index_with_disk, BATTLELOGS_IMPORTED, PROFILE_JSON, ROSTER_IMPORTED,
};
use kobayashi::data::validate::{validate_officer_dataset, ValidationSeverity};
use kobayashi::server;

#[derive(Debug, Clone, Copy)]
enum Command {
    Serve,
    Simulate,
    Optimize,
    Import,
    Validate,
    ValidateLog,
    GenerateLcars,
    MitigationSensitivity,
    Sensitivity,
    MorrisSensitivity,
    SobolSensitivity,
    Battlelogs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OptimizeCliArgs {
    ship: String,
    hostile: String,
    sims: u32,
    /// Optional cap on the number of candidate crews to evaluate.
    max_candidates: Option<u32>,
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
    /// `kobayashi simulate --defender-faction <slug>` (same slugs as LCARS / `OpponentFactionTag::from_data_slug`).
    defender_faction_slug: Option<String>,
    /// `kobayashi simulate --hostile <id|name level>` — faction from hostile record when slug not set.
    hostile_lookup: Option<String>,
}

fn parse_command() -> Option<Command> {
    match env::args().nth(1).as_deref() {
        Some("serve") => Some(Command::Serve),
        Some("simulate") => Some(Command::Simulate),
        Some("optimize") => Some(Command::Optimize),
        Some("import") => Some(Command::Import),
        Some("validate") => Some(Command::Validate),
        Some("validate-log") => Some(Command::ValidateLog),
        Some("generate-lcars") => Some(Command::GenerateLcars),
        Some("mitigation-sensitivity") => Some(Command::MitigationSensitivity),
        Some("sensitivity") => Some(Command::Sensitivity),
        Some("morris-sensitivity") => Some(Command::MorrisSensitivity),
        Some("sobol-sensitivity") => Some(Command::SobolSensitivity),
        Some("battlelogs") => Some(Command::Battlelogs),
        _ => None,
    }
}

fn parse_profile_arg(args: &[String]) -> Option<String> {
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == "--profile" {
            return args.get(idx + 1).cloned();
        }
        idx += 1;
    }
    None
}

fn parse_optimize_args(args: &[String]) -> Result<OptimizeCliArgs, String> {
    if args.len() == 3 && !args[0].starts_with("--") {
        return Ok(OptimizeCliArgs {
            ship: args[0].clone(),
            hostile: args[1].clone(),
            sims: args[2]
                .parse::<u32>()
                .map_err(|_| "sims must be a positive integer".to_string())?,
            max_candidates: None,
        });
    }

    let mut ship = "saladin".to_string();
    let mut hostile = "2918121098".to_string();
    let mut sims: u32 = 5_000;
    let mut max_candidates: Option<u32> = None;

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
            "--max-candidates" => {
                let value = args
                    .get(idx + 1)
                    .ok_or_else(|| "missing value for --max-candidates".to_string())?;
                max_candidates = Some(
                    value
                        .parse::<u32>()
                        .map_err(|_| "--max-candidates must be a positive integer".to_string())?,
                );
                idx += 2;
            }
            "--profile" => {
                idx += 2;
            }
            unknown => return Err(format!("unknown optimize argument: {unknown}")),
        }
    }

    Ok(OptimizeCliArgs {
        ship,
        hostile,
        sims,
        max_candidates,
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
            defender_faction_slug: None,
            hostile_lookup: None,
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
        defender_faction_slug: None,
        hostile_lookup: None,
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
            "--defender-faction" => {
                parsed.defender_faction_slug = Some(
                    args.get(idx + 1)
                        .ok_or_else(|| "missing value for --defender-faction".to_string())?
                        .clone(),
                );
                idx += 2;
            }
            "--hostile" => {
                parsed.hostile_lookup = Some(
                    args.get(idx + 1)
                        .ok_or_else(|| "missing value for --hostile".to_string())?
                        .clone(),
                );
                idx += 2;
            }
            "--profile" => {
                idx += 2;
            }
            unknown => return Err(format!("unknown simulate argument: {unknown}")),
        }
    }

    Ok(parsed)
}

fn optimize_command(args: &[String]) -> Result<(), String> {
    let parsed = parse_optimize_args(args)?;
    let profile_id = resolve_profile_id_for_api(parse_profile_arg(args).as_deref());

    let mut payload = serde_json::json!({
        "ship": parsed.ship,
        "hostile": parsed.hostile,
        "sims": parsed.sims,
    });
    if let Some(cap) = parsed.max_candidates {
        if let serde_json::Value::Object(ref mut map) = payload {
            map.insert("max_candidates".to_string(), serde_json::Value::from(cap));
        }
    }
    let body = payload.to_string();

    let registry = kobayashi::data::data_registry::DataRegistry::load()
        .map_err(|e| format!("Failed to load data registry: {e}"))?;
    let payload =
        server::api::optimize_payload(registry.as_ref(), &body, Some(profile_id.as_str()))
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
    let profile_id = resolve_profile_id_for_api(parse_profile_arg(args).as_deref());
    let profile_path_str = profile_path(&profile_id, PROFILE_JSON)
        .to_string_lossy()
        .to_string();
    let player_profile = load_profile(&profile_path_str);

    let owner_faction_owned = resolve_ship(&parsed.attacker_id).and_then(|s| s.faction);

    let attacker = apply_profile_to_attacker(
        Combatant {
            id: parsed.attacker_id,
            attack: parsed.attacker_attack,
            mitigation: 0.0,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: parsed.attacker_pierce,
            crit_chance: 0.0,
            crit_multiplier: 1.0,
            crit_damage_floor: 0.0,
            proc_chance: 0.0,
            proc_multiplier: 1.0,
            end_of_round_damage: 0.0,
            hull_health: 1000.0,
            shield_health: 0.0,
            shield_mitigation: 0.8,
            apex_barrier: 0.0,
            apex_shred: 0.0,
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            weapons: vec![],
            hostile_mitigation_params: None,
        },
        &player_profile,
        owner_faction_owned.as_deref(),
        OfficerStatRuntimeBonus::default(),
    );
    let defender = Combatant {
        id: parsed.defender_id,
        attack: 0.0,
        mitigation: parsed.defender_mitigation,
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
        hull_health: 1000.0,
        shield_health: 0.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };
    let config = SimulationConfig {
        rounds: parsed.rounds,
        seed: parsed.seed,
        trace_mode: if parsed.trace_events {
            TraceMode::Events
        } else {
            TraceMode::Off
        },
        initial_attacker_hull_damage: 0.0,
        weapon_damage_profile_additive_pool: None,
        profile_weapon_damage_fraction: 0.0,
        defender_hull_faction_id: 0,
        defender_hostile_tag_mask: 0,
        attacker_owner_faction: OpponentFactionTag::Unknown,
        engagement_enemy_types: EnemyTypes::default(),
        defender_level: None,
        attacker_roster_officer_ids: Vec::new(),
        incoming_shield_mitigation_bonus: 0.0,
        incoming_shield_mitigation_bonus_rounds: 0,
        emit_state_snapshots: false,
        crit_damage_reduction_perturb: 0.0,
    };

    let defender_faction = defender_faction_for_cli_simulate(
        parsed.defender_faction_slug.as_deref(),
        parsed.hostile_lookup.as_deref(),
    )?;
    let result = simulate_combat_with_defender_faction(
        &attacker,
        &defender,
        &config,
        &CrewConfiguration::default(),
        defender_faction,
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&result)
            .map_err(|err| format!("failed to serialize simulation result: {err}"))?
    );
    Ok(())
}

fn handle_import(args: &[String]) -> i32 {
    let raw = match args.first() {
        Some(s) if !s.starts_with("--") => s.clone(),
        _ => {
            eprintln!("usage: kobayashi import <path> [--profile <id>]");
            eprintln!("  use a .txt file for your roster (comma-separated: name,tier,level), or a .json file for Spocks export");
            eprintln!("  bare filename resolves under profiles/<profile>/ (default profile if --profile omitted)");
            return 2;
        }
    };
    let profile_id = resolve_profile_id_for_api(parse_profile_arg(args).as_deref());
    let path = if raw.contains('/') || raw.contains('\\') {
        raw.clone()
    } else {
        profile_data_dir(&profile_id)
            .join(&raw)
            .to_string_lossy()
            .into_owned()
    };
    let output_path = profile_path(&profile_id, ROSTER_IMPORTED)
        .to_string_lossy()
        .to_string();

    let result = if path.ends_with(".txt") {
        import_roster_csv_to(&path, &output_path)
    } else if path.ends_with(".json") {
        import_spocks_export_to(&path, &output_path)
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

fn validate_log_command(args: &[String]) -> Result<(), String> {
    let path = args
        .first()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "usage: kobayashi validate-log <path.json>".to_string())?;
    let json = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    let mut log = kobayashi::combat::parse_combat_log_json(&json)?;
    kobayashi::combat::hydrate_ingested_state_snapshots_from_values(&mut log);
    let outcome = kobayashi::combat::validate_canonical_timeline(&log);
    for w in &outcome.warnings {
        eprintln!("warning: {w}");
    }
    for e in &outcome.errors {
        eprintln!("error: {e}");
    }
    if !outcome.errors.is_empty() {
        return Err("timeline validation failed".to_string());
    }
    Ok(())
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

fn handle_generate_lcars(args: &[String]) -> i32 {
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("generate-lcars: cannot get current exe: {e}");
            return 1;
        }
    };
    let dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
    let candidates = ["generate_lcars.exe", "generate_lcars"];
    let bin = candidates
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.exists());
    match bin {
        Some(b) => run_generate_lcars_bin(&b, args),
        None => {
            eprintln!("generate-lcars: binary not found. Run: cargo build --bin generate_lcars");
            1
        }
    }
}

fn run_generate_lcars_bin(bin: &std::path::Path, args: &[String]) -> i32 {
    let mut cmd = process::Command::new(bin);
    cmd.args(args);
    match cmd.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("generate-lcars failed: {e}");
            1
        }
    }
}

fn mitigation_sensitivity_command(args: &[String]) -> Result<(), String> {
    let ship = args
        .first()
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "usage: kobayashi mitigation-sensitivity <ship> <hostile> [--delta-pct <f64>]"
                .to_string()
        })?;
    let hostile = args
        .get(1)
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "usage: kobayashi mitigation-sensitivity <ship> <hostile> [--delta-pct <f64>]"
                .to_string()
        })?;
    let mut delta_pct = 0.1_f64;
    let mut i = 2;
    while i < args.len() {
        if args[i] == "--delta-pct" {
            let v = args
                .get(i + 1)
                .ok_or_else(|| "--delta-pct requires a value".to_string())?;
            delta_pct = v
                .parse::<f64>()
                .map_err(|_| "delta-pct must be a number (e.g. 0.1 for +10%)".to_string())?;
            i += 2;
        } else {
            i += 1;
        }
    }

    let ship_rec = resolve_ship(ship).ok_or_else(|| format!("unknown ship '{ship}'"))?;
    let hostile_rec =
        resolve_hostile(hostile).ok_or_else(|| format!("unknown hostile '{hostile}'"))?;

    let attacker = ship_rec.to_attacker_stats();
    let defender = hostile_rec.to_defender_stats();
    let baseline = HostileMitigationBaseline {
        defender,
        attacker,
        ship_type: hostile_rec.ship_type_for_combat(),
        mystery_mitigation_factor: hostile_rec.mystery_mitigation_factor.unwrap_or(0.0),
        mitigation_floor: hostile_rec.mitigation_floor.unwrap_or(MITIGATION_FLOOR),
        mitigation_ceiling: hostile_rec.mitigation_ceiling.unwrap_or(MITIGATION_CEILING),
        defense_mitigation_bonus: 0.0,
    };
    let rows = default_percent_sensitivity_rows(&baseline, delta_pct);
    print!("{}", format_sensitivity_tsv(&rows));
    Ok(())
}

fn parse_battlelogs_flags(args: &[String]) -> bool {
    args.iter().any(|a| a == "--sample")
}

fn battlelogs_command(args: &[String]) -> Result<(), String> {
    let profile_id = resolve_profile_id_for_api(parse_profile_arg(args).as_deref());
    let path = profile_path(&profile_id, BATTLELOGS_IMPORTED);
    let path_str = path
        .to_str()
        .ok_or_else(|| "battlelogs path is not valid UTF-8".to_string())?;
    let Some(logs) = load_imported_battlelogs(path_str) else {
        println!("(no file or invalid JSON)\npath: {path_str}");
        return Ok(());
    };
    println!("path: {path_str}");
    println!("entries: {}", logs.len());
    if logs.is_empty() {
        return Ok(());
    }
    let mut type_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for entry in &logs {
        let key = entry
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("<no type>")
            .to_string();
        *type_counts.entry(key).or_insert(0) += 1;
    }
    let mut pairs: Vec<_> = type_counts.into_iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    println!("by `type` field:");
    for (k, n) in pairs {
        println!("  {k}: {n}");
    }
    if parse_battlelogs_flags(args) {
        let last = logs.last().expect("non-empty");
        let sample =
            serde_json::to_string_pretty(last).map_err(|e| format!("serialize sample: {e}"))?;
        println!("newest entry (last in file):\n{sample}");
    }
    Ok(())
}

fn parse_named_string_arg_main(args: &[String], name: &str) -> Option<String> {
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == name {
            return args.get(idx + 1).cloned();
        }
        idx += 1;
    }
    None
}

fn parse_csv_string_arg_main(args: &[String], name: &str) -> Vec<String> {
    parse_named_string_arg_main(args, name)
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn sensitivity_command(args: &[String]) -> Result<(), String> {
    use kobayashi::data::data_registry::DataRegistry;
    use kobayashi::optimizer::sensitivity::{run_sensitivity, OutcomeMetric, SensitivityRequest};

    let ship = parse_named_string_arg_main(args, "--ship")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "--ship <id> required".to_string())?;
    let hostile = parse_named_string_arg_main(args, "--hostile")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "--hostile <id> required".to_string())?;
    let captain = parse_named_string_arg_main(args, "--captain");
    let bridge = parse_csv_string_arg_main(args, "--bridge");
    let below_decks = parse_csv_string_arg_main(args, "--below-decks");
    let ship_tier =
        parse_named_string_arg_main(args, "--ship-tier").and_then(|s| s.parse::<u32>().ok());
    let ship_level =
        parse_named_string_arg_main(args, "--ship-level").and_then(|s| s.parse::<u32>().ok());
    let num_sims = parse_named_string_arg_main(args, "--sims")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(2000);
    let seed = parse_named_string_arg_main(args, "--seed")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let metric = match parse_named_string_arg_main(args, "--metric").as_deref() {
        Some("win") | Some("win_rate") => OutcomeMetric::WinRate,
        Some("rounds") | Some("rounds_to_kill") => OutcomeMetric::RoundsToKill,
        Some("defender_hull") | Some("defender_hull_remaining") => {
            OutcomeMetric::DefenderHullRemaining
        }
        _ => OutcomeMetric::HullRemaining,
    };
    let profile_id = parse_profile_arg(args);

    let request = SensitivityRequest {
        ship,
        hostile,
        ship_tier,
        ship_level,
        captain,
        bridge,
        below_decks,
        support_buffs: None,
        profile_id,
        num_sims: Some(num_sims),
        seed: Some(seed),
        rounds: None,
        metric: Some(metric),
        deltas: None,
    };

    let registry = DataRegistry::load().map_err(|e| format!("DataRegistry::load: {e}"))?;
    let response = run_sensitivity(&registry, &request).map_err(|e| format!("run: {e}"))?;

    let mut rows = response.rows.clone();
    rows.sort_by(|a, b| {
        b.mean_diff_relative
            .unwrap_or(0.0)
            .abs()
            .partial_cmp(&a.mean_diff_relative.unwrap_or(0.0).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!(
        "# sensitivity metric={} baseline_mean={} num_sims={} base_seed={}",
        response.metric, response.baseline_mean, response.num_sims, response.base_seed
    );
    println!(
        "stat\tdelta_applied\tmean_diff\tmean_diff_relative\tci95_low\tci95_high\tsignificant"
    );
    for row in rows {
        let rel = match row.mean_diff_relative {
            Some(v) => format!("{v}"),
            None => String::from("NaN"),
        };
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.stat,
            row.delta_applied,
            row.mean_diff,
            rel,
            row.ci95_low,
            row.ci95_high,
            row.significant
        );
    }
    Ok(())
}

fn morris_sensitivity_command(args: &[String]) -> Result<(), String> {
    use kobayashi::data::data_registry::DataRegistry;
    use kobayashi::optimizer::sensitivity::OutcomeMetric;
    use kobayashi::optimizer::sensitivity_morris::{run_morris, MorrisRequest};

    let ship = parse_named_string_arg_main(args, "--ship")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "--ship <id> required".to_string())?;
    let hostile = parse_named_string_arg_main(args, "--hostile")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "--hostile <id> required".to_string())?;
    let captain = parse_named_string_arg_main(args, "--captain");
    let bridge = parse_csv_string_arg_main(args, "--bridge");
    let below_decks = parse_csv_string_arg_main(args, "--below-decks");
    let ship_tier =
        parse_named_string_arg_main(args, "--ship-tier").and_then(|s| s.parse::<u32>().ok());
    let ship_level =
        parse_named_string_arg_main(args, "--ship-level").and_then(|s| s.parse::<u32>().ok());
    let num_sims = parse_named_string_arg_main(args, "--sims").and_then(|s| s.parse::<u32>().ok());
    let r_traj = parse_named_string_arg_main(args, "--r").and_then(|s| s.parse::<u32>().ok());
    let seed = parse_named_string_arg_main(args, "--seed")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let metric = match parse_named_string_arg_main(args, "--metric").as_deref() {
        Some("win") | Some("win_rate") => OutcomeMetric::WinRate,
        Some("rounds") | Some("rounds_to_kill") => OutcomeMetric::RoundsToKill,
        Some("defender_hull") | Some("defender_hull_remaining") => {
            OutcomeMetric::DefenderHullRemaining
        }
        _ => OutcomeMetric::HullRemaining,
    };
    let profile_id = parse_profile_arg(args);

    let request = MorrisRequest {
        ship,
        hostile,
        ship_tier,
        ship_level,
        captain,
        bridge,
        below_decks,
        support_buffs: None,
        profile_id,
        num_sims,
        r_trajectories: r_traj,
        seed: Some(seed),
        rounds: None,
        metric: Some(metric),
        deltas: None,
    };

    let registry = DataRegistry::load().map_err(|e| format!("DataRegistry::load: {e}"))?;
    let response = run_morris(&registry, &request).map_err(|e| format!("run: {e}"))?;

    let mut rows = response.rows.clone();
    rows.sort_by(|a, b| {
        b.mu_star
            .partial_cmp(&a.mu_star)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!(
        "# morris-sensitivity metric={} r={} sims_per_point={} k={} base_seed={} total_sims={}",
        response.metric,
        response.r_trajectories,
        response.num_sims_per_point,
        response.k_stats,
        response.base_seed,
        response.total_sims
    );
    println!(
        "stat\tdelta_applied\tmu_star\tmu\tsigma\tn_samples\tmu_star_ci95_low\tmu_star_ci95_high"
    );
    for row in rows {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.stat,
            row.delta_applied,
            row.mu_star,
            row.mu,
            row.sigma,
            row.n_samples,
            row.mu_star_ci95_low,
            row.mu_star_ci95_high
        );
    }
    Ok(())
}

fn sobol_sensitivity_command(args: &[String]) -> Result<(), String> {
    use kobayashi::data::data_registry::DataRegistry;
    use kobayashi::optimizer::sensitivity::OutcomeMetric;
    use kobayashi::optimizer::sensitivity_sobol::{run_sobol, SobolRequest};

    let ship = parse_named_string_arg_main(args, "--ship")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "--ship <id> required".to_string())?;
    let hostile = parse_named_string_arg_main(args, "--hostile")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "--hostile <id> required".to_string())?;
    let captain = parse_named_string_arg_main(args, "--captain");
    let bridge = parse_csv_string_arg_main(args, "--bridge");
    let below_decks = parse_csv_string_arg_main(args, "--below-decks");
    let ship_tier =
        parse_named_string_arg_main(args, "--ship-tier").and_then(|s| s.parse::<u32>().ok());
    let ship_level =
        parse_named_string_arg_main(args, "--ship-level").and_then(|s| s.parse::<u32>().ok());
    let n_samples = parse_named_string_arg_main(args, "--n").and_then(|s| s.parse::<u32>().ok());
    let seed = parse_named_string_arg_main(args, "--seed")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let metric = match parse_named_string_arg_main(args, "--metric").as_deref() {
        Some("win") | Some("win_rate") => OutcomeMetric::WinRate,
        Some("rounds") | Some("rounds_to_kill") => OutcomeMetric::RoundsToKill,
        Some("defender_hull") | Some("defender_hull_remaining") => {
            OutcomeMetric::DefenderHullRemaining
        }
        _ => OutcomeMetric::HullRemaining,
    };
    let profile_id = parse_profile_arg(args);

    let request = SobolRequest {
        ship,
        hostile,
        ship_tier,
        ship_level,
        captain,
        bridge,
        below_decks,
        support_buffs: None,
        profile_id,
        n_samples,
        seed: Some(seed),
        rounds: None,
        metric: Some(metric),
        deltas: None,
    };

    let registry = DataRegistry::load().map_err(|e| format!("DataRegistry::load: {e}"))?;
    let response = run_sobol(&registry, &request).map_err(|e| format!("run: {e}"))?;

    let mut rows = response.rows.clone();
    rows.sort_by(|a, b| b.st.partial_cmp(&a.st).unwrap_or(std::cmp::Ordering::Equal));

    println!(
        "# sobol-sensitivity metric={} n_samples={} k={} base_seed={} total_sims={} output_variance={}",
        response.metric,
        response.n_samples,
        response.k_stats,
        response.base_seed,
        response.total_sims,
        response.output_variance
    );
    println!(
        "stat\tbase_delta\ts1\ts1_ci95_low\ts1_ci95_high\tst\tst_ci95_low\tst_ci95_high\tinteraction"
    );
    for row in rows {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            row.stat,
            row.base_delta,
            row.s1,
            row.s1_ci95_low,
            row.s1_ci95_high,
            row.st,
            row.st_ci95_low,
            row.st_ci95_high,
            row.interaction
        );
    }
    Ok(())
}

fn print_usage() {
    eprintln!(
        "usage: kobayashi <serve|simulate|optimize|import|validate|validate-log|generate-lcars|mitigation-sensitivity|sensitivity|morris-sensitivity|sobol-sensitivity|battlelogs> [args]\n\
simulate: kobayashi simulate <rounds> <seed> [--profile <id>] [--defender-faction <slug>] [--hostile <id>]\n\
  or kobayashi simulate --attacker-id <id> --attacker-attack <f64> ... [--defender-faction <slug>] [--hostile <id>] [--profile <id>]\n\
optimize: kobayashi optimize <ship> <hostile> <sims> [--profile <id>]\n\
  or kobayashi optimize --ship <id> --hostile <id> --sims <u32> [--max-candidates <u32>] [--profile <id>]\n\
import: kobayashi import <path> [--profile <id>]\n\
validate-log: kobayashi validate-log <path.json>\n\
mitigation-sensitivity: kobayashi mitigation-sensitivity <ship> <hostile> [--delta-pct <f64>]\n\
sensitivity: kobayashi sensitivity --ship <id> --hostile <id> --captain <id> --bridge <id,id,...> [--below-decks <id,...>] [--ship-tier <n>] [--ship-level <n>] [--metric hull|win|rounds|defender_hull] [--sims <n>] [--seed <n>] [--profile <id>]\n\
morris-sensitivity: kobayashi morris-sensitivity --ship <id> --hostile <id> --captain <id> --bridge <id,id,...> [--below-decks <id,...>] [--ship-tier <n>] [--ship-level <n>] [--metric hull|win|rounds|defender_hull] [--sims <n>] [--r <trajectories>] [--seed <n>] [--profile <id>]\n\
sobol-sensitivity: kobayashi sobol-sensitivity --ship <id> --hostile <id> --captain <id> --bridge <id,id,...> [--below-decks <id,...>] [--ship-tier <n>] [--ship-level <n>] [--metric hull|win|rounds|defender_hull] [--n <samples>] [--seed <n>] [--profile <id>]\n\
battlelogs: kobayashi battlelogs [--profile <id>] [--sample]"
    );
}

fn main() {
    let _ = ensure_profile_index_bootstrap();
    let _ = prune_ephemeral_scenario_test_profiles();
    let _ = sync_profile_index_with_disk();
    kobayashi::logging::init();
    kobayashi::parallel::init_from_env();

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
        Some(Command::ValidateLog) => {
            if let Err(err) = validate_log_command(&command_args) {
                eprintln!("{err}");
                exit_code = 2;
            }
        }
        Some(Command::GenerateLcars) => {
            exit_code = handle_generate_lcars(&command_args);
        }
        Some(Command::MitigationSensitivity) => {
            if let Err(err) = mitigation_sensitivity_command(&command_args) {
                eprintln!("mitigation-sensitivity error: {err}");
                print_usage();
                exit_code = 2;
            }
        }
        Some(Command::Sensitivity) => {
            if let Err(err) = sensitivity_command(&command_args) {
                eprintln!("sensitivity error: {err}");
                print_usage();
                exit_code = 2;
            }
        }
        Some(Command::MorrisSensitivity) => {
            if let Err(err) = morris_sensitivity_command(&command_args) {
                eprintln!("morris-sensitivity error: {err}");
                print_usage();
                exit_code = 2;
            }
        }
        Some(Command::SobolSensitivity) => {
            if let Err(err) = sobol_sensitivity_command(&command_args) {
                eprintln!("sobol-sensitivity error: {err}");
                print_usage();
                exit_code = 2;
            }
        }
        Some(Command::Battlelogs) => {
            if let Err(err) = battlelogs_command(&command_args) {
                eprintln!("battlelogs error: {err}");
                print_usage();
                exit_code = 2;
            }
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

    #[test]
    fn parse_simulate_args_defender_faction_and_hostile() {
        let args = vec![
            "--rounds".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "1".to_string(),
            "--defender-faction".to_string(),
            "borg".to_string(),
            "--hostile".to_string(),
            "2918121098".to_string(),
        ];
        let parsed = parse_simulate_args(&args).expect("parse should succeed");
        assert_eq!(parsed.defender_faction_slug.as_deref(), Some("borg"));
        assert_eq!(parsed.hostile_lookup.as_deref(), Some("2918121098"));
    }
}
