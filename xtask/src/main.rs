//! Discoverable maintenance tasks for the Kobayashi repo.
//! Run from repo root: `cargo xtask --help`

mod bench_check;
mod optimizer_bench_check;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "Kobayashi maintenance runner (wraps scripts/ + cargo bins). Run from repo root.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Fetch ship JSON from data.stfc.space, build registry, normalize ships_extended
    RefreshShips {
        /// Extra args for `scripts/fetch_stfcspace_ships.mjs` (--full, --limit N, --ids …)
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        fetch_args: Vec<String>,
    },
    /// Fetch hostile JSON, then `normalize_hostiles_stfc_space`
    RefreshHostiles {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        fetch_args: Vec<String>,
    },
    /// `cargo run --bin validate_data` (pass extra args after `--`)
    Validate {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// `cargo run --bin generate_lcars` (defaults: data/officers; see bin help)
    RegenLcars {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        extra: Vec<String>,
    },
    /// Orchestrated importers (`npm run data:refresh`, same flags as scripts/data-refresh.mjs)
    DataRefresh {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// `npm run fetch:stfcspace:details` (requires `--entities …`)
    FetchStfcspaceDetails {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Fetch research JSON, then import into `data/research_catalog.json`
    RefreshResearch {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        fetch_args: Vec<String>,
    },
    /// Fetch officer reference JSON; use `--regen` to chain normalize ids + generate_lcars
    RefreshOfficers {
        #[arg(long, help = "Run normalize_officer_id_strings.py then generate_lcars")]
        regen: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        fetch_args: Vec<String>,
    },
    /// `cargo run --bin report_unknown_mappings`
    ReportUnknownMappings,
    /// `cargo run --bin normalize_stfc_data` (STFCcommunity baseline)
    NormalizeStfcData,
    /// Full local verification (`npm run verify`)
    Verify,
    /// Regenerate `data/ships_extended/` and fail if it drifts from the committed state.
    /// CI guard: catches an upstream/registry edit that was not followed by regeneration.
    CheckGenerated,
    /// Compare Criterion medians under `target/criterion` to `benchmark_results.log` (10% regression)
    BenchCheck {
        /// Baseline log path (relative to repo root unless absolute)
        #[arg(long, default_value = "benchmark_results.log")]
        baseline: PathBuf,
        /// Criterion output directory (default: `<repo>/target/criterion`)
        #[arg(long)]
        criterion_dir: Option<PathBuf>,
        /// Write baseline file from current Criterion output (no compare)
        #[arg(long)]
        write_baseline: bool,
        /// Write Markdown summary (for CI PR comments)
        #[arg(long)]
        markdown_out: Option<PathBuf>,
    },
    /// Run the cross-method optimizer benchmark and gate recall/regret against a committed baseline
    OptimizerBenchCheck {
        /// Baseline JSON (relative to repo root unless absolute)
        #[arg(long, default_value = "optimizer_method_bench_baseline.json")]
        baseline: PathBuf,
        /// Score an existing JSONL bench run instead of running the bench
        #[arg(long)]
        input: Option<PathBuf>,
        /// Also save the bench output here (CI artifact)
        #[arg(long)]
        jsonl_out: Option<PathBuf>,
        /// Overwrite the baseline's per-method expectations from this run (no compare)
        #[arg(long)]
        write_baseline: bool,
        /// Write Markdown summary
        #[arg(long)]
        markdown_out: Option<PathBuf>,
    },
    /// Fetch live data.stfc.space summaries and compare to committed upstream cache
    CheckUpstreamDrift {
        /// Write Markdown report (optional)
        #[arg(long)]
        markdown_out: Option<PathBuf>,
        /// Compare on-disk before/after trees instead of live fetch (`--compare-dir PATH`)
        #[arg(long)]
        compare_dir: Option<PathBuf>,
    },
    /// Run drift calibration scoreboard; optionally write docs/CALIBRATION_SCOREBOARD.md
    CalibrationScoreboard {
        /// Write Markdown scoreboard (default path: docs/CALIBRATION_SCOREBOARD.md when flag is present without value)
        #[arg(long, num_args = 0..=1, default_missing_value = "docs/CALIBRATION_SCOREBOARD.md")]
        write: Option<PathBuf>,
        #[arg(long)]
        quiet: bool,
    },
}

fn main() -> Result<()> {
    let repo = repo_root()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::RefreshShips { fetch_args } => {
            node(
                &repo,
                "scripts/fetch_stfcspace_ships.mjs",
                fetch_args.as_slice(),
            )?;
            run(
                &repo,
                python_exe().as_str(),
                &["scripts/build_ship_registry.py".to_string()],
            )?;
            cargo_run_bin(&repo, "normalize_data_stfc_space", &[])?;
        }
        Commands::RefreshHostiles { fetch_args } => {
            node(
                &repo,
                "scripts/fetch_stfcspace_hostiles.mjs",
                fetch_args.as_slice(),
            )?;
            cargo_run_bin(&repo, "normalize_hostiles_stfc_space", &[])?;
        }
        Commands::Validate { extra } => {
            cargo_run_bin(&repo, "validate_data", extra.as_slice())?;
        }
        Commands::RegenLcars { extra } => {
            cargo_run_bin(&repo, "generate_lcars", extra.as_slice())?;
        }
        Commands::DataRefresh { args } => {
            npm(&repo, "data:refresh", args.as_slice())?;
        }
        Commands::FetchStfcspaceDetails { args } => {
            npm(&repo, "fetch:stfcspace:details", args.as_slice())?;
        }
        Commands::RefreshResearch { fetch_args } => {
            node(
                &repo,
                "scripts/fetch_stfcspace_research.mjs",
                fetch_args.as_slice(),
            )?;
            node(
                &repo,
                "scripts/import_stfcspace_research.mjs",
                &["--from-upstream".into(), "--limit".into(), "0".into()],
            )?;
        }
        Commands::RefreshOfficers { regen, fetch_args } => {
            node(
                &repo,
                "scripts/fetch_stfcspace_officers.mjs",
                fetch_args.as_slice(),
            )?;
            if regen {
                run(
                    &repo,
                    python_exe().as_str(),
                    &["scripts/normalize_officer_id_strings.py".into()],
                )?;
                cargo_run_bin(&repo, "generate_lcars", &[])?;
            }
        }
        Commands::ReportUnknownMappings => {
            cargo_run_bin(&repo, "report_unknown_mappings", &[])?;
        }
        Commands::NormalizeStfcData => {
            cargo_run_bin(&repo, "normalize_stfc_data", &[])?;
        }
        Commands::Verify => {
            npm(&repo, "verify", &[])?;
        }
        Commands::CheckGenerated => {
            check_generated(&repo)?;
        }
        Commands::BenchCheck {
            baseline,
            criterion_dir,
            write_baseline,
            markdown_out,
        } => {
            let baseline_path = if baseline.is_absolute() {
                baseline
            } else {
                repo.join(baseline)
            };
            let md = markdown_out.map(|p| if p.is_absolute() { p } else { repo.join(p) });
            bench_check::run(
                &repo,
                baseline_path,
                criterion_dir.map(|p| if p.is_absolute() { p } else { repo.join(p) }),
                write_baseline,
                md,
            )?;
        }
        Commands::OptimizerBenchCheck {
            baseline,
            input,
            jsonl_out,
            write_baseline,
            markdown_out,
        } => {
            let resolve = |p: PathBuf| if p.is_absolute() { p } else { repo.join(p) };
            optimizer_bench_check::run(optimizer_bench_check::RunArgs {
                baseline: resolve(baseline),
                input: input.map(resolve),
                jsonl_out: jsonl_out.map(resolve),
                write_baseline,
                markdown_out: markdown_out.map(resolve),
                repo,
            })?;
        }
        Commands::CheckUpstreamDrift {
            markdown_out,
            compare_dir,
        } => {
            let mut args = Vec::new();
            if let Some(dir) = compare_dir {
                args.push("--compare-dir".into());
                args.push(if dir.is_absolute() {
                    dir.to_string_lossy().into_owned()
                } else {
                    repo.join(dir).to_string_lossy().into_owned()
                });
            } else {
                args.push("--check".into());
            }
            if let Some(md) = markdown_out {
                args.push("--markdown-out".into());
                args.push(if md.is_absolute() {
                    md.to_string_lossy().into_owned()
                } else {
                    repo.join(md).to_string_lossy().into_owned()
                });
            }
            node(&repo, "scripts/check_stfcspace_summary_drift.mjs", &args)?;
        }
        Commands::CalibrationScoreboard { write, quiet } => {
            let output = kobayashi::calibration::run_calibration_scoreboard(&repo)
                .map_err(|e| anyhow::anyhow!(e))?;
            if !quiet {
                print!("{}", output.text_report());
            }
            if let Some(path) = write {
                let md_path = if path.as_os_str().is_empty() {
                    repo.join("docs/CALIBRATION_SCOREBOARD.md")
                } else if path.is_absolute() {
                    path
                } else {
                    repo.join(path)
                };
                if let Some(parent) = md_path.parent() {
                    std::fs::create_dir_all(parent).with_context(|| {
                        format!("create scoreboard parent {}", parent.display())
                    })?;
                }
                std::fs::write(&md_path, output.markdown_report())
                    .with_context(|| format!("write scoreboard {}", md_path.display()))?;
                eprintln!("Wrote {}", md_path.display());
            }
            if !output.composite.drift_all_ok() {
                bail!(
                    "drift calibration failed: {} fixture(s) out of band",
                    output.composite.drift_fixtures_failed
                );
            }
        }
    }
    Ok(())
}

fn repo_root() -> Result<PathBuf> {
    let start = std::env::current_dir().context("read current_dir")?;
    for dir in start.ancestors() {
        let cargo = dir.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&cargo) {
            if text.lines().any(|l| l.trim() == "name = \"kobayashi\"") {
                return Ok(dir.to_path_buf());
            }
        }
    }
    bail!(
        "not inside the Kobayashi repository (no ancestor Cargo.toml with name = \"kobayashi\").\n\
         cwd: {}",
        start.display()
    );
}

