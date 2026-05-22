use std::env;
use std::fmt::Write as _;

use crate::combat::{
    default_percent_sensitivity_rows, format_sensitivity_tsv,
    simulate_combat_with_defender_faction, Combatant, CrewConfiguration, EnemyTypes,
    HostileMitigationBaseline, OpponentFactionTag, SimulationConfig, TraceMode, MITIGATION_CEILING,
    MITIGATION_FLOOR,
};
use crate::data::import::{import_roster_csv_to, import_spocks_export_to};
use crate::data::loader::{
    defender_faction_for_cli_simulate, defender_hull_faction_id_for_cli_simulate, resolve_hostile,
    resolve_ship,
};
use crate::data::profile::{apply_profile_to_attacker, load_profile, OfficerStatRuntimeBonus};
use crate::data::profile_index::{
    ensure_profile_index_bootstrap, profile_data_dir, profile_path,
    prune_ephemeral_scenario_test_profiles, resolve_profile_id_for_api,
    sync_profile_index_with_disk, PROFILE_JSON, ROSTER_IMPORTED,
};
use crate::data::validate::{validate_officer_dataset, ValidationSeverity};
use crate::optimizer::optimize_crew;
use crate::parallel::init_from_env;
use crate::server;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Serve,
    Simulate,
    Optimize,
    Import,
    Validate,
    Resolve,
    MitigationSensitivity,
    Sensitivity,
    MorrisSensitivity,
    SobolSensitivity,
}

pub fn parse_command(args: &[String]) -> Option<Command> {
    match args.get(1).map(String::as_str) {
        Some("serve") => Some(Command::Serve),
        Some("simulate") => Some(Command::Simulate),
        Some("optimize") => Some(Command::Optimize),
        Some("import") => Some(Command::Import),
        Some("validate") => Some(Command::Validate),
        Some("resolve") => Some(Command::Resolve),
        Some("mitigation-sensitivity") => Some(Command::MitigationSensitivity),
        Some("sensitivity") => Some(Command::Sensitivity),
        Some("morris-sensitivity") => Some(Command::MorrisSensitivity),
        Some("sobol-sensitivity") => Some(Command::SobolSensitivity),
        _ => None,
    }
}

pub fn run_with_args(args: &[String]) -> i32 {
    let _ = ensure_profile_index_bootstrap();
    let _ = prune_ephemeral_scenario_test_profiles();
    let _ = sync_profile_index_with_disk();
    crate::logging::init();
    init_from_env();

    match parse_command(args) {
        Some(Command::Serve) => handle_serve(),
        Some(Command::Simulate) => handle_simulate(args),
        Some(Command::Optimize) => handle_optimize(args),
        Some(Command::Import) => handle_import(args),
        Some(Command::Validate) => handle_validate(args),
        Some(Command::Resolve) => handle_resolve(args),
        Some(Command::MitigationSensitivity) => handle_mitigation_sensitivity(args),
        Some(Command::Sensitivity) => handle_sensitivity(args),
        Some(Command::MorrisSensitivity) => handle_morris_sensitivity(args),
        Some(Command::SobolSensitivity) => handle_sobol_sensitivity(args),
        None => {
            eprintln!(
                "usage: kobayashi <serve|simulate|optimize|import|validate|resolve|mitigation-sensitivity|sensitivity|morris-sensitivity|sobol-sensitivity>"
            );
            2
        }
    }
}

