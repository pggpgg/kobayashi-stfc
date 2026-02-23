//! Run Monte Carlo once in sequential and once in parallel, then print timings and speedup.
//!
//! Usage: cargo run --release --bin benchmark_parallel_speedup
//!
//! Run from the project root so data/officers and data/ships are available if present.

use std::time::Instant;

use kobayashi::optimizer::crew_generator::{CrewCandidate, CrewGenerator};
use kobayashi::optimizer::monte_carlo::{run_monte_carlo, run_monte_carlo_parallel};

fn main() {
    let ship = "saladin";
    let hostile = "explorer_30";
    let seed = 12345u64;
    let iterations = 1000;

    let candidates: Vec<CrewCandidate> = {
        let gen = CrewGenerator::new();
        let mut from_gen = gen.generate_candidates(ship, hostile, seed);
        if from_gen.len() >= 16 {
            from_gen.truncate(64);
            from_gen
        } else {
            (0..32)
                .map(|i| CrewCandidate {
                    captain: format!("Captain_{i}"),
                    bridge: vec!["Bridge_A".to_string(), "Bridge_B".to_string()],
                    below_decks: vec![
                        "Below_1".to_string(),
                        "Below_2".to_string(),
                        "Below_3".to_string(),
                    ],
                })
                .collect()
        }
    };

    let n = candidates.len();
    println!(
        "Monte Carlo: {} candidates × {} iterations (ship={}, hostile={})",
        n, iterations, ship, hostile
    );
    println!();

    // Sequential
    let t0 = Instant::now();
    let results_seq = run_monte_carlo(ship, hostile, &candidates, iterations, seed);
    let elapsed_seq = t0.elapsed();
    let seq_ms = elapsed_seq.as_secs_f64() * 1000.0;
    println!("Sequential:  {:.2} ms  ({:.1} sims/s)", seq_ms, (n * iterations) as f64 / elapsed_seq.as_secs_f64());

    // Parallel
    let t0 = Instant::now();
    let results_par = run_monte_carlo_parallel(ship, hostile, &candidates, iterations, seed);
    let elapsed_par = t0.elapsed();
    let par_ms = elapsed_par.as_secs_f64() * 1000.0;
    println!("Parallel:    {:.2} ms  ({:.1} sims/s)", par_ms, (n * iterations) as f64 / elapsed_par.as_secs_f64());

    let speedup = seq_ms / par_ms;
    println!();
    println!("Speedup:     {:.2}x faster (parallel vs sequential)", speedup);

    assert_eq!(results_seq.len(), results_par.len());
    // Sanity: same number of results
    for (i, (a, b)) in results_seq.iter().zip(results_par.iter()).enumerate() {
        assert!((a.win_rate - b.win_rate).abs() < 1e-9, "result {} win_rate mismatch", i);
        assert!((a.avg_hull_remaining - b.avg_hull_remaining).abs() < 1e-9, "result {} avg_hull_remaining mismatch", i);
    }
    println!("(Results match sequential vs parallel)");
}
