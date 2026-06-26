//! Measures the wall-clock effect of top-K progressive abandonment on the exhaustive and genetic
//! optimizer paths, using a borderline matchup (a weak survey hull vs a mid hostile) where crew
//! win rates spread — the regime where win-rate abandonment actually prunes work.
//!
//! Run:
//!   cargo run --release --bin early_stop_bench
//!
//! Toggles are driven through the same env gates the production code reads
//! (`KOBAYASHI_FULLMC_EARLY_STOP`, `KOBAYASHI_GENETIC_EARLY_STOP`).

use std::time::Instant;

use kobayashi::data::data_registry::DataRegistry;
use kobayashi::optimizer::genetic::{run_genetic_optimizer_ranked_with_stats, GeneticConfig};
use kobayashi::optimizer::{
    optimize_scenario_with_registry, OptimizationScenario, OptimizerStrategy,
};
use kobayashi::parallel::init_from_env;

const SHIP: &str = "botany_bay";
const HOSTILE: &str = "38048587";
const SEED: u64 = 7;

fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[xs.len() / 2]
}

fn bench_exhaustive(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    tier: Option<u32>,
    label: &str,
) {
    bench_exhaustive_full(registry, ship, hostile, tier, tier.map(|_| 1), None, label);
}

#[allow(clippy::too_many_arguments)]
fn bench_exhaustive_full(
    registry: &DataRegistry,
    ship: &str,
    hostile: &str,
    tier: Option<u32>,
    level: Option<u32>,
    profile: Option<&str>,
    label: &str,
) {
    let sims = 2000usize;
    let max_candidates = Some(600usize);
    let reps = 3;

    let run = |early: bool| -> (f64, String, usize, f64) {
        std::env::set_var("KOBAYASHI_FULLMC_EARLY_STOP", if early { "1" } else { "0" });
        let scenario = OptimizationScenario {
            ship,
            hostile,
            ship_tier: tier,
            ship_level: level,
            profile_id: profile,
            simulation_count: sims,
            seed: SEED,
            max_candidates,
            strategy: OptimizerStrategy::Exhaustive,
            ..Default::default()
        };
        // warmup
        let _ = optimize_scenario_with_registry(registry, &scenario);
        let mut times = Vec::new();
        let mut winner = String::new();
        let mut n = 0usize;
        // Average trials run per crew on the last rep (shows how much abandonment actually pruned).
        let mut avg_trials = 0.0;
        for _ in 0..reps {
            let t0 = Instant::now();
            let res = optimize_scenario_with_registry(registry, &scenario);
            times.push(t0.elapsed().as_secs_f64());
            n = res.len();
            avg_trials = if res.is_empty() {
                0.0
            } else {
                res.iter().map(|r| r.trials_run as f64).sum::<f64>() / res.len() as f64
            };
            winner = res
                .first()
                .map(|r| format!("{}|{:?}|{:?}", r.captain, r.bridge, r.below_decks))
                .unwrap_or_default();
        }
        (median(times), winner, n, avg_trials)
    };

    println!("\n=== EXHAUSTIVE [{label}]  ({ship} t{tier:?}l{level:?} vs {hostile}, sims={sims}, max_candidates={max_candidates:?}) ===");
    let (t_off, w_off, n_off, tr_off) = run(false);
    let (t_on, w_on, n_on, tr_on) = run(true);
    println!("  candidates ranked: off={n_off}  on={n_on}");
    println!(
        "  avg trials/crew:   off={tr_off:.0}  on={tr_on:.0}  (pruned {:.0}%)",
        100.0 * (1.0 - tr_on / tr_off.max(1.0))
    );
    println!(
        "  median wall-clock: off={t_off:.3}s  on={t_on:.3}s  speedup={:.2}x",
        t_off / t_on
    );
    println!(
        "  winner identical:  {}",
        if w_off == w_on { "YES" } else { "NO (!)" }
    );
    if w_off != w_on {
        println!("    off: {w_off}\n    on:  {w_on}");
    }
}

fn bench_genetic(registry: &DataRegistry) {
    let sims = 1500usize;
    let reps = 3;

    let cfg = || GeneticConfig {
        population_size: 128,
        generations: 15,
        sims_per_eval: sims,
        // Legacy full-population eval each generation → exercises the abandonment site directly.
        incremental_fitness: false,
        stagnation_limit: None,
        ship_tier: Some(1),
        ship_level: Some(1),
        ..GeneticConfig::default()
    };

    let run = |early: bool| -> (f64, String) {
        std::env::set_var(
            "KOBAYASHI_GENETIC_EARLY_STOP",
            if early { "1" } else { "0" },
        );
        let config = cfg();
        // warmup
        let _ = run_genetic_optimizer_ranked_with_stats(
            SHIP,
            HOSTILE,
            &config,
            SEED,
            sims,
            Some(registry),
            |_, _, _| true,
            || true,
        );
        let mut times = Vec::new();
        let mut winner = String::new();
        for _ in 0..reps {
            let t0 = Instant::now();
            let (res, _stats) = run_genetic_optimizer_ranked_with_stats(
                SHIP,
                HOSTILE,
                &config,
                SEED,
                sims,
                Some(registry),
                |_, _, _| true,
                || true,
            );
            times.push(t0.elapsed().as_secs_f64());
            winner = res
                .first()
                .map(|r| format!("{}|{:?}", r.captain, r.bridge))
                .unwrap_or_default();
        }
        (median(times), winner)
    };

    println!(
        "\n=== GENETIC  ({SHIP} t1l1 vs {HOSTILE}, pop=128, gens=15, sims_per_eval={sims}) ==="
    );
    let (t_off, w_off) = run(false);
    let (t_on, w_on) = run(true);
    println!(
        "  median wall-clock: off={t_off:.3}s  on={t_on:.3}s  speedup={:.2}x",
        t_off / t_on
    );
    println!("  best-crew (off): {w_off}");
    println!("  best-crew (on):  {w_on}");
}