fn handle_mitigation_sensitivity(args: &[String]) -> i32 {
    let ship = match args.get(2).map(String::as_str).filter(|s| !s.is_empty()) {
        Some(s) => s,
        None => {
            eprintln!(
                "usage: kobayashi mitigation-sensitivity <ship> <hostile> [--delta-pct <f64>]"
            );
            return 2;
        }
    };
    let hostile = match args.get(3).map(String::as_str).filter(|s| !s.is_empty()) {
        Some(s) => s,
        None => {
            eprintln!(
                "usage: kobayashi mitigation-sensitivity <ship> <hostile> [--delta-pct <f64>]"
            );
            return 2;
        }
    };
    let mut delta_pct = 0.1_f64;
    let mut i = 4;
    while i < args.len() {
        if args[i] == "--delta-pct" {
            let Some(v) = args.get(i + 1) else {
                eprintln!("--delta-pct requires a value");
                return 2;
            };
            let Ok(p) = v.parse::<f64>() else {
                eprintln!("delta-pct must be a number");
                return 2;
            };
            delta_pct = p;
            i += 2;
        } else {
            i += 1;
        }
    }
    let Some(ship_rec) = resolve_ship(ship) else {
        eprintln!("unknown ship '{ship}'");
        return 1;
    };
    let Some(hostile_rec) = resolve_hostile(hostile) else {
        eprintln!("unknown hostile '{hostile}'");
        return 1;
    };
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
    0
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

/// Optional `--defender-faction` / `--hostile` for `simulate` (same semantics as `main` binary).
fn parse_simulate_defender_faction_flags(args: &[String]) -> (Option<String>, Option<String>) {
    let mut faction: Option<String> = None;
    let mut hostile: Option<String> = None;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--defender-faction" => {
                if let Some(v) = args.get(i + 1) {
                    faction = Some(v.clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--hostile" => {
                if let Some(v) = args.get(i + 1) {
                    hostile = Some(v.clone());
                    i += 2;
                } else {
                    i += 1;
                }
            }
            "--profile" => {
                i = i.saturating_add(2);
            }
            _ => i += 1,
        }
    }
    (faction, hostile)
}

