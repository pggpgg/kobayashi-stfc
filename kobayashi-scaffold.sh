#!/usr/bin/env bash
# KOBAYASHI Project Scaffold Generator
# Run this inside your cloned empty repo:
#   chmod +x setup.sh && ./setup.sh

set -e

echo "⚔ Scaffolding KOBAYASHI..."

# ── Directory structure ──────────────────────────────────────
mkdir -p src/{data,lcars,combat,optimizer,parallel,server/static}
mkdir -p data/{officers,profiles}
mkdir -p frontend/src/{components,lib}
mkdir -p tests/fixtures/{officers,recorded_fights}

# ── Cargo.toml ───────────────────────────────────────────────
cat > Cargo.toml << 'EOF'
[package]
name = "kobayashi"
version = "0.1.0"
edition = "2021"
description = "KOBAYASHI — STFC Monte Carlo combat simulator & crew optimizer"
license = "MIT"
repository = "https://github.com/YOUR_USERNAME/kobayashi"
readme = "README.md"

[dependencies]
# Parallelism
rayon = "1.10"
crossbeam = "0.8"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"

# Async runtime + web server
tokio = { version = "1", features = ["full"] }
axum = "0.7"
axum-extra = { version = "0.9", features = ["typed-header"] }
tower-http = { version = "0.5", features = ["fs", "cors"] }

# Utilities
rand = "0.8"
csv = "1.3"
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
rust-embed = "8"
thiserror = "1"
anyhow = "1"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1      # maximize optimization at cost of compile time
target-cpu = "native"  # optimize for the machine it's built on
EOF

# ── src/main.rs ──────────────────────────────────────────────
cat > src/main.rs << 'EOF'
use clap::{Parser, Subcommand};
use tracing::info;

mod data;
mod lcars;
mod combat;
mod optimizer;
mod parallel;
mod server;

#[derive(Parser)]
#[command(name = "kobayashi")]
#[command(about = "⚔ KOBAYASHI — STFC Combat Simulator & Crew Optimizer")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the web interface on localhost
    Serve {
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Run a batch optimization from the command line
    Optimize {
        #[arg(short, long)]
        ship: String,
        #[arg(short = 'e', long)]
        hostile: String,
        #[arg(short, long, default_value = "5000")]
        sims: u32,
        #[arg(short, long, default_value = "10")]
        top: usize,
    },
    /// Simulate a specific crew
    Simulate {
        #[arg(short, long)]
        ship: String,
        #[arg(short = 'e', long)]
        hostile: String,
        #[arg(short, long)]
        captain: String,
        #[arg(short, long)]
        bridge: String,
        #[arg(short = 'l', long)]
        below: String,
        #[arg(short, long, default_value = "10000")]
        sims: u32,
    },
    /// Import officer data from Spocks.club export
    Import {
        #[arg(short, long)]
        file: String,
    },
    /// Validate LCARS officer definition files
    Validate {
        #[arg(short, long, default_value = "data/officers")]
        path: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("kobayashi=info")
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { port } => {
            info!("⚔ KOBAYASHI starting on http://localhost:{}", port);
            server::run(port).await?;
        }
        Commands::Optimize { ship, hostile, sims, top } => {
            info!("Running optimization: {} vs {} ({} sims, top {})", ship, hostile, sims, top);
            // TODO: load data, run optimizer, print results
            println!("Not yet implemented — coming soon!");
        }
        Commands::Simulate { ship, hostile, captain, bridge, below, sims } => {
            info!("Simulating crew: {}/{}/{} on {} vs {} ({} sims)",
                captain, bridge, below, ship, hostile, sims);
            // TODO: load data, run simulation, print results
            println!("Not yet implemented — coming soon!");
        }
        Commands::Import { file } => {
            info!("Importing from: {}", file);
            // TODO: parse import file, merge with officer data
            println!("Not yet implemented — coming soon!");
        }
        Commands::Validate { path } => {
            info!("Validating LCARS files in: {}", path);
            // TODO: load and validate all .lcars.yaml files
            println!("Not yet implemented — coming soon!");
        }
    }

    Ok(())
}
EOF

# ── Module stubs ─────────────────────────────────────────────

# data/
cat > src/data/mod.rs << 'EOF'
pub mod officer;
pub mod ship;
pub mod hostile;
pub mod synergy;
pub mod profile;
pub mod import;
EOF