/// Scan ship × hostiles and report win-rate + hull spread among candidates, to find a matchup where
/// crews mostly win but take varying hull damage (the regime hull-aware abandonment targets).
#[allow(clippy::too_many_arguments)]
fn probe(
    registry: &DataRegistry,
    ship: &str,
    tier: Option<u32>,
    level: Option<u32>,
    profile: Option<&str>,
    hostiles: &[&str],
) {
    println!("\n=== PROBE  ship={ship} tier={tier:?} level={level:?} profile={profile:?} ===");
    for &hostile in hostiles {
        std::env::set_var("KOBAYASHI_FULLMC_EARLY_STOP", "0");
        let scenario = OptimizationScenario {
            ship,
            hostile,
            ship_tier: tier,
            ship_level: level,
            profile_id: profile,
            simulation_count: 400,
            seed: SEED,
            max_candidates: Some(200),
            strategy: OptimizerStrategy::Exhaustive,
            ..Default::default()
        };
        let res = optimize_scenario_with_registry(registry, &scenario);
        if res.len() < 10 {
            println!("  {hostile}: only {} candidates", res.len());
            continue;
        }
        let mut wins: Vec<f64> = res.iter().map(|r| r.win_rate).collect();
        let mut hulls: Vec<f64> = res.iter().map(|r| r.avg_hull_remaining).collect();
        wins.sort_by(|a, b| a.partial_cmp(b).unwrap());
        hulls.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q = |v: &[f64], f: f64| v[((v.len() as f64 * f) as usize).min(v.len() - 1)];
        println!(
            "  {hostile}: n={} win[min={:.2} med={:.2} max={:.2}] hull[min={:.2} p25={:.2} med={:.2} p75={:.2} max={:.2}]",
            res.len(),
            wins[0], q(&wins, 0.5), wins[wins.len() - 1],
            hulls[0], q(&hulls, 0.25), q(&hulls, 0.5), q(&hulls, 0.75), hulls[hulls.len() - 1],
        );
    }
}

fn main() {
    init_from_env();
    let registry = DataRegistry::load().expect("data registry");
    let mode = std::env::args().nth(1).unwrap_or_default();
    if mode == "probe" {
        // Realistic endgame PvE: maxed Enterprise-D (demo roster) vs hard level-60 hostiles.
        probe(
            &registry,
            "uss_enterprise_d",
            Some(12),
            Some(60),
            Some("demo"),
            &[
                "3931453197",
                "563945657",
                "8355930",
                "8874374",
                "6413103",
                "10264305",
                "27926657",
                "21007889",
            ],
        );
        return;
    }
    let edm = |h: &str, label: &str| {
        bench_exhaustive_full(
            &registry,
            "uss_enterprise_d",
            h,
            Some(12),
            Some(60),
            Some("demo"),
            label,
        )
    };
    if mode == "margin" {
        // Sweep the abandonment margin on the wide-hull-spread scenario (10264305: win=1, hull
        // 0.85–1.00) to see whether a smaller margin + lower min-trials floor makes hull-aware
        // pruning actually fire. Reports avg trials/crew (pruning %) and whether the winner holds.
        std::env::set_var("KOBAYASHI_EARLYSTOP_MIN_TRIALS_DIV", "40");
        for m in ["0.05", "0.02", "0.01", "0.005", "0.002"] {
            std::env::set_var("KOBAYASHI_EARLYSTOP_MARGIN", m);
            println!("\n##### MARGIN={m}  (min_trials_div=40) #####");
            edm("10264305", "wide-hull-spread");
        }
        // One expensive win-spread scenario at the most aggressive margin: does pruning pay off?
        std::env::set_var("KOBAYASHI_EARLYSTOP_MARGIN", "0.01");
        println!("\n##### MARGIN=0.01 on expensive borderline (21007889) #####");
        edm("21007889", "borderline:mixed-win");
        return;
    }
    // Maxed Enterprise-D (demo roster) vs hard level-60 hostiles: win ~1 with real hull spread.
    edm("3931453197", "hard:user-example"); // hull ~0.92–0.93 (tight)
    edm("10264305", "hard:wide-hull-spread"); // hull ~0.85–1.00
    edm("21007889", "borderline:mixed-win"); // win 0–1
                                             // Saturated-win matchup (regression guard for the common one-shot case).
    bench_exhaustive(&registry, "saladin", "2918121098", None, "saturated-win");
    bench_genetic(&registry);
}
