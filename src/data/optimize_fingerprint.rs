//! Reuse fingerprint for persisted optimizer observations (roadmap §1.3).
//!
//! [`crate::data::optimize_history`] stores confirmed Monte Carlo aggregates and can hand them back
//! **instead of simulating**. That is only sound while the fight those numbers describe is still the
//! fight this run would produce. Historically nothing recorded the simulator or the data behind an
//! entry, so a combat-engine fix, an upstream data refresh, or a profile edit silently turned cached
//! win rates into wrong answers reported as freshly confirmed.
//!
//! A [`ReuseFingerprint`] is four independent segments, so a reuse path can gate on the parts it
//! actually depends on and a mismatch stays attributable:
//!
//! | segment | covers | changes when |
//! |---|---|---|
//! | `engine` | the simulator itself | combat behavior moves (see [`engine_canary_digest`]), a combat-affecting env flag flips, or [`COMBAT_ENGINE_BEHAVIOR_VERSION`] is bumped |
//! | `data` | repo catalogs | officer/LCARS, ship, hostile, research, support-buff, forbidden-tech, or eligibility data changes |
//! | `profile` | the player's own state | `profile.json` or any synced import under `profiles/{id}/` changes |
//! | `scenario` | the resolved matchup | the resolved ship record (tier/level/components), hostile record, or buff selection changes |
//!
//! Metrics may only be reused when all four match. Crew **identities** stay useful across engine and
//! profile changes, so paths that consume only "who was in the crew" are deliberately left ungated —
//! see [`crate::data::optimize_history`].

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use crate::combat::{
    simulate_combat, Ability, AbilityClass, AbilityEffect, Combatant, CrewConfiguration, CrewSeat,
    CrewSeatContext, SimulationConfig, TimingWindow, TraceMode,
};
use crate::data::data_registry::DataRegistry;
use crate::data::import::ShipEntry;
use crate::data::optimize_observations::stable_text_hash;
use crate::data::profile_index::{
    profile_data_dir, BUFFS_IMPORTED, BUILDINGS_IMPORTED, FORBIDDEN_TECH_IMPORTED, PROFILE_JSON,
    RESEARCH_IMPORTED, ROSTER_IMPORTED, SHIPS_IMPORTED,
};

/// Encoding version of the fingerprint string itself. Bump when the segment layout changes; stored
/// fingerprints with a different schema compare as mismatched (attributed as `"schema"`).
pub const OPTIMIZE_REUSE_FINGERPRINT_SCHEMA: u32 = 1;

/// Manual lever for engine behavior changes that [`engine_canary_digest`] cannot see (a mechanic no
/// canary fight exercises). Bump it when changing `src/combat/`, `src/lcars/`, or
/// `src/optimizer/monte_carlo/` in a way that can move a win rate. The canary digest is the primary,
/// automatic signal — this is belt-and-braces, not the load-bearing part.
pub const COMBAT_ENGINE_BEHAVIOR_VERSION: u32 = 1;

/// Env vars that change simulated outcomes. Their values go into the `engine` segment because they
/// alter stored numbers with nothing else recording them.
const COMBAT_AFFECTING_ENV_VARS: &[&str] = &[
    "KOBAYASHI_COMBAT_EFFECT_SPEC_ENABLE",
    "KOBAYASHI_EXPERIMENTAL_SIMD_DAMAGE_KERNEL",
    "KOBAYASHI_FT_LEVEL_TIER_SCALING",
    "KOBAYASHI_FULLMC_EARLY_STOP",
    "KOBAYASHI_GENETIC_EARLY_STOP",
    "KOBAYASHI_SCOUT_COARSE_MULT",
    "KOBAYASHI_WEAPON_DAMAGE_ADDITIVE_POOL",
];

/// Per-profile files whose contents feed the resolved combat profile. Missing files hash as empty,
/// so synthetic test profiles are stable.
const PROFILE_INPUT_FILES: &[&str] = &[
    PROFILE_JSON,
    RESEARCH_IMPORTED,
    BUILDINGS_IMPORTED,
    FORBIDDEN_TECH_IMPORTED,
    SHIPS_IMPORTED,
    ROSTER_IMPORTED,
    BUFFS_IMPORTED,
];