cat > src/data/officer.rs << 'EOF'
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Officer {
    pub id: String,
    pub name: String,
    pub faction: String,
    pub rarity: Rarity,
    pub group: Option<String>,
    pub captain_ability: Option<Ability>,
    pub bridge_ability: Option<Ability>,
    pub below_decks_ability: Option<Ability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ability {
    pub name: String,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Effect {
    #[serde(rename = "type")]
    pub effect_type: EffectType,
    pub stat: Option<String>,
    pub target: Option<Target>,
    pub operator: Option<Operator>,
    pub value: Option<f64>,
    pub trigger: Option<Trigger>,
    pub duration: Option<Duration>,
    pub decay: Option<Decay>,
    pub accumulate: Option<Accumulate>,
    pub scaling: Option<Scaling>,
    pub condition: Option<Condition>,
    // extra_attack fields
    pub chance: Option<f64>,
    pub multiplier: Option<f64>,
    // tag fields
    pub tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectType {
    StatModify,
    ExtraAttack,
    Tag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    #[serde(rename = "self")]
    Self_,
    Enemy,
    AllAllies,
    AllEnemies,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Add,
    Multiply,
    Set,
    Min,
    Max,
    AddPctOfMax,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Passive,
    OnCombatStart,
    OnRoundStart,
    OnAttack,
    OnHit,
    OnCritical,
    OnShieldBreak,
    OnHullBreach,
    OnKill,
    OnReceiveDamage,
    OnRoundEnd,
    OnCombatEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Duration {
    Simple(DurationKind),
    Complex { rounds: Option<u8>, stacks: Option<u8> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DurationKind {
    Permanent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decay {
    #[serde(rename = "type")]
    pub decay_type: String,   // "linear" | "exponential"
    pub amount: f64,
    pub floor: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Accumulate {
    #[serde(rename = "type")]
    pub acc_type: String,     // "linear" | "exponential" | "step"
    pub amount: f64,
    pub ceiling: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scaling {
    pub base: f64,
    pub per_rank: Option<f64>,
    pub max_rank: Option<u8>,
    pub base_chance: Option<f64>,  // for extra_attack scaling
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    #[serde(rename = "type")]
    pub condition_type: String,
    pub stat: Option<String>,
    pub threshold_pct: Option<f64>,
    pub faction: Option<String>,
    pub min: Option<u8>,
    pub max: Option<u8>,
    pub conditions: Option<Vec<Condition>>,
}
EOF

cat > src/data/ship.rs << 'EOF'
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ship {
    pub id: String,
    pub name: String,
    pub weapon_dmg: f64,
    pub shield_hp: f64,
    pub shield_mitigation: f64,
    pub hull_hp: f64,
    pub armor: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
}
EOF

cat > src/data/hostile.rs << 'EOF'
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hostile {
    pub id: String,
    pub name: String,
    pub level: u32,
    pub weapon_dmg: f64,
    pub shield_hp: f64,
    pub shield_mitigation: f64,
    pub hull_hp: f64,
    pub armor: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub faction: Option<String>,
    pub special_mechanics: Option<Vec<String>>,
}
EOF

cat > src/data/synergy.rs << 'EOF'
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynergyTag {
    pub id: String,
    pub name: String,
    pub officers: Vec<String>,
    pub mechanism: String,
    pub priority: SynergyPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SynergyPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Default)]
pub struct SynergyIndex {
    pub manual: Vec<SynergyTag>,
    pub learned: CoOccurrenceMatrix,
}

#[derive(Debug, Default)]
pub struct CoOccurrenceMatrix {
    // officer_id pair → score boost observed
    pub entries: std::collections::HashMap<(String, String), f64>,
}

impl SynergyIndex {
    pub fn learn_from_results(&mut self, _results: &[()]) {
        // TODO: analyze top-performing crews for co-occurring officer pairs
    }
}
EOF

cat > src/data/profile.rs << 'EOF'
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub name: String,
    pub effective_bonuses: EffectiveBonuses,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveBonuses {
    #[serde(default)]
    pub weapon_damage: f64,
    #[serde(default)]
    pub shield_hp: f64,
    #[serde(default)]
    pub shield_mitigation: f64,
    #[serde(default)]
    pub hull_hp: f64,
    #[serde(default)]
    pub armor: f64,
    #[serde(default)]
    pub crit_chance: f64,
    #[serde(default)]
    pub crit_damage: f64,
}

impl Default for PlayerProfile {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            effective_bonuses: EffectiveBonuses {
                weapon_damage: 0.0,
                shield_hp: 0.0,
                shield_mitigation: 0.0,
                hull_hp: 0.0,
                armor: 0.0,
                crit_chance: 0.0,
                crit_damage: 0.0,
            },
        }
    }
}
EOF

cat > src/data/import.rs << 'EOF'
use anyhow::Result;
use tracing::info;

/// Import officer data from a Spocks.club export file
pub fn import_spocks_export(_path: &str) -> Result<()> {
    info!("Spocks.club import not yet implemented");
    // TODO:
    // 1. Detect format (JSON or CSV)
    // 2. Parse officer names, tiers, levels
    // 3. Map to canonical LCARS officer IDs
    // 4. Compute effective ability values from tier/level
    // 5. Diff against existing data
    // 6. Merge and save
    Ok(())
}
EOF

# lcars/
cat > src/lcars/mod.rs << 'EOF'
pub mod parser;
pub mod schema;
pub mod resolver;
pub mod errors;
EOF

cat > src/lcars/parser.rs << 'EOF'
use std::path::Path;
use anyhow::Result;
use crate::data::officer::Officer;

/// Load all .lcars.yaml files from a directory
pub fn load_officers_from_dir(dir: &Path) -> Result<Vec<Officer>> {
    let mut officers = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "yaml" || e == "yml") {
                let content = std::fs::read_to_string(&path)?;
                let parsed: LcarsFile = serde_yaml::from_str(&content)?;
                officers.extend(parsed.officers);
            }
        }
    }
    Ok(officers)
}

#[derive(serde::Deserialize)]
struct LcarsFile {
    officers: Vec<Officer>,
}
EOF

cat > src/lcars/schema.rs << 'EOF'
use crate::data::officer::Officer;
use crate::lcars::errors::ValidationWarning;

/// Validate an officer definition against the LCARS schema
pub fn validate_officer(officer: &Officer) -> Vec<ValidationWarning> {
    let mut warnings = Vec::new();

    if officer.captain_ability.is_none()
        && officer.bridge_ability.is_none()
        && officer.below_decks_ability.is_none()
    {
        warnings.push(ValidationWarning {
            officer_id: officer.id.clone(),
            message: "Officer has no abilities defined".to_string(),
        });
    }

    // TODO: validate stat names, trigger/duration combos,
    // scaling ranges, condition types, etc.

    warnings
}
EOF

cat > src/lcars/resolver.rs << 'EOF'
use crate::data::officer::Officer;

/// Pre-combat resolved buff set — the output of LCARS resolution.
/// Static buffs are folded into base stats; only dynamic effects
/// remain for per-round evaluation.
#[derive(Debug, Clone, Default)]
pub struct BuffSet {
    // Static (pre-computed before fight loop)
    pub weapon_damage_mult: f64,
    pub shield_pierce: f64,
    pub crit_chance_add: f64,
    pub crit_damage_add: f64,
    pub hull_hp_mult: f64,
    pub armor_add: f64,

    // Dynamic (evaluated per round in the fight loop)
    pub has_decay_damage: bool,
    pub decay_base: f64,
    pub decay_per_round: f64,
    pub decay_floor: f64,

    pub has_extra_attack: bool,
    pub extra_attack_chance: f64,
    pub extra_attack_rounds: u8,

    pub has_on_kill_heal: bool,
    pub on_kill_heal_pct: f64,
}

/// Resolve a crew's LCARS abilities into a flat BuffSet
pub fn resolve_crew(
    captain: &Officer,
    bridge: &Officer,
    below: &Officer,
    _captain_rank: u8,
    _bridge_rank: u8,
    _below_rank: u8,
) -> BuffSet {
    let mut buffs = BuffSet {
        weapon_damage_mult: 1.0,
        hull_hp_mult: 1.0,
        ..Default::default()
    };

    // TODO: iterate over each officer's relevant ability
    // (captain_ability for captain slot, bridge_ability for bridge, etc.)
    // and fold effects into buffs based on effect_type, operator, trigger
    let _ = (captain, bridge, below);

    buffs
}
EOF

cat > src/lcars/errors.rs << 'EOF'
#[derive(Debug)]
pub struct ValidationWarning {
    pub officer_id: String,
    pub message: String,
}

impl std::fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LCARS warning [{}]: {}", self.officer_id, self.message)
    }
}
EOF

# combat/
cat > src/combat/mod.rs << 'EOF'
pub mod engine;
pub mod buffs;
pub mod effects;
pub mod rng;
EOF

cat > src/combat/rng.rs << 'EOF'
/// SplitMix64 — fast, high-quality PRNG (~0.8ns/call, passes BigCrush)
/// Deterministic: same seed always produces same sequence.
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[inline(always)]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    /// Returns a float in [0.0, 1.0)
    #[inline(always)]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}
EOF

cat > src/combat/buffs.rs << 'EOF'
// Buff stacking and resolution logic
// TODO: implement stacking rules:
//   1. base stat
//   2. flat adds (sum)
//   3. pct adds (sum, then apply)
//   4. multipliers (compound)
//   5. min/max caps
EOF

cat > src/combat/effects.rs << 'EOF'
// Per-round effect evaluation: decay, accumulate, triggered effects
// TODO: implement effect tick logic
EOF

cat > src/combat/engine.rs << 'EOF'
use crate::combat::rng::SplitMix64;
use crate::lcars::resolver::BuffSet;

/// Stats needed by the combat engine (ship or hostile)
#[derive(Debug, Clone)]
pub struct CombatStats {
    pub weapon_dmg: f64,
    pub shield_hp: f64,
    pub shield_mitigation: f64,
    pub hull_hp: f64,
    pub armor: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
}

#[derive(Debug, Clone)]
pub struct FightResult {
    pub win: bool,
    pub rounds: u8,
    pub hull_remaining: f64,
    pub hull_pct: f64,
    pub damage_dealt_r1: f64,
}

const MAX_ROUNDS: u8 = 20;

/// Core combat simulation. Pure function, zero allocations.
/// This is the hot path — every nanosecond matters.
#[inline]
pub fn simulate(
    player: &CombatStats,
    hostile: &CombatStats,
    buffs: &BuffSet,
    seed: u64,
) -> FightResult {
    let mut rng = SplitMix64::new(seed);

    // Apply static buffs to player stats
    let p_weapon = player.weapon_dmg * buffs.weapon_damage_mult;
    let p_crit_ch = player.crit_chance + buffs.crit_chance_add;
    let p_crit_dm = player.crit_damage + buffs.crit_damage_add;
    let p_hull_max = player.hull_hp * buffs.hull_hp_mult;
    let p_armor = player.armor + buffs.armor_add;
    let p_shield_mit = player.shield_mitigation;

    let mut p_shield = player.shield_hp;
    let mut p_hull = p_hull_max;
    let mut e_shield = hostile.shield_hp;
    let mut e_hull = hostile.hull_hp;

    let mut dmg_r1: f64 = 0.0;

    for round in 1..=MAX_ROUNDS {
        // ── Player attacks ──────────────────────────────
        let mut dmg_mult = 1.0_f64;

        // Decay damage (e.g., Harrison)
        if buffs.has_decay_damage {
            let bonus = (buffs.decay_base - buffs.decay_per_round * (round as f64 - 1.0))
                .max(buffs.decay_floor);
            dmg_mult *= bonus;
        }

        let mut base_dmg = p_weapon * dmg_mult;

        // Crit roll
        if rng.next_f64() < p_crit_ch {
            base_dmg *= p_crit_dm;
        }

        // Extra attack roll (e.g., Nero)
        let shots = if buffs.has_extra_attack
            && round <= buffs.extra_attack_rounds
            && rng.next_f64() < buffs.extra_attack_chance
        {
            2u8
        } else {
            1u8
        };

        let effective_pierce = buffs.shield_pierce;

        for _ in 0..shots {
            let dealt = apply_damage(
                base_dmg, effective_pierce,
                &mut e_shield, &mut e_hull,
                hostile.shield_mitigation, hostile.armor,
            );
            if round == 1 { dmg_r1 += dealt; }
        }

        // Check kill
        if e_hull <= 0.0 {
            // On-kill heal
            if buffs.has_on_kill_heal {
                p_hull = (p_hull + p_hull_max * buffs.on_kill_heal_pct).min(p_hull_max);
            }
            return FightResult {
                win: true,
                rounds: round,
                hull_remaining: p_hull.max(0.0),
                hull_pct: (p_hull / p_hull_max).max(0.0),
                damage_dealt_r1: dmg_r1,
            };
        }

        // ── Hostile attacks ─────────────────────────────
        let mut e_dmg = hostile.weapon_dmg;
        if rng.next_f64() < hostile.crit_chance {
            e_dmg *= hostile.crit_damage;
        }

        apply_damage(
            e_dmg, 0.0,
            &mut p_shield, &mut p_hull,
            p_shield_mit, p_armor,
        );

        if p_hull <= 0.0 {
            return FightResult {
                win: false,
                rounds: round,
                hull_remaining: 0.0,
                hull_pct: 0.0,
                damage_dealt_r1: dmg_r1,
            };
        }
    }

    FightResult {
        win: false,
        rounds: MAX_ROUNDS,
        hull_remaining: p_hull.max(0.0),
        hull_pct: (p_hull / p_hull_max).max(0.0),
        damage_dealt_r1: dmg_r1,
    }
}

/// Apply damage to a target, handling shield → hull overflow.
/// Returns total effective damage dealt.
#[inline(always)]
fn apply_damage(
    raw_dmg: f64,
    shield_pierce: f64,
    shield: &mut f64,
    hull: &mut f64,
    shield_mit: f64,
    armor: f64,
) -> f64 {
    let mut dealt = 0.0;
    if *shield > 0.0 {
        let effective_mit = (shield_mit - shield_pierce).max(0.0);
        let shield_dmg = raw_dmg * (1.0 - effective_mit);
        if shield_dmg >= *shield {
            let overflow = shield_dmg - *shield;
            dealt += *shield;
            *shield = 0.0;
            let hull_dmg = (overflow - armor).max(0.0);
            *hull -= hull_dmg;
            dealt += hull_dmg;
        } else {
            *shield -= shield_dmg;
            dealt += shield_dmg;
        }
    } else {
        let hull_dmg = (raw_dmg - armor).max(0.0);
        *hull -= hull_dmg;
        dealt += hull_dmg;
    }
    dealt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_player() -> CombatStats {
        CombatStats {
            weapon_dmg: 12000.0,
            shield_hp: 25000.0,
            shield_mitigation: 0.20,
            hull_hp: 40000.0,
            armor: 1500.0,
            crit_chance: 0.10,
            crit_damage: 1.50,
        }
    }

    fn test_hostile() -> CombatStats {
        CombatStats {
            weapon_dmg: 8000.0,
            shield_hp: 15000.0,
            shield_mitigation: 0.15,
            hull_hp: 30000.0,
            armor: 800.0,
            crit_chance: 0.05,
            crit_damage: 1.30,
        }
    }

    #[test]
    fn test_deterministic_results() {
        let p = test_player();
        let h = test_hostile();
        let buffs = BuffSet { weapon_damage_mult: 1.0, hull_hp_mult: 1.0, ..Default::default() };
        let r1 = simulate(&p, &h, &buffs, 42);
        let r2 = simulate(&p, &h, &buffs, 42);
        assert_eq!(r1.rounds, r2.rounds);
        assert_eq!(r1.win, r2.win);
        assert!((r1.hull_pct - r2.hull_pct).abs() < f64::EPSILON);
    }

    #[test]
    fn test_shield_pierce_helps() {
        let p = test_player();
        let h = test_hostile();
        let no_pierce = BuffSet { weapon_damage_mult: 1.0, hull_hp_mult: 1.0, ..Default::default() };
        let with_pierce = BuffSet {
            weapon_damage_mult: 1.0, hull_hp_mult: 1.0,
            shield_pierce: 0.30, ..Default::default()
        };
        let r_no = simulate(&p, &h, &no_pierce, 42);
        let r_yes = simulate(&p, &h, &with_pierce, 42);
        // With shield pierce, should win faster or with more hull
        assert!(r_yes.rounds <= r_no.rounds || r_yes.hull_pct >= r_no.hull_pct);
    }
}
EOF

# optimizer/
cat > src/optimizer/mod.rs << 'EOF'
pub mod monte_carlo;
pub mod crew_generator;
pub mod tiered;
pub mod genetic;
pub mod analytical;
pub mod ranking;
EOF

cat > src/optimizer/monte_carlo.rs << 'EOF'
use crate::combat::engine::{self, CombatStats, FightResult};
use crate::lcars::resolver::BuffSet;

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub win_rate: f64,
    pub avg_rounds: f64,
    pub avg_hull_pct: f64,
    pub r1_kill_rate: f64,
    pub avg_r1_damage: f64,
}