fn python_exe() -> String {
    std::env::var("PYTHON").unwrap_or_else(|_| {
        if cfg!(windows) {
            "python".into()
        } else {
            "python3".into()
        }
    })
}

fn print_cmd(program: &str, args: &[String]) {
    eprintln!("+ {}{}", program, quote_args_for_echo(args));
}

fn quote_args_for_echo(args: &[String]) -> String {
    let mut s = String::new();
    for a in args {
        if needs_quote(a) {
            s.push(' ');
            s.push('"');
            for c in a.chars() {
                if c == '"' || c == '\\' {
                    s.push('\\');
                }
                s.push(c);
            }
            s.push('"');
        } else {
            s.push(' ');
            s.push_str(a);
        }
    }
    s
}

fn needs_quote(a: &str) -> bool {
    a.is_empty() || a.chars().any(|c| c.is_whitespace())
}

fn run(repo: &std::path::Path, program: &str, args: &[String]) -> Result<()> {
    print_cmd(program, args);
    let status = Command::new(program)
        .args(args)
        .current_dir(repo)
        .status()
        .with_context(|| format!("failed to spawn `{program}`"))?;
    if !status.success() {
        bail!("`{}` exited with status {:?}", program, status.code());
    }
    Ok(())
}

fn cargo_run_bin(repo: &std::path::Path, bin: &str, extra: &[String]) -> Result<()> {
    let mut args = vec!["run".into(), "--bin".into(), bin.into(), "--".into()];
    args.extend_from_slice(extra);
    run(repo, "cargo", &args)
}