/// Four-part identity of the run that produced a persisted observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReuseFingerprint {
    pub schema: u32,
    pub engine: u64,
    pub data: u64,
    pub profile: u64,
    pub scenario: u64,
}

impl ReuseFingerprint {
    /// Compact, stable text form persisted on history entries and observation rows.
    pub fn encode(&self) -> String {
        format!(
            "{}:{:016x}:{:016x}:{:016x}:{:016x}",
            self.schema, self.engine, self.data, self.profile, self.scenario
        )
    }

    pub fn decode(encoded: &str) -> Option<Self> {
        let mut parts = encoded.split(':');
        let schema = parts.next()?.parse::<u32>().ok()?;
        let engine = u64::from_str_radix(parts.next()?, 16).ok()?;
        let data = u64::from_str_radix(parts.next()?, 16).ok()?;
        let profile = u64::from_str_radix(parts.next()?, 16).ok()?;
        let scenario = u64::from_str_radix(parts.next()?, 16).ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            schema,
            engine,
            data,
            profile,
            scenario,
        })
    }

    /// The `data` segment alone, for paths that reuse officer **names** rather than metrics.
    pub fn data_segment(&self) -> u64 {
        self.data
    }
}

/// Scenario inputs that determine the fight, gathered from the optimize request.
#[derive(Debug, Clone, Default)]
pub struct ReuseScenarioInputs<'a> {
    pub ship: &'a str,
    pub hostile: &'a str,
    pub ship_tier: Option<u32>,
    pub ship_level: Option<u32>,
    pub below_decks_slots: u32,
    pub enemy_type: Option<&'a str>,
    pub defender_opponent: &'a str,
    pub support_buffs: Option<&'a [String]>,
    pub defender_support_buffs: Option<&'a [String]>,
    pub defender_alliance_debuffs: Option<&'a [String]>,
    /// PvP: the fixed defender ship and the opponent profile whose own state changes the fight.
    pub defender_ship: Option<&'a str>,
    pub defender_ship_tier: Option<u32>,
    pub defender_ship_level: Option<u32>,
    pub defender_profile_id: Option<&'a str>,
}

/// Which segment differs first, for notes, logs, and the observations inspector. `None` when the two
/// fingerprints are equal.
pub fn first_mismatched_component(stored: &str, current: &str) -> Option<&'static str> {
    if stored == current {
        return None;
    }
    let (Some(s), Some(c)) = (
        ReuseFingerprint::decode(stored),
        ReuseFingerprint::decode(current),
    ) else {
        return Some("schema");
    };
    if s.schema != c.schema {
        return Some("schema");
    }
    if s.engine != c.engine {
        return Some("engine");
    }
    if s.data != c.data {
        return Some("data");
    }
    if s.profile != c.profile {
        return Some("profile");
    }
    if s.scenario != c.scenario {
        return Some("scenario");
    }
    None
}

/// Full fingerprint for one optimize run.
pub fn compute_reuse_fingerprint(
    registry: &DataRegistry,
    profile_id: Option<&str>,
    inputs: &ReuseScenarioInputs<'_>,
) -> ReuseFingerprint {
    ReuseFingerprint {
        schema: OPTIMIZE_REUSE_FINGERPRINT_SCHEMA,
        engine: engine_fingerprint(),
        data: data_catalog_fingerprint(registry),
        profile: profile_inputs_fingerprint(profile_id),
        scenario: scenario_fingerprint(registry, inputs),
    }
}

// --- engine ---------------------------------------------------------------------------------

static ENGINE_FINGERPRINT: OnceLock<u64> = OnceLock::new();

/// `COMBAT_ENGINE_BEHAVIOR_VERSION` + crate version + combat-affecting env values + the canary
/// digest. Computed once per process.
pub fn engine_fingerprint() -> u64 {
    *ENGINE_FINGERPRINT.get_or_init(|| {
        let mut canonical = format!(
            "behavior={COMBAT_ENGINE_BEHAVIOR_VERSION};crate={};",
            env!("CARGO_PKG_VERSION")
        );
        for name in COMBAT_AFFECTING_ENV_VARS {
            let value = std::env::var(name).unwrap_or_default();
            canonical.push_str(name);
            canonical.push('=');
            canonical.push_str(value.trim());
            canonical.push(';');
        }
        canonical.push_str(&format!("canary={:016x}", engine_canary_digest()));
        stable_text_hash(&canonical)
    })
}