/// Run N simulations of a crew vs a hostile.
/// Each sim uses a deterministic seed derived from its index.
pub fn run(
    player: &CombatStats,
    hostile: &CombatStats,
    buffs: &BuffSet,
    num_sims: u32,
) -> SimulationResult {
    let mut wins = 0u32;
    let mut total_rounds = 0u32;
    let mut total_hull_pct = 0.0f64;
    let mut r1_kills = 0u32;
    let mut total_r1_dmg = 0.0f64;

    for i in 0..num_sims {
        let seed = (i as u64).wrapping_mul(0x9E3779B97F4A7C15).wrapping_add(1);
        let result = engine::simulate(player, hostile, buffs, seed);

        if result.win {
            wins += 1;
            total_rounds += result.rounds as u32;
            total_hull_pct += result.hull_pct;
            if result.rounds == 1 { r1_kills += 1; }
        }
        total_r1_dmg += result.damage_dealt_r1;
    }

    let n = num_sims as f64;
    let w = wins as f64;

    SimulationResult {
        win_rate: w / n,
        avg_rounds: if wins > 0 { total_rounds as f64 / w } else { f64::INFINITY },
        avg_hull_pct: if wins > 0 { total_hull_pct / w } else { 0.0 },
        r1_kill_rate: r1_kills as f64 / n,
        avg_r1_damage: total_r1_dmg / n,
    }
}
EOF