fn handle_simulate(args: &[String]) -> i32 {
    let rounds = parse_u32_arg(args.get(2), "rounds", 3);
    let seed = parse_u64_arg(args.get(3), "seed", 7);
    let as_table = args.iter().any(|arg| arg == "--table");
    let (faction_slug, hostile_lookup) = parse_simulate_defender_faction_flags(args);
    let defender_faction =
        match defender_faction_for_cli_simulate(faction_slug.as_deref(), hostile_lookup.as_deref())
        {
            Ok(t) => t,
            Err(e) => {
                eprintln!("simulate error: {e}");
                return 2;
            }
        };
    let defender_hull_faction_id =
        defender_hull_faction_id_for_cli_simulate(hostile_lookup.as_deref());
    let defender_hostile_tag_mask = hostile_lookup
        .as_deref()
        .and_then(resolve_hostile)
        .map(|h| h.hostile_tag_mask())
        .unwrap_or(0);

    let profile_id = resolve_profile_id_for_api(parse_profile_arg(args).as_deref());
    let profile_path_str = profile_path(&profile_id, PROFILE_JSON)
        .to_string_lossy()
        .to_string();
    let player_profile = load_profile(&profile_path_str);

    let attacker = apply_profile_to_attacker(
        Combatant {
            id: "player".to_string(),
            attack: 120.0,
            mitigation: 0.1,
            armor: 0.0,
            shield_deflection: 0.0,
            dodge: 0.0,
            damage_reduction: 0.0,
            pierce: 0.15,
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
            isolytic_damage: 0.0,
            isolytic_defense: 0.0,
            weapons: vec![],
            hostile_mitigation_params: None,
        },
        &player_profile,
        None,
        OfficerStatRuntimeBonus::default(),
    );
    let defender = Combatant {
        id: "hostile".to_string(),
        attack: 10.0,
        mitigation: 0.35,
        armor: 0.0,
        shield_deflection: 0.0,
        dodge: 0.0,
        damage_reduction: 0.0,
        pierce: 0.0,
        crit_chance: 0.0,
        crit_multiplier: 1.0,
        proc_chance: 0.0,
        proc_multiplier: 1.0,
        end_of_round_damage: 0.0,
        hull_health: 1000.0,
        shield_health: 500.0,
        shield_mitigation: 0.8,
        apex_barrier: 0.0,
        apex_shred: 0.0,
        isolytic_damage: 0.0,
        isolytic_defense: 0.0,
        weapons: vec![],
        hostile_mitigation_params: None,
    };

    let result = simulate_combat_with_defender_faction(
        &attacker,
        &defender,
        &SimulationConfig {
            rounds,
            seed,
            trace_mode: TraceMode::Events,
            initial_attacker_hull_damage: 0.0,
            weapon_damage_profile_additive_pool: None,
            profile_weapon_damage_fraction: 0.0,
            defender_hull_faction_id,
            defender_hostile_tag_mask,
            attacker_owner_faction: OpponentFactionTag::Unknown,
            engagement_enemy_types: EnemyTypes::default(),
            defender_level: None,
            attacker_roster_officer_ids: Vec::new(),
            incoming_shield_mitigation_bonus: 0.0,
            incoming_shield_mitigation_bonus_rounds: 0,
            emit_state_snapshots: false,
            crit_damage_reduction_perturb: 0.0,
        },
        &CrewConfiguration::default(),
        defender_faction,
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
    let profile_id = resolve_profile_id_for_api(parse_profile_arg(args).as_deref());

    let ranked = optimize_crew(ship, hostile, sims, Some(profile_id.as_str()));
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

fn parse_named_string_arg(args: &[String], name: &str) -> Option<String> {
    let mut idx = 0;
    while idx < args.len() {
        if args[idx] == name {
            return args.get(idx + 1).cloned();
        }
        idx += 1;
    }
    None
}

fn parse_csv_string_arg(args: &[String], name: &str) -> Vec<String> {
    parse_named_string_arg(args, name)
        .map(|s| {
            s.split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn handle_sensitivity(args: &[String]) -> i32 {
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::sensitivity::{run_sensitivity, OutcomeMetric, SensitivityRequest};

    let ship = match parse_named_string_arg(args, "--ship") {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!(
                "usage: kobayashi sensitivity --ship <id> --hostile <id> --captain <id> --bridge <id,id,...> \
                 [--below-decks <id,...>] [--ship-tier <n>] [--ship-level <n>] \
                 [--metric hull|win|rounds|defender_hull] [--sims <n>] [--seed <n>] [--profile <id>]"
            );
            return 2;
        }
    };
    let hostile = match parse_named_string_arg(args, "--hostile") {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!("--hostile <id> required");
            return 2;
        }
    };
    let captain = parse_named_string_arg(args, "--captain");
    let bridge = parse_csv_string_arg(args, "--bridge");
    let below_decks = parse_csv_string_arg(args, "--below-decks");
    let ship_tier = parse_named_string_arg(args, "--ship-tier").and_then(|s| s.parse::<u32>().ok());
    let ship_level =
        parse_named_string_arg(args, "--ship-level").and_then(|s| s.parse::<u32>().ok());
    let num_sims = parse_named_string_arg(args, "--sims")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(2000);
    let seed = parse_named_string_arg(args, "--seed")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let metric = match parse_named_string_arg(args, "--metric").as_deref() {
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

    let registry = match DataRegistry::load() {
        Ok(r) => r,
        Err(err) => {
            eprintln!("DataRegistry::load failed: {err}");
            return 1;
        }
    };

    let response = match run_sensitivity(&registry, &request) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("run_sensitivity failed: {err}");
            return 1;
        }
    };

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
    0
}

fn handle_morris_sensitivity(args: &[String]) -> i32 {
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::sensitivity::OutcomeMetric;
    use crate::optimizer::sensitivity_morris::{run_morris, MorrisRequest};

    let ship = match parse_named_string_arg(args, "--ship") {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!(
                "usage: kobayashi morris-sensitivity --ship <id> --hostile <id> --captain <id> --bridge <id,id,...> \
                 [--below-decks <id,...>] [--ship-tier <n>] [--ship-level <n>] \
                 [--metric hull|win|rounds|defender_hull] [--sims <n>] [--r <trajectories>] [--seed <n>] [--profile <id>]"
            );
            return 2;
        }
    };
    let hostile = match parse_named_string_arg(args, "--hostile") {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!("--hostile <id> required");
            return 2;
        }
    };
    let captain = parse_named_string_arg(args, "--captain");
    let bridge = parse_csv_string_arg(args, "--bridge");
    let below_decks = parse_csv_string_arg(args, "--below-decks");
    let ship_tier = parse_named_string_arg(args, "--ship-tier").and_then(|s| s.parse::<u32>().ok());
    let ship_level =
        parse_named_string_arg(args, "--ship-level").and_then(|s| s.parse::<u32>().ok());
    let num_sims = parse_named_string_arg(args, "--sims").and_then(|s| s.parse::<u32>().ok());
    let r_traj = parse_named_string_arg(args, "--r").and_then(|s| s.parse::<u32>().ok());
    let seed = parse_named_string_arg(args, "--seed")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let metric = match parse_named_string_arg(args, "--metric").as_deref() {
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

    let registry = match DataRegistry::load() {
        Ok(r) => r,
        Err(err) => {
            eprintln!("DataRegistry::load failed: {err}");
            return 1;
        }
    };

    let response = match run_morris(&registry, &request) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("run_morris failed: {err}");
            return 1;
        }
    };

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
    0
}