/// Digest of a small suite of **fully synthetic** fights — combatants built in code, fixed seeds, no
/// registry data — so any engine change that moves those numbers invalidates cached metrics without
/// anyone remembering to bump a constant.
///
/// A canary only sees the paths it exercises. The three fights below cover distinct mechanic
/// families (plain damage/mitigation/crit/pierce; shields + burning + hull regen + round-end damage;
/// accumulating multipliers + isolytic/apex). **When you add a new mechanic family to the engine, add
/// a fight here**, and bump [`COMBAT_ENGINE_BEHAVIOR_VERSION`] for anything the suite cannot see.
pub fn engine_canary_digest() -> u64 {
    stable_text_hash(&canary_fight_summaries().concat())
}

/// One `label:outcome` line per canary fight. Split out so a test can assert the suite is not
/// degenerate (all fights ending identically would make the digest blind).
fn canary_fight_summaries() -> Vec<String> {
    canary_fights()
        .into_iter()
        .map(|(label, attacker, defender, config, crew)| {
            let result = simulate_combat(&attacker, &defender, &config, &crew);
            format!(
                "{label}:dmg={:.6};won={};limit={};rounds={};ahull={:.6};dhull={:.6};dshield={:.6};ashield={:.6};iso={:.6};",
                result.total_damage,
                result.attacker_won,
                result.winner_by_round_limit,
                result.rounds_simulated,
                result.attacker_hull_remaining,
                result.defender_hull_remaining,
                result.defender_shield_remaining,
                result.attacker_shield_remaining,
                result.total_isolytic_damage,
            )
        })
        .collect()
}

fn canary_combatant(id: &str) -> Combatant {
    Combatant {
        id: id.to_string(),
        attack: 0.0,
        mitigation: 0.0,
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
        weapons: Vec::new(),
        hostile_mitigation_params: None,
    }
}

fn canary_config(rounds: u32, seed: u64) -> SimulationConfig {
    SimulationConfig {
        rounds,
        seed,
        trace_mode: TraceMode::Off,
        ..SimulationConfig::default()
    }
}

fn canary_seat(
    seat: CrewSeat,
    class: AbilityClass,
    timing: TimingWindow,
    effect: AbilityEffect,
) -> CrewSeatContext {
    CrewSeatContext::legacy(
        seat,
        Ability {
            name: "canary".to_string(),
            class,
            timing,
            boostable: false,
            effect,
            condition: None,
            weapon_scope: Default::default(),
        },
        false,
    )
}

type CanaryFight = (
    &'static str,
    Combatant,
    Combatant,
    SimulationConfig,
    CrewConfiguration,
);