cat > src/optimizer/crew_generator.rs << 'EOF'
// Crew enumeration: exhaustive, synergy-prioritized, and filtered
// TODO: implement CrewGenerator with iter_prioritized()
EOF

cat > src/optimizer/tiered.rs << 'EOF'
// Two-phase optimization: scouting pass → confirmation pass
// TODO: implement tiered simulation runner
EOF

cat > src/optimizer/genetic.rs << 'EOF'
// Genetic algorithm optimizer for large search spaces
// TODO: implement population, crossover, mutation, selection
EOF

cat > src/optimizer/analytical.rs << 'EOF'
// Closed-form expected damage calculator (no RNG)
// Used as a fast pre-filter before Monte Carlo
// TODO: implement expected damage formulas
EOF

cat > src/optimizer/ranking.rs << 'EOF'
use crate::optimizer::monte_carlo::SimulationResult;

#[derive(Debug, Clone)]
pub struct RankedCrew {
    pub captain_id: String,
    pub bridge_id: String,
    pub below_id: String,
    pub result: SimulationResult,
    pub rank: usize,
}

/// Rank crews by primary metric, with tiebreakers.
/// Default: R1 kill rate → win rate → avg rounds (ascending)
pub fn rank(mut crews: Vec<RankedCrew>) -> Vec<RankedCrew> {
    crews.sort_by(|a, b| {
        b.result.r1_kill_rate.partial_cmp(&a.result.r1_kill_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.result.win_rate.partial_cmp(&a.result.win_rate)
                .unwrap_or(std::cmp::Ordering::Equal))
            .then(a.result.avg_rounds.partial_cmp(&b.result.avg_rounds)
                .unwrap_or(std::cmp::Ordering::Equal))
    });
    for (i, crew) in crews.iter_mut().enumerate() {
        crew.rank = i + 1;
    }
    crews
}
EOF

