//! Discoverable maintenance tasks for the Kobayashi repo.
//! Run from repo root: `cargo xtask --help`

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