fn canary_fights() -> Vec<CanaryFight> {
    // (a) plain damage: mitigation, pierce, crit, counter-fire.
    let mut a_attacker = canary_combatant("canary_a_attacker");
    a_attacker.attack = 500.0;
    a_attacker.pierce = 200.0;
    a_attacker.crit_chance = 0.25;
    a_attacker.crit_multiplier = 1.5;
    let mut a_defender = canary_combatant("canary_a_defender");
    a_defender.attack = 300.0;
    a_defender.mitigation = 300.0;
    a_defender.armor = 100.0;
    a_defender.shield_deflection = 100.0;
    a_defender.dodge = 100.0;

    // (b) shields, burning, hull regen, round-end damage.
    let mut b_attacker = canary_combatant("canary_b_attacker");
    b_attacker.attack = 400.0;
    b_attacker.shield_health = 500.0;
    let mut b_defender = canary_combatant("canary_b_defender");
    b_defender.attack = 250.0;
    b_defender.shield_health = 800.0;
    b_defender.end_of_round_damage = 40.0;
    let b_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::AttackPhase,
                AbilityEffect::Burning {
                    chance: 1.0,
                    duration_rounds: 3,
                },
            ),
            canary_seat(
                CrewSeat::BelowDeck,
                AbilityClass::BelowDeck,
                TimingWindow::RoundStart,
                AbilityEffect::HullRegenMaxFraction(0.05),
            ),
            canary_seat(
                CrewSeat::BelowDeck,
                AbilityClass::BelowDeck,
                TimingWindow::RoundStart,
                AbilityEffect::ShieldRegenMaxFraction(0.1),
            ),
        ],
    };

    // (c) multi-round accumulation plus isolytic / apex routing.
    let mut c_attacker = canary_combatant("canary_c_attacker");
    c_attacker.attack = 600.0;
    c_attacker.isolytic_damage = 0.2;
    c_attacker.apex_shred = 0.1;
    let mut c_defender = canary_combatant("canary_c_defender");
    c_defender.attack = 350.0;
    c_defender.hull_health = 4000.0;
    c_defender.apex_barrier = 0.25;
    c_defender.isolytic_defense = 0.05;
    let c_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                TimingWindow::RoundStart,
                AbilityEffect::AccumulatingAttackMultiplier {
                    initial: 1.0,
                    growth_per_round: 0.1,
                    ceiling: 1.5,
                },
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::AttackPhase,
                AbilityEffect::CritChanceBonus(0.2),
            ),
        ],
    };

    vec![
        (
            "a_plain",
            a_attacker,
            a_defender,
            canary_config(12, 7),
            CrewConfiguration::default(),
        ),
        (
            "b_sustain",
            b_attacker,
            b_defender,
            canary_config(20, 11),
            b_crew,
        ),
        (
            "c_growth",
            c_attacker,
            c_defender,
            canary_config(25, 13),
            c_crew,
        ),
    ]
}

// --- data ------------------------------------------------------------------------------------

static DATA_FINGERPRINT: OnceLock<u64> = OnceLock::new();

/// Digest of the catalogs the registry loaded. Computed from **in-memory** data (not files on disk)
/// so it describes what this process is actually simulating with, and cached for the process because
/// [`DataRegistry::load`] runs once at startup behind an `Arc`.
pub fn data_catalog_fingerprint(registry: &DataRegistry) -> u64 {
    *DATA_FINGERPRINT.get_or_init(|| {
        let mut canonical = String::with_capacity(4096);

        // LCARS officers are the sole ability source and are built in-process from the canonical
        // catalog + upstream stats + translations, sorted by id — so one digest covers all of them.
        // No LCARS type contains a HashMap, so this serialization is byte-deterministic.
        match registry.lcars_officers() {
            Some(officers) => {
                canonical.push_str(&format!("lcars_count={};", officers.len()));
                if let Ok(json) = serde_json::to_string(officers) {
                    canonical.push_str(&format!("lcars={:016x};", stable_text_hash(&json)));
                }
            }
            None => canonical.push_str("lcars=absent;"),
        }
        canonical.push_str(&format!("officers={};", registry.officers().len()));

        // BTreeMap-backed, so deterministic.
        match registry.eligibility_matrix() {
            Some(matrix) => {
                if let Ok(json) = serde_json::to_string(matrix.as_ref()) {
                    canonical.push_str(&format!("eligibility={:016x};", stable_text_hash(&json)));
                }
            }
            None => canonical.push_str("eligibility=absent;"),
        }

        push_optional_json_digest(
            &mut canonical,
            "research",
            registry.research_catalog().map(serde_json::to_string),
        );
        push_optional_json_digest(
            &mut canonical,
            "forbidden_chaos",
            registry
                .forbidden_chaos_catalog()
                .map(serde_json::to_string),
        );

        // Support buffs are HashMap-backed: sort the ids and fold each definition's Debug form.
        match registry.support_buffs_catalog() {
            Some(catalog) => {
                let mut ids: Vec<&String> = catalog.known_ids().collect();
                ids.sort();
                let mut buf = String::new();
                for id in ids {
                    buf.push_str(id);
                    if let Some(def) = catalog.get(id) {
                        buf.push_str(&format!("={def:?};"));
                    }
                }
                canonical.push_str(&format!("support_buffs={:016x};", stable_text_hash(&buf)));
            }
            None => canonical.push_str("support_buffs=absent;"),
        }

        // Per-record ship/hostile JSON is covered by the `scenario` segment (resolved records); the
        // index carries the upstream provenance for the catalog as a whole.
        match registry.ship_index() {
            Some(index) => canonical.push_str(&format!(
                "ships={};ship_data_version={};",
                index.ships.len(),
                index.data_version.as_deref().unwrap_or("")
            )),
            None => canonical.push_str("ships=absent;"),
        }
        match registry.hostile_index() {
            Some(index) => canonical.push_str(&format!(
                "hostiles={};hostile_data_version={};",
                index.hostiles.len(),
                index.data_version.as_deref().unwrap_or("")
            )),
            None => canonical.push_str("hostiles=absent;"),
        }

        // Hostile ability catalog: loaded lazily outside the registry, and a data refresh changes it.
        canonical.push_str(&format!(
            "hostile_abilities={:016x};",
            file_digest(Path::new(
                crate::data::hostile_ability_resolve::DEFAULT_HOSTILE_ABILITY_CATALOG_PATH
            ))
        ));

        canonical.push_str(&format!("hull_ids={};", registry.hull_id_registry().len()));

        stable_text_hash(&canonical)
    })
}