# parallel/
cat > src/parallel/mod.rs << 'EOF'
pub mod pool;
pub mod batch;
pub mod progress;
EOF

cat > src/parallel/pool.rs << 'EOF'
/// Initialize the Rayon thread pool with optimal settings
pub fn init() -> Result<(), rayon::ThreadPoolBuildError> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(num_cpus())
        .build_global()
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
EOF

cat > src/parallel/batch.rs << 'EOF'
// Distribute crew combos across worker threads via Rayon par_iter
// TODO: implement parallel batch runner
EOF

cat > src/parallel/progress.rs << 'EOF'
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Thread-safe progress tracker
pub struct Progress {
    pub completed: AtomicU64,
    pub total: AtomicU64,
}

impl Progress {
    pub fn new(total: u64) -> Arc<Self> {
        Arc::new(Self {
            completed: AtomicU64::new(0),
            total: AtomicU64::new(total),
        })
    }

    pub fn increment(&self) {
        self.completed.fetch_add(1, Ordering::Relaxed);
    }

    pub fn pct(&self) -> f64 {
        let c = self.completed.load(Ordering::Relaxed) as f64;
        let t = self.total.load(Ordering::Relaxed) as f64;
        if t > 0.0 { c / t * 100.0 } else { 0.0 }
    }
}
EOF