fn handle_sobol_sensitivity(args: &[String]) -> i32 {
    use crate::data::data_registry::DataRegistry;
    use crate::optimizer::sensitivity::OutcomeMetric;
    use crate::optimizer::sensitivity_sobol::{run_sobol, SobolRequest};

    let ship = match parse_named_string_arg(args, "--ship") {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!(
                "usage: kobayashi sobol-sensitivity --ship <id> --hostile <id> --captain <id> --bridge <id,id,...> \
                 [--below-decks <id,...>] [--ship-tier <n>] [--ship-level <n>] \
                 [--metric hull|win|rounds|defender_hull] [--n <samples>] [--seed <n>] [--profile <id>]"
            );
            return 2;
        }
    };
    let hostile = match parse_named_string_arg(args, "--hostile") {
        Some(s) if !s.is_empty() => s,
        _ => {
            eprintln!("--hostile <id> required");
            return 2;
        }
    };
    let captain = parse_named_string_arg(args, "--captain");
    let bridge = parse_csv_string_arg(args, "--bridge");
    let below_decks = parse_csv_string_arg(args, "--below-decks");
    let ship_tier = parse_named_string_arg(args, "--ship-tier").and_then(|s| s.parse::<u32>().ok());
    let ship_level =
        parse_named_string_arg(args, "--ship-level").and_then(|s| s.parse::<u32>().ok());
    let n_samples = parse_named_string_arg(args, "--n").and_then(|s| s.parse::<u32>().ok());
    let seed = parse_named_string_arg(args, "--seed")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    let metric = match parse_named_string_arg(args, "--metric").as_deref() {
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

    let registry = match DataRegistry::load() {
        Ok(r) => r,
        Err(err) => {
            eprintln!("DataRegistry::load failed: {err}");
            return 1;
        }
    };

    let response = match run_sobol(&registry, &request) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("run_sobol failed: {err}");
            return 1;
        }
    };

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
    0
}

fn handle_import(args: &[String]) -> i32 {
    let raw = match args.get(2).filter(|s| !s.starts_with("--")) {
        Some(p) => p.clone(),
        None => {
            eprintln!("usage: kobayashi import <path> [--profile <id>]");
            eprintln!("  use a .txt file for your roster (comma-separated: name,tier,level), or a .json file for Spocks export");
            eprintln!("  bare filename resolves under profiles/<profile>/ (default profile if --profile omitted)");
            return 2;
        }
    };
    let profile_id = resolve_profile_id_for_api(parse_profile_arg(args).as_deref());
    let path = if raw.contains('/') || raw.contains('\\') {
        raw
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

fn handle_resolve(args: &[String]) -> i32 {
    let officer_id = args.get(2).map(|s| s.as_str()).unwrap_or("");
    if officer_id.is_empty() {
        eprintln!("Usage: kobayashi resolve <officer_id_or_name>");
        return 2;
    }

    let lcars_officers = match crate::lcars::load_lcars_dir("data/officers") {
        Ok(officers) => officers,
        Err(err) => {
            eprintln!("failed to load LCARS officers: {err}");
            return 1;
        }
    };

    let by_id = crate::lcars::index_lcars_officers_by_id(lcars_officers.clone());

    // Try by id first, then by name (case-insensitive)
    let officer = by_id.get(officer_id).cloned().or_else(|| {
        let lower = officer_id.to_lowercase();
        lcars_officers
            .iter()
            .find(|o| o.name.to_lowercase() == lower)
            .cloned()
    });

    match officer {
        Some(o) => {
            let opts = crate::lcars::ResolveOptions::default();
            let buff_set = crate::lcars::resolve_crew_to_buff_set(
                &o.id,
                std::slice::from_ref(&o.id),
                std::slice::from_ref(&o.id),
                &by_id,
                &opts,
            );
            println!("Officer: {} ({})", o.name, o.id);
            println!("Resolved BuffSet:");
            println!("{:#?}", buff_set);
            0
        }
        None => {
            eprintln!("Officer '{}' not found in LCARS definitions", officer_id);
            // List available officers
            let mut names: Vec<&str> = lcars_officers.iter().map(|o| o.name.as_str()).collect();
            names.sort();
            eprintln!("Available officers ({}):", names.len());
            for name in names.iter().take(20) {
                eprintln!("  {}", name);
            }
            if names.len() > 20 {
                eprintln!("  ... and {} more", names.len() - 20);
            }
            1
        }
    }
}