/// Officer-catalog digest alone, for `GET /api/data/version`.
pub fn officer_catalog_digest(registry: &DataRegistry) -> u64 {
    match registry.lcars_officers() {
        Some(officers) => serde_json::to_string(officers)
            .map(|json| stable_text_hash(&json))
            .unwrap_or(0),
        None => 0,
    }
}

fn push_optional_json_digest(
    out: &mut String,
    label: &str,
    json: Option<Result<String, serde_json::Error>>,
) {
    match json {
        Some(Ok(json)) => out.push_str(&format!("{label}={:016x};", stable_text_hash(&json))),
        Some(Err(_)) => out.push_str(&format!("{label}=unserializable;")),
        None => out.push_str(&format!("{label}=absent;")),
    }
}

// --- profile ---------------------------------------------------------------------------------

/// Digest of the player's own state: the raw bytes of every combat-relevant file under
/// `profiles/{id}/`.
///
/// Raw bytes rather than the merged effective profile on purpose. The merged profile is only built
/// deep inside scenario construction — after the reuse decision has to be made — and
/// `PlayerProfile::bonuses` is a `HashMap` whose serialization order varies per process, which would
/// make the fingerprint never match and silently disable the cache. File bytes are order-stable and
/// fail in the safe direction: a whitespace-only edit costs one re-simulation.
pub fn profile_inputs_fingerprint(profile_id: Option<&str>) -> u64 {
    let Some(pid) = profile_id.map(str::trim).filter(|p| !p.is_empty()) else {
        return stable_text_hash("profile=none");
    };
    let dir = profile_data_dir(pid);
    let mut canonical = format!("profile_id={pid};");
    for name in PROFILE_INPUT_FILES {
        canonical.push_str(&format!("{name}={:016x};", file_digest(&dir.join(name))));
    }
    stable_text_hash(&canonical)
}

fn file_digest(path: &Path) -> u64 {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut hash = 0xcbf2_9ce4_8422_2325u64;
            for b in &bytes {
                hash ^= u64::from(*b);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            hash
        }
        Err(_) => 0,
    }
}

// --- scenario --------------------------------------------------------------------------------