# server/
cat > src/server/mod.rs << 'EOF'
pub mod api;
pub mod routes;

pub async fn run(port: u16) -> anyhow::Result<()> {
    let app = routes::create_router();

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("KOBAYASHI web interface: http://localhost:{}", port);
    axum::serve(listener, app).await?;

    Ok(())
}
EOF

cat > src/server/routes.rs << 'EOF'
use axum::{Router, routing::get};

pub fn create_router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
    // TODO: add API routes from api.rs
}

async fn index() -> axum::response::Html<&'static str> {
    axum::response::Html(
        "<h1>⚔ KOBAYASHI</h1><p>Web interface coming soon. API available at /api/</p>"
    )
}

async fn health() -> &'static str {
    "ok"
}
EOF

cat > src/server/api.rs << 'EOF'
// REST + WebSocket API endpoints
// TODO: implement /api/officers, /api/simulate, /api/optimize, etc.
EOF

# ── Sample LCARS officer file ────────────────────────────────
cat > data/officers/augments.lcars.yaml << 'EOF'
officers:
  - id: khan
    name: "Khan Noonien Singh"
    faction: augment
    rarity: epic
    group: "Botany Bay"

    captain_ability:
      name: "Superior Intellect"
      effects:
        - type: stat_modify
          stat: shield_pierce
          target: self
          operator: add
          value: 0.30
          trigger: passive
          duration: permanent
          scaling:
            base: 0.20
            per_rank: 0.025
            max_rank: 5

    bridge_ability:
      name: "Wrath"
      effects:
        - type: stat_modify
          stat: weapon_damage
          target: self
          operator: multiply
          value: 1.15
          trigger: passive
          duration: permanent
          scaling:
            base: 1.08
            per_rank: 0.0175
            max_rank: 5

    below_decks_ability:
      name: "Augmented Blood"
      effects:
        - type: stat_modify
          stat: hull_hp
          target: self
          operator: multiply
          value: 1.10
          trigger: passive
          duration: permanent

  - id: harrison
    name: "Harrison"
    faction: augment
    rarity: epic
    group: "Botany Bay"

    captain_ability:
      name: "First Strike"
      effects:
        - type: stat_modify
          stat: weapon_damage
          target: self
          operator: multiply
          value: 1.60
          trigger: on_round_start
          duration:
            rounds: 1
          decay:
            type: linear
            amount: 0.15
            floor: 1.0
          scaling:
            base: 1.40
            per_rank: 0.05
            max_rank: 5

    bridge_ability:
      name: "Ruthless"
      effects:
        - type: stat_modify
          stat: weapon_damage
          target: self
          operator: multiply
          value: 1.10
          trigger: passive
          duration: permanent

  - id: mudd
    name: "Harry Mudd"
    faction: neutral
    rarity: epic

    captain_ability:
      name: "Con Artist"
      effects:
        - type: stat_modify
          stat: hull_hp
          target: self
          operator: add_pct_of_max
          value: 0.05
          trigger: on_kill
          duration: permanent
        - type: tag
          tag: loot_bonus
          value: 0.25
          trigger: passive

    bridge_ability:
      name: "Swindle"
      effects:
        - type: stat_modify
          stat: weapon_damage
          target: self
          operator: multiply
          value: 1.08
          trigger: passive
          duration: permanent