fn cargo_run_bin_with_env(repo: &std::path::Path, bin: &str, env: &[(&str, String)]) -> Result<()> {
    let args: Vec<String> = vec!["run".into(), "--bin".into(), bin.into()];
    print_cmd("cargo", &args);
    let mut cmd = Command::new("cargo");
    cmd.args(&args).current_dir(repo);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .with_context(|| format!("failed to spawn `cargo run --bin {bin}`"))?;
    if !status.success() {
        bail!(
            "`cargo run --bin {}` exited with status {:?}",
            bin,
            status.code()
        );
    }
    Ok(())
}

/// Regenerate `data/ships_extended/` from upstream and fail if the result differs from what is
/// committed — i.e. a source edit landed without its generated output being refreshed.
///
/// `data_version` is stamped with the regeneration date, so it is pinned to the value already
/// committed; otherwise this would report drift on every day except the one the data landed.
/// Officer output is deliberately not checked: `officers.lcars.yaml` is gitignored, and the
/// in-process officer model has its own parity test.
fn check_generated(repo: &std::path::Path) -> Result<()> {
    let index_path = repo.join("data/ships_extended/index.json");
    let committed = std::fs::read_to_string(&index_path)
        .with_context(|| format!("failed to read {}", index_path.display()))?;
    let parsed: serde_json::Value = serde_json::from_str(&committed)
        .with_context(|| format!("failed to parse {}", index_path.display()))?;
    let data_version = parsed
        .get("data_version")
        .and_then(|v| v.as_str())
        .context("data/ships_extended/index.json has no string `data_version` to pin")?
        .to_string();

    cargo_run_bin_with_env(
        repo,
        "normalize_data_stfc_space",
        &[("STFCSPACE_SHIPS_VERSION", data_version)],
    )?;

    check_no_drift(repo, &["data/ships_extended"])
}

/// Fail if any of `paths` has uncommitted changes per `git status --porcelain` — i.e. a generator
/// produced output that differs from what is committed.
fn check_no_drift(repo: &std::path::Path, paths: &[&str]) -> Result<()> {
    let mut args = vec![
        "status".to_string(),
        "--porcelain".to_string(),
        "--".to_string(),
    ];
    args.extend(paths.iter().map(|p| p.to_string()));
    print_cmd("git", &args);
    let out = Command::new("git")
        .args(&args)
        .current_dir(repo)
        .output()
        .context("failed to spawn `git status`")?;
    if !out.status.success() {
        bail!("`git status` exited with status {:?}", out.status.code());
    }
    if !out.stdout.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&out.stdout));
        bail!(
            "Generated data drifted from its sources ({}).\n\
             Regenerate and commit:\n  \
             cargo run --bin normalize_data_stfc_space",
            paths.join(", ")
        );
    }
    Ok(())
}

fn node(repo: &std::path::Path, script: &str, args: &[String]) -> Result<()> {
    let mut v = vec![script.into()];
    v.extend_from_slice(args);
    run(repo, "node", &v)
}

fn npm(repo: &std::path::Path, script: &str, trailing: &[String]) -> Result<()> {
    let mut args = vec!["run".into(), script.into(), "--".into()];
    args.extend_from_slice(trailing);
    run(repo, "npm", &args)
}