/// Digest of the resolved matchup. Uses the **resolved** `ShipRecord` and `HostileRecord` rather than
/// their ids, so a data refresh that rewrites `data/ships_extended/{id}.json` or
/// `data/hostiles/{id}.json` under the same id is detected. Both lookups are LRU-cached in the
/// registry. Neither record type contains a `HashMap`, so the serialization is deterministic.
///
/// Component overrides from the player's synced ships are covered by the `profile` segment
/// (`ships.imported.json`), so the catalog record is resolved here without them.
pub fn scenario_fingerprint(registry: &DataRegistry, inputs: &ReuseScenarioInputs<'_>) -> u64 {
    let mut canonical = format!(
        "ship={};hostile={};tier={};level={};bd_slots={};enemy_type={};defender_opponent={};",
        inputs.ship,
        inputs.hostile,
        inputs.ship_tier.unwrap_or(0),
        inputs.ship_level.unwrap_or(0),
        inputs.below_decks_slots,
        inputs.enemy_type.unwrap_or(""),
        inputs.defender_opponent,
    );

    push_ship_record_digest(
        &mut canonical,
        registry,
        "ship_record",
        inputs.ship,
        inputs.ship_tier,
        inputs.ship_level,
    );

    match registry
        .resolve_hostile(inputs.hostile)
        .and_then(|r| serde_json::to_string(&r).ok())
    {
        Some(json) => {
            canonical.push_str(&format!("hostile_record={:016x};", stable_text_hash(&json)))
        }
        None => canonical.push_str("hostile_record=unresolved;"),
    }

    push_buff_ids(&mut canonical, "support_buffs", inputs.support_buffs);
    push_buff_ids(
        &mut canonical,
        "defender_support_buffs",
        inputs.defender_support_buffs,
    );
    push_buff_ids(
        &mut canonical,
        "defender_alliance_debuffs",
        inputs.defender_alliance_debuffs,
    );

    // PvP: the defender ship and the opponent's own profile change the fight just as ours does.
    if let Some(defender_ship) = inputs.defender_ship {
        canonical.push_str(&format!(
            "defender_ship={defender_ship};defender_tier={};defender_level={};",
            inputs.defender_ship_tier.unwrap_or(0),
            inputs.defender_ship_level.unwrap_or(0),
        ));
        push_ship_record_digest(
            &mut canonical,
            registry,
            "defender_ship_record",
            defender_ship,
            inputs.defender_ship_tier,
            inputs.defender_ship_level,
        );
    }
    if let Some(defender_pid) = inputs.defender_profile_id {
        canonical.push_str(&format!(
            "defender_profile={:016x};",
            profile_inputs_fingerprint(Some(defender_pid))
        ));
    }

    stable_text_hash(&canonical)
}

fn push_ship_record_digest(
    out: &mut String,
    registry: &DataRegistry,
    label: &str,
    ship: &str,
    tier: Option<u32>,
    level: Option<u32>,
) {
    let no_imported: &[ShipEntry] = &[];
    match registry
        .resolve_ship_with_tier_level_and_imported_components(ship, tier, level, no_imported)
        .and_then(|record| serde_json::to_string(&record).ok())
    {
        Some(json) => out.push_str(&format!("{label}={:016x};", stable_text_hash(&json))),
        None => out.push_str(&format!("{label}=unresolved;")),
    }
}

