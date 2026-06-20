//! Discoverable maintenance tasks for the Kobayashi repo.
//! Run from repo root: `cargo xtask --help`

mod bench_check;

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