EOF

# ── Sample ships data ────────────────────────────────────────
cat > data/ships.json << 'EOF'
[
  {
    "id": "enterprise",
    "name": "USS Enterprise",
    "weapon_dmg": 12000,
    "shield_hp": 25000,
    "shield_mitigation": 0.20,
    "hull_hp": 40000,
    "armor": 1500,
    "crit_chance": 0.10,
    "crit_damage": 1.50
  },
  {
    "id": "saladin",
    "name": "Saladin",
    "weapon_dmg": 15000,
    "shield_hp": 18000,
    "shield_mitigation": 0.15,
    "hull_hp": 35000,
    "armor": 1200,
    "crit_chance": 0.12,
    "crit_damage": 1.50
  }
]
EOF

# ── Sample hostiles data ─────────────────────────────────────
cat > data/hostiles.json << 'EOF'
[
  {
    "id": "explorer_30",
    "name": "Explorer Lv30",
    "level": 30,
    "weapon_dmg": 8000,
    "shield_hp": 15000,
    "shield_mitigation": 0.15,
    "hull_hp": 30000,
    "armor": 800,
    "crit_chance": 0.05,
    "crit_damage": 1.30
  },
  {
    "id": "battleship_30",
    "name": "Battleship Lv30",
    "level": 30,
    "weapon_dmg": 10000,
    "shield_hp": 20000,
    "shield_mitigation": 0.20,
    "hull_hp": 40000,
    "armor": 1200,
    "crit_chance": 0.08,
    "crit_damage": 1.40
  }
]
EOF