fn push_buff_ids(out: &mut String, label: &str, ids: Option<&[String]>) {
    out.push_str(label);
    out.push('=');
    if let Some(ids) = ids {
        // Selection order is not meaningful; dedupe and sort so it cannot flip the fingerprint.
        let sorted: BTreeMap<&str, ()> = ids.iter().map(|id| (id.trim(), ())).collect();
        for id in sorted.keys() {
            out.push_str(id);
            out.push(',');
        }
    }
    out.push(';');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ReuseFingerprint {
        ReuseFingerprint {
            schema: OPTIMIZE_REUSE_FINGERPRINT_SCHEMA,
            engine: 0x1111_2222_3333_4444,
            data: 0xaaaa_bbbb_cccc_dddd,
            profile: 1,
            scenario: u64::MAX,
        }
    }

    #[test]
    fn encode_decode_roundtrip() {
        let fp = sample();
        let encoded = fp.encode();
        assert_eq!(ReuseFingerprint::decode(&encoded), Some(fp));
        assert!(encoded.len() <= 96, "unexpectedly long: {encoded}");
    }

    #[test]
    fn decode_rejects_malformed_strings() {
        assert!(ReuseFingerprint::decode("").is_none());
        assert!(ReuseFingerprint::decode("1:2:3:4").is_none());
        assert!(ReuseFingerprint::decode("1:2:3:4:5:6").is_none());
        assert!(ReuseFingerprint::decode("x:2:3:4:5").is_none());
    }

    #[test]
    fn first_mismatched_component_names_the_changed_segment() {
        let base = sample();
        let current = base.encode();
        assert_eq!(first_mismatched_component(&current, &current), None);

        let mut engine = base;
        engine.engine ^= 1;
        assert_eq!(
            first_mismatched_component(&engine.encode(), &current),
            Some("engine")
        );

        let mut data = base;
        data.data ^= 1;
        assert_eq!(
            first_mismatched_component(&data.encode(), &current),
            Some("data")
        );

        let mut profile = base;
        profile.profile ^= 1;
        assert_eq!(
            first_mismatched_component(&profile.encode(), &current),
            Some("profile")
        );

        let mut scenario = base;
        scenario.scenario ^= 1;
        assert_eq!(
            first_mismatched_component(&scenario.encode(), &current),
            Some("scenario")
        );

        let mut schema = base;
        schema.schema += 1;
        assert_eq!(
            first_mismatched_component(&schema.encode(), &current),
            Some("schema")
        );
        assert_eq!(
            first_mismatched_component("not-a-fingerprint", &current),
            Some("schema")
        );
    }

    #[test]
    fn engine_canary_digest_is_deterministic_and_nontrivial() {
        let first = engine_canary_digest();
        assert_eq!(first, engine_canary_digest());
        assert_ne!(first, 0);
        // An empty suite would hash the empty string; make sure the fights actually ran.
        assert_ne!(first, stable_text_hash(""));
    }

    #[test]
    fn engine_fingerprint_is_stable_within_the_process() {
        assert_eq!(engine_fingerprint(), engine_fingerprint());
    }

    /// A suite whose fights all end the same way would hash the same after most engine changes.
    /// Every fight must reach a distinct outcome, and none may end instantly.
    #[test]
    fn canary_fights_are_not_degenerate() {
        let summaries = canary_fight_summaries();
        assert_eq!(summaries.len(), 3, "expected three canary fights");
        for summary in &summaries {
            assert!(
                !summary.contains("rounds=0"),
                "canary fight ended before round 1: {summary}"
            );
            assert!(
                !summary.contains("dmg=0.000000"),
                "canary fight dealt no damage: {summary}"
            );
        }
        let unique: std::collections::BTreeSet<&str> = summaries
            .iter()
            .map(|s| s.split(':').nth(1).unwrap_or_default())
            .collect();
        assert_eq!(
            unique.len(),
            summaries.len(),
            "canary fights must not share an outcome: {summaries:?}"
        );
    }

    #[test]
    fn profile_fingerprint_distinguishes_profiles_and_survives_missing_dirs() {
        let missing_a = profile_inputs_fingerprint(Some("__kobayashi_absent_a"));
        let missing_b = profile_inputs_fingerprint(Some("__kobayashi_absent_b"));
        assert_ne!(
            missing_a, missing_b,
            "profile id must be part of the digest"
        );
        assert_eq!(
            missing_a,
            profile_inputs_fingerprint(Some("__kobayashi_absent_a"))
        );
        assert_ne!(missing_a, profile_inputs_fingerprint(None));
    }

    /// Canary for a future `serde_json` `preserve_order` feature: if map serialization ever becomes
    /// insertion-ordered, the data and scenario segments stop matching across processes and the
    /// cache silently dies. Fail loudly here instead.
    #[test]
    fn json_object_serialization_is_key_ordered() {
        let value: serde_json::Value =
            serde_json::from_str(r#"{"zebra":1,"apple":2,"mango":3}"#).expect("json");
        assert_eq!(
            serde_json::to_string(&value).expect("json"),
            r#"{"apple":2,"mango":3,"zebra":1}"#,
            "serde_json must stay BTreeMap-backed (no preserve_order feature)"
        );
    }

    #[test]
    fn buff_ids_are_order_and_duplicate_insensitive() {
        let mut a = String::new();
        let mut b = String::new();
        push_buff_ids(
            &mut a,
            "x",
            Some(&["beta".to_string(), "alpha".to_string(), "beta".to_string()]),
        );
        push_buff_ids(
            &mut b,
            "x",
            Some(&["alpha".to_string(), "beta".to_string()]),
        );
        assert_eq!(a, b);
    }
}