# ── Sample synergies ─────────────────────────────────────────
cat > data/synergies.json << 'EOF'
[
  {
    "id": "khan_marcus_pierce",
    "name": "Shield Breaker",
    "officers": ["khan", "marcus"],
    "mechanism": "Both add shield_pierce — stacks to ~45%",
    "priority": "high"
  },
  {
    "id": "khan_nero_burst",
    "name": "Alpha Strike",
    "officers": ["khan", "nero"],
    "mechanism": "Shield pierce + double shot = massive R1 burst",
    "priority": "high"
  }
]
EOF

# ── Default player profile ───────────────────────────────────
cat > data/profiles/default.yaml << 'EOF'
name: "default"
effective_bonuses:
  weapon_damage: 0.0
  shield_hp: 0.0
  shield_mitigation: 0.0
  hull_hp: 0.0
  armor: 0.0
  crit_chance: 0.0
  crit_damage: 0.0
EOF

# ── .gitignore ───────────────────────────────────────────────
cat > .gitignore << 'EOF'
/target
/frontend/node_modules
/frontend/dist
*.swp
*.swo
.DS_Store
EOF

# ── LICENSE (MIT) ────────────────────────────────────────────
cat > LICENSE << 'EOF'
MIT License

Copyright (c) 2026 KOBAYASHI Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
EOF

# ── README.md ────────────────────────────────────────────────
cat > README.md << 'EOF'
# ⚔ KOBAYASHI

**Komprehensive Officer Battle Analysis: Your Assets Simulated against Hostiles Iteratively**

A high-performance Monte Carlo combat simulator and crew optimizer for Star Trek Fleet Command.

Officers are described using **LCARS** (Language for Combat Ability Resolution & Simulation), a declarative YAML-based DSL.

## Quick Start

```bash
# Build
cargo build --release

# Start web interface
./target/release/kobayashi serve

# Or run from CLI
./target/release/kobayashi optimize --ship saladin --hostile explorer_30 --sims 5000
./target/release/kobayashi simulate --ship saladin --hostile explorer_30 --captain khan --bridge nero --below tlaan --sims 10000

# Validate officer definitions
./target/release/kobayashi validate --path data/officers

# Import from Spocks.club
./target/release/kobayashi import --file my_export.json
```

## Project Status

🚧 **Early development** — combat engine and LCARS parser are functional, optimizer and web UI are scaffolded.

## Contributing

Officer definitions live in `data/officers/*.lcars.yaml`. See the [LCARS spec](DESIGN.md#3-lcars-language-specification) for the schema. PRs welcome!

## License

MIT
EOF

echo ""
echo "✅ KOBAYASHI scaffolded successfully!"
echo ""
echo "Next steps:"
echo "  1. cd into your repo"
echo "  2. Run: cargo build"
echo "  3. Run: cargo test"
echo "  4. Run: cargo run -- serve"
echo "  5. Open http://localhost:3000"
echo ""
echo "⚔ Live long and optimize."
