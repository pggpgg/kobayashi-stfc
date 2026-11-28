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
//! | `data` | repo catalogs | officer/LCARS, building, ship, hostile, research, support-buff, forbidden-tech, or eligibility data changes |
//! | `profile` | the player's own state | `profile.json` or any synced import under `profiles/{id}/` changes |
//! | `scenario` | the resolved matchup | the resolved ship record (tier/level/components), hostile record, or buff selection changes |
//!
//! Metrics may only be reused when all four match. Crew **identities** stay useful across engine and
//! profile changes, so paths that consume only "who was in the crew" are deliberately left ungated —
//! see [`crate::data::optimize_history`].

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

use crate::combat::effect_spec_compile::compile_officer_combat_spec;
use crate::combat::{
    simulate_combat, Ability, AbilityClass, AbilityCondition, AbilityEffect, Combatant,
    CrewConfiguration, CrewSeat, CrewSeatContext, SimulationConfig, TimingWindow, TraceMode,
};
use crate::data::combat_effect_spec::{
    AbilityModifierSpec, AbilityOperationSpec, AbilityTargetSpec, AbilityTriggerSpec, ChanceSpec,
    CombatEffectSpec, DurationSpec, EffectSource, ValueSpec,
};
use crate::data::data_registry::DataRegistry;
use crate::data::import::{GlobalBuffEntry, ShipEntry};
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
///
/// * 1 → 2 (2026-08): hostile ability values are resolved from the upstream level curve rather
///   than always `values[0]` (`level_curve_index` in
///   [`crate::data::hostile_ability_resolve`]). This moves defender-side ability magnitudes for
///   19 abilities across 163 hostiles. The canary fights build their combatants in code and never
///   touch a hostile record, so the digest cannot see it — this bump is what invalidates metrics
///   persisted before the change.
/// * 2 → 3 (2026-10): research snapshots select the owned rank; grade-gated research
///   scales all three base piercing stats during scenario compilation. Synthetic
///   engine canaries do not load profiles/catalogs, so they cannot detect this change.
/// * 3 → 4 (2026-10): conditional research uses raw defensive-stat bonuses and
///   morale-gated shield repair. Raw defense context now selects the same inbound
///   mitigation baseline with inactive or active research; scenario wiring carries
///   opponent weapon stats and preserves raw-vs-fractional officer Defense units.
/// * 4 → 5 (2026-10): captain officer abilities activate; finite enemy-hit
///   bonuses start after firing; raw officer-defense points use the nonlinear
///   mitigation path; profile bonuses propagate into individual weapons. Native
///   setup families use active source membership and shared HP pools; shield
///   over-assignment is discarded; verified native damage percentages share the
///   weapon base pool. Native critical-bonus sources convert at the data boundary.
/// * 5 → 6 (2026-10): native launch defense pools replace legacy research and
///   building mappings; separately ranked CombatStart defense sources retain
///   their opponent gates. Profile-free canaries cannot detect source selection.
/// * 6 → 7 (2026-10): Orlop Defense retains its native fractional rank curve
///   and below-decks-only officer-rating scope instead of a crew-wide bonus.
/// * 7 → 8 (2026-10): exact native group-66 Eclipse defense BID 1238121830
///   applies at CombatStart only against defender hull faction 1750120904.
/// * 8 → 9 (2026-10): mitigation retains the native float32 offset rather than
///   substituting decimal 1.1; recorded Jem'Hadar and Exchange controls cover it.
/// * 9 → 10 (2026-10): source-bound officer models use native integer CoreStats
///   from weights and float32 growth; custom LCARS stats retain their values.
///   Profile-free engine canaries do not load this officer-model source boundary.
/// * 10 → 11 (2026-10): genetic finalists retain the search scenario and repaired
///   crews cannot duplicate officers. Old optimizer metrics can describe a
///   different tier/profile scenario and must not be reused after these fixes.
/// * 11 → 12 (2026-10): conditional health pools, reactive counter-fire and
///   lethal regeneration are corrected; unsupported officer gates fail closed.
///   Additive officer/critical buffs and ship accuracy retain their native units.
///   Optimizer checkpoints, crew identity, partial seeds, learned-score cohorts
///   and defender ship seat schedules also change persisted result semantics.
/// * 12 → 13 (2026-10): native NPC offensive buffs and active piercing/officer
///   research sources are compiled; Hull Breach isolytic research uses RoundStart.
///   Survival metrics use actual attacker HP, including conditional maximum HP.
///   Profile-free canaries cannot observe source membership or optimizer metrics.
/// * 16 → 17 (2026-11): complete native officer-stat setup pools retain captain,
///   bridge and below-decks membership through scenario and cached-round paths.
///   Canary f covers native source eligibility and scoped breakpoint lookup;
///   synthetic fights cannot detect omission of rows by a scenario caller.
/// * 17 → 18 (2026-11): active Syndicate launch piercing and archive NPC piercing
///   enter their respective native pools; exact Stella archive Apex retains its
///   owner-hull and Eclipse Armada gates. Canaries cover compilation, while the
///   version also invalidates metrics from scenario callers missing these seats.
///   Monaveen's twelve PvP Apex rewards retain each owning crew's Tal gate.
/// * 18 → 19 (2026-11): native crew launch HP joins the account percentage pool;
///   resolved ship/grade/faction HP gates, Monaveen's Texas-only base damage,
///   and source-joined officer synergy affect scenario construction
///   outside the synthetic engine canaries.
/// * 19 → 20 (2026-11): 0718's crew-wide Defense changes the scenario's rating
///   before ship breakpoints. The engine canaries do not load the officer catalog.
///   Powered Armor's fight-start hull and raw Armor behavior has a separate canary.
/// * 20 → 21 (2026-11): owned generator bonuses require the saved Syndicate
///   level during profile setup. Canary r exercises the gate with synthetic data.
/// * 22 → 23 (2026-11): unsupported officer crit triggers and opponent crit
///   damage reductions stop compiling as friendly bonuses. The pre-resolved
///   engine canaries do not exercise the canonical officer converter.
/// * 23 → 24 (2026-11): crew-wide launch Attack now contributes to ship breakpoints.
///   Canary f covers the crew rating pool. The source mapping itself is outside
///   the synthetic canaries.
/// * 24 → 25 (2026-11): active native building buffs reach combat setup without
///   an owned-building import. Canary f covers the shared merge; the canary
///   fights do not exercise profile file discovery or scenario routing.
/// * 25 → 26 (2026-11): D4 Relentless Hunting retains the source's decimal
///   fractions. Canary v covers combat application but does not load the curve.
/// * 26 → 27 (2026-11): canonical opponent weapon damage reductions no longer
///   compile as friendly attack bonuses. The canaries use pre-resolved seats.
/// * 27 → 28 (2026-11): combat-start weapon damage retains canonical duration.
///   Canary w checks combat expiry. The canaries do not load the officer catalog.
/// * 28 → 29 (2026-11): standalone scenarios load the building pools before
///   research. Canary f exercises building pools without profile file loading.
/// * 29 → 30 (2026-11): Enterprise-D firing cadence and eligible-round Galaxy
///   Class stacks. The canaries do not load normalized ship records.
/// * 30 → 31 (2026-11): Honor upstream `warm_up` / `cool_down` weapon firing
///   schedules for all normalized player ships and hostiles (#419). The canaries
///   do not load normalized ship or hostile records.
/// * 31 → 32 (2026-11): Track C officer fidelity push — opponent weapon damage
///   reduction (`ModAllDamage MultiplySub`), seat-scoped launch officer stat pools
///   (`SelfAll`/`SelfBridge`/`SelfCaptain`/`SelfOfficer`), reactive damage-taken
///   triggers (`HullDamageTaken`/`ShieldDamageTaken`/`CriticalShotTaken`/`HitTaken`),
///   non-crew-scaled raw `AllDefenses`/`AllPiercing` (self + enemy reduction), and
///   timed/cumulative/on-hit enemy `CritChance Sub` (symmetric PvP). New canaries
///   exercise the engine paths while the version invalidates catalog-resolved crews.
/// * 32 → 33 (2026-11): Update 96 mechanics & onboarding — `WeaponType::Polaron`
///   (upstream `weapon_type == 6` bypass of Standard Mitigation, Apex Barrier,
///   Critical Mitigation, and Shields), Polaron Mitigation & multiplier, Infusion
///   Capacitors & Spike (`∄` shield/mitigation bypass), stacked `CritMitigationRating`,
///   Polaron Prototype hostile abilities (Inertial Diverters, Shield Sweep, Polaron
///   Weaponry, Thorn in My Side, Not Worthy, Test of Power), Wave Defense / FKR /
///   Outpost canonical gates, and conditional FT routing for Silicon-based Parasites
///   and `[PRO] Infusion Modulator`.
/// * 33 → 34 (2026-11): End-to-end combat context wiring — `enemy_type` override
///   and additive `wave_defense` propagate into `SharedScenarioData`, `SimulationConfig`,
///   and `OfficerStatConditionContext` (fixing multi-word `EnemyType` keys like
///   `solo_armadas` and PvP defender faction); fixed `resolve_enemy_type` dead branches
///   for Q-Trials, Outposts, and Mission Bosses; fixed `weapon_damage_vs_armada_hostile`
///   and `weapon_damage_vs_outpost` profile additive weapon pool application; mapped
///   `EnemySentinel`, `TargetNotPlayerStation`, `SelfAtAssault2`, `TargetIsInvadingEntity`,
///   `TargetNotInvadingEntity`, `TargetIsArmadaOrInvadingEntity`, and Caleb Mir's
///   `EnemyNonArmadaHostileOrOutpost` / `EnemyOutpostOrRetaliator` gates.
/// * 34 → 35 (2026-11): Hostile auto-inference catalog enrichment — verified Solo Armadas
///   (Dominion Flagships, Borg Spheres/Expansion Cubes, Conqueror Borg Solo Armadas, Arena
///   Solo Armada) auto-infer as `solo_armadas`, verified Outpost Retaliators (Cardassian/Node/
///   Borg Galor-Class hostiles) auto-infer as `outpost_retaliation_attackers`, and canonical
///   Borg `faction.loca_id: 13` maps to `OpponentFactionTag::Borg`.
/// * 35 → 36 (2026-11): CombatBattleType derivation, PvP tactical context (`CombatGameContext`,
///   `SelfDefending`/`SelfAttacking`/`SelfAtStation`), symmetric PvP defender perspective in
///   `engine.rs` & defender `OfficerStatConditionContext`, `defender_profile_id` roster isolation,
///   and unlocking `SelfDefending`-gated opponent weapon damage reduction (Carol, K'Bisch, TOS Chekov).
/// * 36 → 37 (2026-11): QA #413 quick wins — Holodeck `473361651` routes to NPC-hostile-only base
///   piercing stats (`armor_piercing`, `shield_piercing`, `accuracy`) instead of unconditional
///   damage-through `pierce`; FCM Data `1610205778` moves to its verified captain seat; unsupported
///   single-axis `ArmorPiercing` abilities (Azetbur, Honorguard Worf, M'Ral, Paul Stamets) stay
///   quarantined with diagnostics instead of falling through to damage-through `shield_pierce`.
/// * 37 → 38 (2026-11): Unified `operation 5` (`MultiplyBaseAdd`) crew-stat-scaled officer engine
///   (#423) — effective 3-axis crew A/D/H evaluation at combat start, `PRE_ALL` defenses & piercing
///   (Tom Paris, SNW, Ba'el, Gowron, Martok, The Doctor, Cap. Tilly, B'Elanna, Charvanek, Marcus,
///   Kang, Six of Eleven, etc.), direct flat `ShieldHPRepair` (Beverly Crusher, Spock, TOS Spock),
///   `POST_ALL` pre-crit `FlatWeaponDamagePreCrit` (Mirek, Wesley Crusher, Ro Mudd, Tyler, TOS
///   Kirk, etc.), plus final-subround reactive stack expiry parity. New canary `ag_op5_crew_scaled`
///   exercises the new engine effects while this bump invalidates catalog-resolved crews.
/// * 38 → 39 (2026-11): Batch B live-verified combat fixes (Fights A–F) — native Defensive Tactics
///   (`rid 1914017763`, `bid 2859405133`) CombatStart non-Armada hostile weapon-damage reduction
///   (`-5%..-30%`, additive with Chen/T'Laan), ship `officer_bonus.attack` additive in the
///   `weapon_damage` MULT pool (matching Realta vs Vi'Dar captures), unified `op 6`
///   (`MultiplyBaseSub`) `ShieldHPRepair` flat enemy shield drain (Ent-E Troi, Vemet, Six Of Eleven
///   cap, Michael Burnham), Suder (`2041170794`) round-boundary hull repair, Annorax (`2281926825`)
///   reactive `HitTaken` flat Apex Barrier (`num_rounds=2`), and single-axis `ArmorPiercing` pool
///   scaling (M'Ral launch, Azetbur on-hit, Paul Stamets PvP). New canary `ah_batch_b_mechanics`
///   covers the engine paths while this bump invalidates persisted profile/catalog observations.
/// * 39 → 40 (2026-11): Augment Exile [WOK] hostile tag (`faction.id 1306860549`), U.S.S. Reliant
///   ability gates (`Always Reliant`, `Reliant Barrier`, and `M/ARA Insulation`
///   `DilithiumDestabilizationImmunity`), Hijacked D7 / DY-200 `Not So Wounded` (+5,000 Apex
///   Barrier), and WOK Carol Marcus Exile loot routing. Canary `ai_reliant_dilithium_immunity`
///   verifies `DilithiumDestabilizationImmunity` suppresses `HostileLethalCombatBegin`.
/// * 40 → 41 (2026-11): U.S.S. Reliant live calibration fixes (R1/R2/L48/DY-200/R3) — canonical
///   `ShieldMitigation` officer rank scaling & `Sub` (additive, SNW Pike / WOK Scotty) vs
///   `MultiplySub` (multiplicative bypass, Harrison / SNW Kirk); per-weapon piercing in outbound
///   hostile mitigation (`base_attacker_weapon_stats`); Wave Defense / PvP / Excelsior / Origin
///   Sector / faction-specific Apex Shred research gating; verified native CombatStart/RoundStart
///   research & building buffs (4★ ship weaponry/piercing, hostile/energy/kinetic/advantage/fragile/FKR
///   weapon damage, outbound crit damage + Inspiring Leader, Augment Exile / Ressikin Flute / class /
///   Captain Proton's Blaster / Strand of Krenim Hair / DTI Temporal Device isolytic, Space Critical
///   Resistance / Guardian of Forever hostile CDR); and Explorer/Interceptor/Battleship Emergency
///   Protocol previous-round hull repair. Canary `aj_reliant_calibration_mechanics` covers per-weapon
///   outbound mitigation, additive `ShieldMitigationBonus`, and counter-fire additive hostile CDR.
/// * 41 → 42 (2026-11): Live-verified U.S.S. Reliant L50–L58 ladder & 63-crit hostile/player
///   critical-damage calibration — native `CritModifier` / `CriticalDamage` projected directly as
///   final multiplier without `+1.0` (`critical_damage_to_multiplier`), additive CDR subtracts down
///   to `0.0` before `crit_damage_floor` (`crit.rs`), RoundStart `ModShieldHpRepair` (`NS Morale
///   SHP`) overfills above `max_ship_shps` (`engine.rs`), and exact SHA-256-bound non-Armada
///   condition-57 (`Hostile Stakeout` `1450484905`, `Court of Q` `1768821190`), 2-round timed crit
///   seats (`Section 31 Crit Chance/Damage` `3342225309`/`2900801564`, `KSG Crit Chance Reduction`
///   `10304068`), and `Annorax's Datapad` (`1238401736`, non-Armada + Tal-not-on-bridge Apex
///   Barrier).
/// * 42 → 43 (2026-11): Preserve `spec.chance` when compiling `CombatBegin` `ShotsBonus` abilities
///   in `effect_spec_compile.rs` (fixes Harcourt Fenton Mudd `143246507` *"823 ways to die"* and
///   Seven Of Eleven `3774124358` previously compiling with hardcoded `chance = 1.0`), and align
///   `harry-mudd-374d5f` (`677303054`, *"Harcourt Fenton Mudd"*) / `mudd-32546a` (`634330627`,
///   *"Harry Mudd"*) display names with STFC `translations-officer_names.json`.
/// * 43 → 44 (2026-11): Route native STFC officer `ModAllDamage` `MultiplyAdd` abilities
///   (`SNW James Kirk`, `Mirror Ezri`/`Troi`, `PIC Picard`/`Riker`/`Worf`, `SNW D'Chok`, `Caleb Mir`,
///   `Harry Mudd`, `Beckett Mariner`, etc.) into the single additive `BaseWeaponDamageBonus` pool
///   (`effect_spec_compile.rs`) instead of multiplying on top of the completed profile pool, and
///   align `profile_weapon_damage_fraction` (`scenario.rs`) with `apply_profile_to_attacker` by
///   including owner-faction weapon damage and officer Attack breakpoint bonus.
/// * 44 → 45 (2026-11): Fix U.S.S. Dauntless `Active Sweep` (`1905773933`) from multiplicative
///   `attack_multiplier` to additive `component_base_weapon_damage` (verified to <0.6% across all 4
///   weapons against live L65 Imperial Starfleet Planet Harvester battle log), and model Mirror
///   Universe `Deadlock` (`849650945`, combat-start player Hull Breach) and `Dismantlement` +
///   `Photophobia` (`910140799`, round-start +20% counter weapon damage and +10% counter isolytic
///   damage gated on `AttackerHullBreach`) across 194 MU hostiles.
/// * 45 → 46 (2026-11): Route native STFC officer `CritDamage` `op=Add` abilities (`SNW Sam Kirk`,
///   `Ent-E Troi`, `Seven of Nine`, `Suder`, etc.) to additive `BaseCritDamageBonus` instead of
///   multiplicative `CritDamageMultiplier`; fix backwards `IsolyticDefenseBonus` application so
///   attacker isolytic defense protects against inbound counter-fire rather than defender outbound
///   shots (and vice versa); evaluate attacker counter-fire CDR and Apex Barrier on live `combat_ctx`
///   rather than status-zeroed `aux_ctx`; and add exact-bound native research seats for
///   `NS HB Crit Damage Reduction` (`981948123`), `Blade of Tkon` (`2917329560`),
///   `Vidiian Honatta Organ Harvester` (`3696141147`), Dauntless `Seek and Destroy Critical
///   Damage` (`3677857189`), and Dauntless launch HP gate `[83001]` (`Dauntless Hull/Shield
///   Upgrade` `1730696589` / `1535845511`). Verified against 11 live L61 Harvester fights + L65 control.
pub const COMBAT_ENGINE_BEHAVIOR_VERSION: u32 = 46;

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
    pub wave_defense: bool,
    pub game_context: Option<&'a str>,
    pub attacker_is_defending: bool,
    pub attacker_at_station: bool,
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
/// A canary only sees the paths it exercises. The fights below cover distinct mechanic
/// families (plain damage/mitigation/crit/pierce; shields + burning + hull regen + round-end damage;
/// accumulating multipliers + isolytic/apex). **When you add a new mechanic family to the engine, add
/// a fight here**, and bump [`COMBAT_ENGINE_BEHAVIOR_VERSION`] for anything the suite cannot see.
pub fn engine_canary_digest() -> u64 {
    stable_text_hash(&canary_fight_summaries().concat())
}

/// One `label:outcome` line per canary fight. Split out so a test can assert the suite is not
/// degenerate (all fights ending identically would make the digest blind).
fn canary_fight_summaries() -> Vec<String> {
    let mut summaries: Vec<String> = canary_fights()
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
        .collect();
    // This path requires defender seats, which the historical attacker-only
    // canary tuple does not carry. Exercise native signed piercing percentages
    // before the raw incoming curve, including an amplified enemy reduction.
    let mut attacker = canary_combatant("canary_o_player");
    attacker.attack = 43.0;
    attacker.hull_health = 9_000.0;
    attacker.hostile_mitigation_params = Some(crate::combat::HostileMitigationParams {
        round_native_defense_stats: false,
        defender_stats: crate::combat::DefenderStats {
            armor: 240.0,
            shield_deflection: 320.0,
            dodge: 180.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: crate::combat::AttackerStats {
            armor_piercing: 40.0,
            shield_piercing: 60.0,
            accuracy: 100.0,
        },
        profile_piercing_fractions: None,
        base_attacker_weapon_stats: Vec::new(),
        ship_type: crate::combat::ShipType::Explorer,
        mystery_mitigation_factor: 0.0,
        floor: 0.0,
        ceiling: 1.0,
    });
    let mut defender = canary_combatant("canary_o_hostile");
    defender.attack = 193.0;
    defender.hull_health = 4_000.0;
    let crew = CrewConfiguration {
        seats: vec![canary_seat(
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::HostileBasePiercingStatsReduction(2.1),
        )],
    };
    let defender_crew = CrewConfiguration {
        seats: vec![canary_seat(
            CrewSeat::Ship,
            AbilityClass::ShipAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::HostileCounterPierceMultiplier { bonus: 5.0 },
        )],
    };
    let result = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &canary_config(5, 61),
        &crew,
        crate::combat::OpponentFactionTag::Unknown,
        crate::combat::ShipType::Battleship,
        crate::combat::ShipType::Explorer,
        true,
        false,
        &defender_crew,
    );
    summaries.push(format!("o_raw_counter_piercing:dmg={:.6};won={};limit={};rounds={};ahull={:.6};dhull={:.6};dshield={:.6};ashield={:.6};iso={:.6};",
        result.total_damage,result.attacker_won,result.winner_by_round_limit,result.rounds_simulated,
        result.attacker_hull_remaining,result.defender_hull_remaining,result.defender_shield_remaining,
        result.attacker_shield_remaining,result.total_isolytic_damage));
    // Defender-owned RoundStart subtraction composes with a timed own bonus
    // before the final clamp. Synthetic .6 strengths expose an invented source
    // cap even in a short fight; attacker-owned subtraction checks direction.
    let mut attacker = canary_combatant("canary_p_player");
    attacker.attack = 83.0;
    attacker.hull_health = 100_000.0;
    attacker.shield_health = 100_000.0;
    attacker.shield_mitigation = 0.99;
    let mut defender = canary_combatant("canary_p_hostile");
    defender.attack = 211.0;
    defender.hull_health = 100_000.0;
    defender.shield_health = 100_000.0;
    let crew = CrewConfiguration {
        seats: vec![canary_seat(
            CrewSeat::Ship,
            AbilityClass::ShipAbility,
            TimingWindow::RoundStart,
            AbilityEffect::CumulativeOpponentShieldMitigationDebuff {
                per_round: 0.17,
                cap: 1.0,
            },
        )],
    };
    let defender_crew = CrewConfiguration {
        seats: vec![canary_seat(
            CrewSeat::Ship,
            AbilityClass::ShipAbility,
            TimingWindow::RoundStart,
            AbilityEffect::CumulativeOpponentShieldMitigationDebuff {
                per_round: 0.6,
                cap: f64::MAX,
            },
        )],
    };
    let config = SimulationConfig {
        incoming_shield_mitigation_bonus: 0.6,
        incoming_shield_mitigation_bonus_rounds: 2,
        ..canary_config(3, 67)
    };
    let result = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &config,
        &crew,
        crate::combat::OpponentFactionTag::Unknown,
        crate::combat::ShipType::Explorer,
        crate::combat::ShipType::Battleship,
        true,
        false,
        &defender_crew,
    );
    summaries.push(format!("p_hostile_round_start_shield_subtraction:dmg={:.6};won={};limit={};rounds={};ahull={:.6};dhull={:.6};dshield={:.6};ashield={:.6};iso={:.6};",
        result.total_damage,result.attacker_won,result.winner_by_round_limit,result.rounds_simulated,
        result.attacker_hull_remaining,result.defender_hull_remaining,result.defender_shield_remaining,
        result.attacker_shield_remaining,result.total_isolytic_damage));
    // Fire Isolytic Damage in both directions to detect which hull the Armada's
    // combat-start Isolytic Defense protects.
    let mut attacker = canary_combatant("canary_s_attacker");
    attacker.attack = 510.0;
    attacker.isolytic_damage = 1.0;
    let mut defender = canary_combatant("canary_s_armada");
    defender.attack = 200.0;
    let defender_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::IsolyticDamageBonus(1.5),
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::IsolyticDefenseBonus(0.5),
            ),
        ],
    };
    let result = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &canary_config(2, 71),
        &CrewConfiguration::default(),
        crate::combat::OpponentFactionTag::Romulan,
        crate::combat::ShipType::Armada,
        crate::combat::ShipType::Battleship,
        true,
        false,
        &defender_crew,
    );
    summaries.push(format!("s_hostile_combat_begin_isolytic_defense:dmg={:.6};won={};limit={};rounds={};ahull={:.6};dhull={:.6};dshield={:.6};ashield={:.6};iso={:.6};",
        result.total_damage,result.attacker_won,result.winner_by_round_limit,result.rounds_simulated,
        result.attacker_hull_remaining,result.defender_hull_remaining,result.defender_shield_remaining,
        result.attacker_shield_remaining,result.total_isolytic_damage));

    // The hostile engagement limit stops a potential win. Cascade Nullification
    // also exercises separate outbound Isolytic Defense and counter critical damage.
    let mut attacker = canary_combatant("canary_t_player");
    attacker.attack = 230.0;
    attacker.isolytic_damage = 0.25;
    let mut defender = canary_combatant("canary_t_hostile");
    defender.attack = 10.0;
    defender.crit_chance = 1.0;
    let defender_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileEngagementRoundLimit { rounds: 3 },
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::IsolyticDefenseBonus(2.5),
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileCritDamageFloorBonus(3.0),
            ),
        ],
    };
    defender.crit_damage_floor =
        crate::combat::hostile_crit_damage_floor_bonus_from_defender_crew(&defender_crew);
    let result = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &canary_config(8, 73),
        &CrewConfiguration { seats: vec![] },
        crate::combat::OpponentFactionTag::Unknown,
        crate::combat::ShipType::Armada,
        crate::combat::ShipType::Battleship,
        true,
        false,
        &defender_crew,
    );
    summaries.push(format!(
        "t_hostile_engagement_limit:dmg={:.6};won={};limit={};rounds={};dhull={:.6};ahull={:.6};iso={:.6};",
        result.total_damage,
        result.attacker_won,
        result.winner_by_round_limit,
        result.rounds_simulated,
        result.defender_hull_remaining,
        result.attacker_hull_remaining,
        result.total_isolytic_damage,
    ));

    // The stronger decay ends after round one. The weaker seat still applies in round two.
    let mut attacker = canary_combatant("canary_y_player");
    attacker.hull_health = 10_000.0;
    attacker.attack = 100.0;
    let defender = canary_combatant("canary_y_hostile");
    let sustained = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::RoundStart,
        AbilityEffect::HostileHyperthermicDecay { fraction: 0.10 },
    );
    let mut timed = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::RoundStart,
        AbilityEffect::HostileHyperthermicDecay { fraction: 0.25 },
    );
    timed.ability.condition = Some(crate::combat::AbilityCondition::RoundRange { min: 1, max: 1 });
    let result = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &attacker,
        &defender,
        &canary_config(2, 71),
        &CrewConfiguration::default(),
        crate::combat::OpponentFactionTag::Unknown,
        crate::combat::ShipType::Explorer,
        crate::combat::ShipType::Battleship,
        true,
        false,
        &CrewConfiguration {
            seats: vec![sustained, timed],
        },
    );
    summaries.push(format!(
        "y_hostile_hyperthermic_decay:dmg={:.6};rounds={};ahull={:.6};",
        result.total_damage, result.rounds_simulated, result.attacker_hull_remaining,
    ));

    // Ship passives such as Secret Weapon use the opponent category at combat begin.
    let mut attacker = canary_combatant("canary_z_attacker");
    attacker.attack = 731.0;
    let mut defender = canary_combatant("canary_z_defender");
    defender.hull_health = 10_000.0;
    let mut passive = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::AttackMultiplier(0.35),
    );
    passive.ability.condition = Some(AbilityCondition::DefenderIsNpcHostile);
    let crew = CrewConfiguration {
        seats: vec![passive],
    };
    for (label, npc) in [("z_hostile_damage", true), ("z_player_control", false)] {
        let result = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
            &attacker,
            &defender,
            &canary_config(1, 71),
            &crew,
            crate::combat::OpponentFactionTag::Unknown,
            crate::combat::ShipType::Battleship,
            crate::combat::ShipType::Explorer,
            npc,
            !npc,
            &CrewConfiguration::default(),
        );
        summaries.push(format!(
            "{label}:dmg={:.6};rounds={};dhull={:.6};",
            result.total_damage, result.rounds_simulated, result.defender_hull_remaining,
        ));
    }

    // Player-only defensive passives (e.g. Grade 7 Apex Aegis) apply only in PvP ship combat.
    let mut pvp_attacker = canary_combatant("canary_aa_attacker");
    pvp_attacker.attack = 1_100.0;
    pvp_attacker.hull_health = 10_000.0;
    let mut pvp_defender = canary_combatant("canary_aa_defender");
    pvp_defender.attack = 800.0;
    pvp_defender.hull_health = 10_000.0;
    let mut pvp_barrier = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::ApexBarrierBonus(10_000.0),
    );
    pvp_barrier.ability.condition = Some(AbilityCondition::DefenderIsPlayerShip);
    let pvp_crew = CrewConfiguration {
        seats: vec![pvp_barrier],
    };
    for (label, npc) in [
        ("aa_pvp_barrier_active", false),
        ("aa_pvp_barrier_inert_pve", true),
    ] {
        let result = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
            &pvp_attacker,
            &pvp_defender,
            &canary_config(1, 73),
            &pvp_crew,
            crate::combat::OpponentFactionTag::Unknown,
            crate::combat::ShipType::Battleship,
            crate::combat::ShipType::Interceptor,
            npc,
            !npc,
            &CrewConfiguration::default(),
        );
        summaries.push(format!(
            "{label}:dmg={:.6};rounds={};ahull={:.6};",
            result.total_damage, result.rounds_simulated, result.attacker_hull_remaining,
        ));
    }

    // Update 96 canaries: Critical Mitigation rating, Infusion Capacitors + Spike vs Inertial
    // Diverters, and Polaron Weapon bypass + Polaron Mitigation + Polaron Weaponry bonuses.
    let mut cm_att = canary_combatant("canary_ab_attacker");
    cm_att.attack = 100.0;
    cm_att.hull_health = 10_000.0;
    let mut cm_def = canary_combatant("canary_ab_defender");
    cm_def.attack = 1_000.0;
    cm_def.crit_chance = 1.0;
    cm_def.crit_multiplier = 2.0;
    cm_def.hull_health = 10_000.0;
    let mut cm_seat = canary_seat(
        CrewSeat::BelowDeck,
        AbilityClass::BelowDeck,
        TimingWindow::CombatBegin,
        AbilityEffect::CritMitigationRating(50_000.0),
    );
    cm_seat.ability.condition = Some(AbilityCondition::DefenderIsPlayerShip);
    let cm_res = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &cm_att,
        &cm_def,
        &canary_config(1, 79),
        &CrewConfiguration {
            seats: vec![cm_seat],
        },
        crate::combat::OpponentFactionTag::Unknown,
        crate::combat::ShipType::Explorer,
        crate::combat::ShipType::Battleship,
        false,
        true,
        &CrewConfiguration::default(),
    );
    summaries.push(format!(
        "ab_crit_mitigation_rating:ahull={:.6};",
        cm_res.attacker_hull_remaining
    ));

    let mut inf_att = canary_combatant("canary_ac_attacker");
    inf_att.attack = 1_000.0;
    inf_att.hull_health = 10_000.0;
    let mut inf_def = canary_combatant("canary_ac_defender");
    inf_def.hull_health = 10_000.0;
    inf_def.shield_health = 5_000.0;
    inf_def.shield_mitigation = 1.0;
    inf_def.mitigation = 0.7;
    let inf_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::InfusionCapacitors(50_000.0),
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::InfusionCapacitorSpike(1.0),
            ),
        ],
    };
    let inf_def_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileNegatesStandardDamage,
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileShieldRestoreOnDeplete,
            ),
        ],
    };
    let inf_res = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &inf_att,
        &inf_def,
        &canary_config(1, 83),
        &inf_crew,
        crate::combat::OpponentFactionTag::Federation,
        crate::combat::ShipType::Explorer,
        crate::combat::ShipType::Battleship,
        true,
        false,
        &inf_def_crew,
    );
    summaries.push(format!(
        "ac_infusion_inertial_diverters:dhull={:.6};dshield={:.6};",
        inf_res.defender_hull_remaining, inf_res.defender_shield_remaining
    ));

    let mut pol_att = canary_combatant("canary_ad_attacker");
    pol_att.attack = 100.0;
    pol_att.hull_health = 10_000.0;
    pol_att.shield_health = 5_000.0;
    pol_att.shield_mitigation = 0.8;
    pol_att.mitigation = 0.7;
    pol_att.apex_barrier = 100_000.0;
    let mut pol_def = canary_combatant("canary_ad_defender");
    pol_def.hull_health = 10_000.0;
    pol_def.weapons = vec![crate::combat::WeaponStats {
        attack: 1_000.0,
        shots: Some(1),
        weapon_type: crate::combat::WeaponType::Polaron,
        ..Default::default()
    }];
    let pol_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::PolaronMitigation(50_000.0),
            ),
            canary_seat(
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                TimingWindow::CombatBegin,
                AbilityEffect::PolaronMitigationMultiplier(0.2),
            ),
        ],
    };
    let pol_def_crew = CrewConfiguration {
        seats: vec![canary_seat(
            CrewSeat::Ship,
            AbilityClass::ShipAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::HostilePolaronDamageBonus {
                same_faction_bonus: 0.25,
                non_counter_bonus: 0.50,
            },
        )],
    };
    let mut pol_cfg = canary_config(1, 89);
    pol_cfg.attacker_owner_faction = crate::combat::OpponentFactionTag::Federation;
    let pol_res = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &pol_att,
        &pol_def,
        &pol_cfg,
        &pol_crew,
        crate::combat::OpponentFactionTag::Federation,
        crate::combat::ShipType::Explorer,
        crate::combat::ShipType::Explorer,
        true,
        false,
        &pol_def_crew,
    );
    summaries.push(format!(
        "ad_polaron_weapon_mitigation:ahull={:.6};ashield={:.6};",
        pol_res.attacker_hull_remaining, pol_res.attacker_shield_remaining
    ));

    // Additive engagement tags (e.g. Wave Defense `[RedMovingSpace, Waves]`): both a
    // Wave-gated ability and a non-Armada hostile ability fire together on the same fight.
    let mut wave_att = canary_combatant("canary_ae_attacker");
    wave_att.attack = 420.0;
    wave_att.hull_health = 10_000.0;
    let mut wave_def = canary_combatant("canary_ae_defender");
    wave_def.attack = 150.0;
    wave_def.hull_health = 10_000.0;
    let mut wave_seat = canary_seat(
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::AttackMultiplier(0.40),
    );
    wave_seat.ability.condition = Some(AbilityCondition::EngagementIncludes(
        crate::combat::EnemyType::Waves,
    ));
    let mut hostile_seat = canary_seat(
        CrewSeat::Captain,
        AbilityClass::CaptainManeuver,
        TimingWindow::CombatBegin,
        AbilityEffect::AttackMultiplier(0.25),
    );
    hostile_seat.ability.condition = Some(AbilityCondition::And(vec![
        AbilityCondition::DefenderIsNpcHostile,
        AbilityCondition::Not(Box::new(AbilityCondition::DefenderShipTypeIs(
            crate::combat::ShipType::Armada,
        ))),
    ]));
    let mut wave_cfg = canary_config(2, 97);
    wave_cfg.engagement_enemy_types = crate::combat::EnemyTypes(vec![
        crate::combat::EnemyType::RedMovingSpace,
        crate::combat::EnemyType::Waves,
    ]);
    let wave_res = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &wave_att,
        &wave_def,
        &wave_cfg,
        &CrewConfiguration {
            seats: vec![hostile_seat, wave_seat],
        },
        crate::combat::OpponentFactionTag::Unknown,
        crate::combat::ShipType::Explorer,
        crate::combat::ShipType::Battleship,
        true,
        false,
        &CrewConfiguration::default(),
    );
    summaries.push(format!(
        "ae_engagement_wave_defense:dmg={:.6};dhull={:.6};rounds={};",
        wave_res.total_damage, wave_res.defender_hull_remaining, wave_res.rounds_simulated
    ));

    // CombatBattleType + CombatGameContext + symmetric PvP defender perspective:
    // attacker evaluates AttackerIsAttacking + CaptureNode + battle_type 4, while
    // player defender evaluates swapped AttackerIsDefending + CaptureNode + battle_type 4.
    let mut pvp_att = canary_combatant("canary_af_attacker");
    pvp_att.attack = 500.0;
    pvp_att.hull_health = 10_000.0;
    let mut pvp_def = canary_combatant("canary_af_defender");
    pvp_def.attack = 300.0;
    pvp_def.hull_health = 10_000.0;
    let mut att_node_seat = canary_seat(
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::AttackMultiplier(0.50),
    );
    att_node_seat.ability.condition = Some(AbilityCondition::And(vec![
        AbilityCondition::DefenderIsPlayerShip,
        AbilityCondition::AttackerIsAttacking,
        AbilityCondition::CombatGameContextIs(crate::combat::CombatGameContext::CaptureNode),
        AbilityCondition::CombatBattleTypeAny(vec![4]),
    ]));
    let mut def_node_seat = canary_seat(
        CrewSeat::Captain,
        AbilityClass::CaptainManeuver,
        TimingWindow::CombatBegin,
        AbilityEffect::OpponentWeaponDamageReduction {
            reduction: 0.25,
            cumulative: false,
        },
    );
    def_node_seat.ability.condition = Some(AbilityCondition::And(vec![
        AbilityCondition::DefenderIsPlayerShip,
        AbilityCondition::AttackerIsDefending,
        AbilityCondition::CombatGameContextIs(crate::combat::CombatGameContext::CaptureNode),
        AbilityCondition::CombatBattleTypeAny(vec![4]),
    ]));
    let mut pvp_cfg = canary_config(2, 101);
    pvp_cfg.engagement_enemy_types =
        crate::combat::EnemyTypes(vec![crate::combat::EnemyType::PvpSpace]);
    pvp_cfg.combat_battle_type_id = Some(4);
    pvp_cfg.combat_game_context = crate::combat::CombatGameContext::CaptureNode;
    pvp_cfg.attacker_is_defending = false;
    let pvp_res = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &pvp_att,
        &pvp_def,
        &pvp_cfg,
        &CrewConfiguration {
            seats: vec![att_node_seat],
        },
        crate::combat::OpponentFactionTag::Federation,
        crate::combat::ShipType::Explorer,
        crate::combat::ShipType::Interceptor,
        false,
        true,
        &CrewConfiguration {
            seats: vec![def_node_seat],
        },
    );
    summaries.push(format!(
        "af_battle_type_and_pvp_defender_perspective:dmg={:.6};ahull={:.6};dhull={:.6};rounds={};",
        pvp_res.total_damage,
        pvp_res.attacker_hull_remaining,
        pvp_res.defender_hull_remaining,
        pvp_res.rounds_simulated
    ));

    // U.S.S. Reliant M/ARA Insulation (`DilithiumDestabilizationImmunity`) vs Augment Exile
    // `HostileLethalCombatBegin` (100% lethal chance): must survive all rounds instead of dying at round 0.
    let mut reliant_att = canary_combatant("canary_ai_reliant");
    reliant_att.attack = 400.0;
    reliant_att.hull_health = 10_000.0;
    let mut exile_def = canary_combatant("canary_ai_exile");
    exile_def.attack = 100.0;
    exile_def.hull_health = 10_000.0;
    let mut immune_seat = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::DilithiumDestabilizationImmunity,
    );
    immune_seat.ability.condition = Some(AbilityCondition::And(vec![
        AbilityCondition::DefenderIsNpcHostile,
        AbilityCondition::DefenderHostileTagsAllPresent {
            required_mask: crate::combat::hostile_tags::HOSTILE_TAG_MASK_AUGMENT_EXILE,
        },
    ]));
    let lethal_seat = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::HostileLethalCombatBegin { chance: 1.0 },
    );
    let mut reliant_cfg = canary_config(2, 103);
    reliant_cfg.defender_hostile_tag_mask =
        crate::combat::hostile_tags::HOSTILE_TAG_MASK_AUGMENT_EXILE;
    let reliant_res = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &reliant_att,
        &exile_def,
        &reliant_cfg,
        &CrewConfiguration {
            seats: vec![immune_seat],
        },
        crate::combat::OpponentFactionTag::Unknown,
        crate::combat::ShipType::Interceptor,
        crate::combat::ShipType::Explorer,
        true,
        false,
        &CrewConfiguration {
            seats: vec![lethal_seat],
        },
    );
    summaries.push(format!(
        "ai_reliant_dilithium_immunity:dmg={:.6};ahull={:.6};rounds={};",
        reliant_res.total_damage, reliant_res.attacker_hull_remaining, reliant_res.rounds_simulated
    ));

    // Per-weapon outbound mitigation (`base_attacker_weapon_stats`), additive opponent
    // `ShieldMitigationBonus(-0.25)`, and counter-fire additive + multiplicative hostile CDR.
    let mut cal_att = canary_combatant("canary_aj_att");
    cal_att.hull_health = 50_000.0;
    cal_att.shield_health = 50_000.0;
    cal_att.weapons = vec![
        crate::combat::WeaponStats {
            attack: 1_000.0,
            shots: Some(1),
            crit_chance: Some(0.0),
            crit_multiplier: Some(1.0),
            weapon_type: crate::combat::WeaponType::Energy,
            ..Default::default()
        },
        crate::combat::WeaponStats {
            attack: 1_000.0,
            shots: Some(1),
            crit_chance: Some(0.0),
            crit_multiplier: Some(1.0),
            weapon_type: crate::combat::WeaponType::Kinetic,
            ..Default::default()
        },
    ];
    let mut cal_def = canary_combatant("canary_aj_def");
    cal_def.attack = 1_000.0;
    cal_def.crit_chance = 1.0;
    cal_def.crit_multiplier = 2.5;
    cal_def.hull_health = 50_000.0;
    cal_def.shield_health = 50_000.0;
    cal_def.shield_mitigation = 0.80;
    cal_def.hostile_mitigation_params = Some(crate::combat::HostileMitigationParams {
        round_native_defense_stats: false,
        defender_stats: crate::combat::DefenderStats {
            armor: 10_000.0,
            shield_deflection: 10_000.0,
            dodge: 10_000.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: crate::combat::AttackerStats {
            armor_piercing: 50_000.0,
            shield_piercing: 50_000.0,
            accuracy: 50_000.0,
        },
        profile_piercing_fractions: None,
        base_attacker_weapon_stats: vec![
            crate::combat::AttackerStats {
                armor_piercing: 2_000.0,
                shield_piercing: 2_000.0,
                accuracy: 2_000.0,
            },
            crate::combat::AttackerStats {
                armor_piercing: 20_000.0,
                shield_piercing: 20_000.0,
                accuracy: 20_000.0,
            },
        ],
        ship_type: crate::combat::ShipType::Interceptor,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.712,
    });
    let cal_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::ShieldMitigationBonus(-0.25),
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileCritDamageReduction {
                    reduction: 0.60,
                    duration_rounds: crate::combat::types::MAX_COMBAT_ROUNDS,
                    additive_percentage_points: true,
                    stacks: false,
                },
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileCritDamageReduction {
                    reduction: 0.05,
                    duration_rounds: crate::combat::types::MAX_COMBAT_ROUNDS,
                    additive_percentage_points: false,
                    stacks: false,
                },
            ),
        ],
    };
    let cal_cfg = canary_config(2, 104);
    let cal_res = crate::combat::simulate_combat_with_defender_faction_and_defender_crew(
        &cal_att,
        &cal_def,
        &cal_cfg,
        &cal_crew,
        crate::combat::OpponentFactionTag::Unknown,
        crate::combat::ShipType::Interceptor,
        crate::combat::ShipType::Explorer,
        true,
        false,
        &CrewConfiguration::default(),
    );
    summaries.push(format!(
        "aj_reliant_calibration_mechanics:dmg={:.6};dhull={:.6};dshd={:.6};ahull={:.6};",
        cal_res.total_damage,
        cal_res.defender_hull_remaining,
        cal_res.defender_shield_remaining,
        cal_res.attacker_hull_remaining,
    ));
    summaries
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

    // (d) one-round conditional raw defense and morale-gated shield repair.
    let mut d_attacker = canary_combatant("canary_d_attacker");
    d_attacker.attack = 75.0;
    // A fractional point result makes the native integer boundary observable.
    d_attacker.armor = 0.013;
    d_attacker.hull_health = 1_200.0;
    d_attacker.shield_health = 400.0;
    d_attacker.hostile_mitigation_params = Some(crate::combat::HostileMitigationParams {
        round_native_defense_stats: true,
        defender_stats: crate::combat::DefenderStats {
            armor: 20.0,
            shield_deflection: 40.0,
            dodge: 70.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: crate::combat::AttackerStats {
            armor_piercing: 100.0,
            shield_piercing: 160.0,
            accuracy: 220.0,
        },
        profile_piercing_fractions: None,
        base_attacker_weapon_stats: Vec::new(),
        ship_type: crate::combat::ShipType::Battleship,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.72,
    });
    let mut d_defender = canary_combatant("canary_d_defender");
    d_defender.attack = 250.0;
    d_defender.hull_health = 3_000.0;
    let mut defense = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::RoundStart,
        AbilityEffect::BaseMitigationStatsBonus(0.5),
    );
    defense.ability.condition = Some(crate::combat::AbilityCondition::MoraleActive);
    let mut repair = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::RoundStart,
        AbilityEffect::ShieldRegenMaxFraction(0.1),
    );
    repair.ability.condition = Some(crate::combat::AbilityCondition::MoraleActive);
    let d_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::BaseMitigationStatsBonus(0.75),
            ),
            canary_seat(
                CrewSeat::BelowDeck,
                AbilityClass::BelowDeck,
                TimingWindow::CombatBegin,
                AbilityEffect::BaseMitigationStatsAdd(40.0),
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::RoundStart,
                AbilityEffect::Morale(1.0),
            ),
            defense,
            repair,
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::RoundStart,
                AbilityEffect::BaseWeaponDamageBonus(0.25),
            ),
        ],
    };

    let mut e_attacker = canary_combatant("canary_e_attacker");
    e_attacker.weapons = vec![
        crate::combat::WeaponStats {
            attack: 100.0,
            ..Default::default()
        };
        2
    ];
    let mut e_defender = canary_combatant("canary_e_defender");
    e_defender.hull_health = 20_000.0;
    let e_crew = CrewConfiguration {
        seats: vec![canary_seat(
            CrewSeat::Captain,
            AbilityClass::BridgeAbility,
            TimingWindow::AttackPhase,
            AbilityEffect::OnHitTimedBonus {
                crit_chance: 0.04,
                cascade_damage: 0.1,
                duration_rounds: 3,
            },
        )],
    };

    // (f) each native source group and seat scope is needed to cross a breakpoint.
    let mut f_profile = crate::data::profile::PlayerProfile::default();
    let f_buffs = [
        (476178618, 3),   // global Defense +15%
        (334143899, 2),   // building all officer stats +2%
        (1474824297, 2),  // Armory hull health +10%
        (1030529782, 2),  // Shuttle Bay weapon damage +2%
        (941300209, 5),   // captain all stats +35%
        (809236561, 5),   // bridge (including captain) all stats +35%
        (1895868262, 10), // below-decks Defense +100%
    ]
    .map(|(bid, level)| GlobalBuffEntry {
        bid,
        level,
        expiry_time: None,
    });
    f_profile.bonuses.extend(
        crate::data::native_consumable_buffs::bonuses_for_imported_buffs(&f_buffs, None, 0),
    );
    let building_bonuses = crate::data::native_building_buffs::building_bonuses_for_imported_buffs(
        &f_buffs, None, None, 0,
    );
    crate::data::profile::merge_building_bonuses_from_sources(
        &mut f_profile,
        &[],
        &std::collections::HashMap::new(),
        None,
        Path::new("data/buildings"),
        &crate::data::building::BuildingBonusContext::default(),
        building_bonuses.as_ref(),
    );
    // Catalog groups are disjoint; officer pools add across the three sources.
    for (key, value) in
        crate::data::native_research_buffs::research_bonuses_for_imported_buffs(&f_buffs, None, 0)
            .unwrap_or_default()
    {
        *f_profile.bonuses.entry(key).or_default() += value;
    }
    let f_rows = [
        crate::lcars::resolver::OfficerStatSeat::Captain,
        crate::lcars::resolver::OfficerStatSeat::BelowDecks,
    ]
    .into_iter()
    .enumerate()
    .map(|(i, seat)| crate::lcars::resolver::CrewedOfficerStats {
        officer_id: format!("canary_f_{i}"),
        rank: Some(1),
        level: 1,
        seat,
        raw: crate::combat::CrewOfficerStatTotals {
            attack: 50.0,
            defense: 50.0,
            ..Default::default()
        },
    })
    .collect::<Vec<_>>();
    let f_base_attack_rating = crate::data::profile::evaluate_officer_stats(
        &f_rows,
        &f_profile,
        None,
        &Default::default(),
    )
    .effective_totals
    .attack;
    let f_ship = crate::data::ship::ShipRecord {
        ship_class: "battleship".into(),
        armor: 20.0,
        officer_bonus: crate::data::ship::OfficerBonusTable {
            // +30% to both seats crosses this threshold. Captain only does not.
            attack: vec![crate::data::ship::OfficerBonusBreakpoint {
                value: f_base_attack_rating + 22.5,
                bonus: 0.25,
            }],
            defense: vec![crate::data::ship::OfficerBonusBreakpoint {
                value: 200.5,
                bonus: 0.5,
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let f_runtime = crate::data::profile::compute_officer_stat_runtime_bonus_with_officers(
        crate::combat::CrewOfficerStatTotals {
            attack: 100.0,
            defense: 100.0,
            ..Default::default()
        },
        crate::combat::CrewOfficerStatTotals {
            attack: 50.0,
            defense: 50.0,
            ..Default::default()
        },
        &f_rows,
        &f_ship,
        &f_profile,
        None,
        &[("officer_attack".into(), 0.30)].into(),
        &[],
        &crate::data::profile::OfficerStatConditionContext::default(),
        &[],
    );
    let f_attacker = crate::data::profile::apply_profile_to_attacker(
        d_attacker.clone(),
        &f_profile,
        None,
        f_runtime,
    );
    let f_defender = d_defender.clone();

    let mut g_attacker = canary_combatant("canary_g_attacker");
    g_attacker.attack = 125.0;
    g_attacker.crit_chance = 1.0;
    g_attacker.crit_multiplier = 2.5;
    let g_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::BaseWeaponDamageBonus(0.4),
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::BaseCritDamageBonus(0.1),
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::CritDamageMultiplier(1.5),
            ),
        ],
    };

    // (h) conditional launch-time maximum hull with pre-existing damage.
    let mut h_attacker = canary_combatant("canary_h_attacker");
    h_attacker.attack = 25.0;
    let mut h_defender = canary_combatant("canary_h_defender");
    h_defender.hull_health = 5_000.0;
    h_defender.attack = 25.0;
    let mut h_ability = canary_seat(
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::HullHpMultiplier(0.5),
    );
    h_ability.ability.condition = Some(crate::combat::AbilityCondition::AttackerShipTypeIs(
        crate::combat::ShipType::Battleship,
    ));
    let mut h_config = canary_config(3, 31);
    h_config.initial_attacker_hull_damage = 1_100.0;

    // (i) an enemy critical-chance reduction applies to incoming fire.
    let mut i_attacker = canary_combatant("canary_i_attacker");
    i_attacker.attack = 75.0;
    i_attacker.hull_health = 2_000.0;
    let mut i_defender = canary_combatant("canary_i_defender");
    i_defender.attack = 150.0;
    i_defender.crit_chance = 1.0;
    i_defender.crit_multiplier = 3.0;
    i_defender.hull_health = 3_000.0;
    let i_crew = CrewConfiguration {
        seats: vec![canary_seat(
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            TimingWindow::CombatBegin,
            AbilityEffect::HostileCritChanceReduction(1.0),
        )],
    };

    // (j) Aquatic base-component damage, flat shot deflection, weapon schedules and reactive crit.
    let mut j_attacker = canary_combatant("canary_j_attacker");
    j_attacker.weapons = vec![crate::combat::WeaponStats {
        attack: 250.0,
        component_base_attack: Some(40.0),
        firing_schedule: Some(crate::combat::WeaponFiringSchedule {
            first_round: 2,
            interval_rounds: 2,
        }),
        crit_chance: Some(1.0),
        ..Default::default()
    }];
    let mut j_defender = canary_combatant("canary_j_defender");
    j_defender.hull_health = 8_000.0;
    j_defender.weapons = vec![crate::combat::WeaponStats {
        attack: 12.0,
        shots: Some(10),
        firing_schedule: Some(crate::combat::WeaponFiringSchedule {
            first_round: 1,
            interval_rounds: 100,
        }),
        ..Default::default()
    }];
    let j_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::ComponentBaseWeaponDamageBonus(3.0),
            ),
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::IncomingShotDeflection(9),
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::DefensePhase,
                AbilityEffect::DamageTakenCritStack {
                    amount: 0.3,
                    duration_rounds: 2,
                    reduce_opponent: false,
                },
            ),
        ],
    };

    // (k) piercing-derived setup must not add a second quarter of the damage
    // remaining after mitigation, in either direction. Include the setup helper
    // so restoring the old formula changes the persisted engine fingerprint.
    let defense = crate::combat::DefenderStats {
        armor: 110.0,
        shield_deflection: 80.0,
        dodge: 60.0,
    };
    let piercing = crate::combat::AttackerStats {
        armor_piercing: 100.0,
        shield_piercing: 100.0,
        accuracy: 100.0,
    };
    let ship_type = crate::combat::ShipType::Explorer;
    let mut k_attacker = canary_combatant("canary_k_attacker");
    k_attacker.attack = 500.0;
    k_attacker.hull_health = 10_000.0;
    k_attacker.mitigation = crate::combat::mitigation(defense, piercing, ship_type);
    k_attacker.pierce = crate::combat::pierce_damage_through_bonus(defense, piercing, ship_type);
    let mut k_defender = k_attacker.clone();
    k_defender.id = "canary_k_defender".to_string();
    k_defender.attack = 250.0;

    // Compile actual account sources into raw piercing, including the existing
    // pool. A missing source or multiplying the finished stats changes outcomes.
    let account_piercing = |npc_archive: bool| {
        let mut profile = crate::data::profile::PlayerProfile::default();
        profile.bonuses.insert("accuracy".into(), 0.3);
        profile.bonuses.insert("armor_piercing".into(), 0.6);
        profile.bonuses.insert("shield_piercing".into(), 0.9);
        let rows: Vec<_> = if npc_archive {
            vec![(1_404_701_608, 61)]
        } else {
            vec![
                (1_333_781_883, 61),
                (3_809_461_333, 61),
                (1_085_339_663, 61),
                (675_636_702, 61),
                (609_129_565, 61),
                (2_259_348_690, 61),
            ]
        }
        .into_iter()
        .map(|(bid, level)| GlobalBuffEntry {
            bid,
            level,
            expiry_time: None,
        })
        .collect();
        for pool in [
            crate::data::native_consumable_buffs::bonuses_for_imported_buffs(&rows, None, 0),
            crate::data::native_consumable_buffs::combat_start_piercing_bonuses_for_imported_buffs(
                &rows,
                npc_archive,
                0,
            ),
        ] {
            for (key, value) in pool {
                *profile.bonuses.entry(key).or_default() += value;
            }
        }
        let mut stats = crate::combat::AttackerStats {
            accuracy: 125.0,
            armor_piercing: 90.0,
            shield_piercing: 70.0,
        };
        crate::data::profile::apply_profile_piercing_to_attacker_stats(&mut stats, &profile, None);
        let mut attacker = canary_combatant(if npc_archive { "canary_m" } else { "canary_l" });
        attacker.attack = if npc_archive { 173.0 } else { 219.0 };
        attacker.hull_health = 12_000.0;
        let mut defender = canary_combatant("account_piercing_target");
        defender.hull_health = 10_000.0;
        defender.attack = 61.0;
        defender.mitigation = crate::combat::mitigation(defense, stats, ship_type);
        (attacker, defender)
    };
    let (l_attacker, l_defender) = account_piercing(false);
    let (m_attacker, m_defender) = account_piercing(true);
    let monaveen_rows = [
        2199227326, 2280947091, 3378417511, 3663693120, 1721910014, 1763915157, 3558267162,
        487851441, 2328144758, 610243845, 1240186710, 2956393975,
    ]
    .map(|bid| GlobalBuffEntry {
        bid,
        level: 61,
        expiry_time: None,
    });
    let monaveen =
        crate::data::native_consumable_buffs::conditional_apex_bonuses_for_imported_buffs(
            &monaveen_rows,
            0,
        );
    let mut n_attacker = canary_combatant("canary_n_monaveen");
    n_attacker.attack = 113.0;
    n_attacker.hull_health = 5_000.0;
    n_attacker.apex_barrier = monaveen
        .get("apex_barrier_vs_player_tal_not_on_bridge")
        .copied()
        .unwrap_or(0.0);
    let mut n_defender = canary_combatant("canary_n_target");
    n_defender.hull_health = 12_000.0;
    n_defender.attack = 437.0;

    // Begin at 50% of max hull. Raw Armor must stay unbuffed until the
    // remaining hull drops below 70% of the carried 500 HP.
    let mut arix_attacker = canary_combatant("arix_player");
    arix_attacker.attack = 17.0;
    arix_attacker.hostile_mitigation_params = Some(crate::combat::HostileMitigationParams {
        round_native_defense_stats: true,
        defender_stats: crate::combat::DefenderStats {
            armor: 100.0,
            shield_deflection: 40.0,
            dodge: 40.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: crate::combat::AttackerStats {
            armor_piercing: 100.0,
            shield_piercing: 100.0,
            accuracy: 100.0,
        },
        profile_piercing_fractions: None,
        base_attacker_weapon_stats: Vec::new(),
        ship_type: crate::combat::ShipType::Battleship,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.72,
    });
    let mut arix_defender = canary_combatant("arix_hostile");
    arix_defender.attack = 110.0;
    let mut arix_armor = canary_seat(
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        TimingWindow::RoundStart,
        AbilityEffect::BaseArmorBonus(0.5),
    );
    arix_armor.ability.condition = Some(AbilityCondition::StatBelow {
        stat: "attacker_hull_pct_of_fight_start".into(),
        threshold_pct: 0.7,
    });
    let arix_config = SimulationConfig {
        initial_attacker_hull_damage: 500.0,
        ..canary_config(5, 71)
    };
    let mut counter_piercing = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::HostileCounterStatDebuff {
            reduction: 0.35,
            duration_rounds: 1,
        },
    );
    counter_piercing.ability.condition = Some(AbilityCondition::DefenderIsNpcHostile);

    // An owned generator supplies a weapon bonus only when its Syndicate gate passes.
    use crate::data::building::{
        cumulative_building_level_bonuses, BonusEntry, BuildingLevel, BuildingRecord,
    };
    use crate::data::profile::{apply_profile_to_attacker, OfficerStatRuntimeBonus, PlayerProfile};
    let generator = BuildingRecord {
        id: "canary_generator".into(),
        building_name: "Generator".into(),
        data_version: None,
        source_note: None,
        levels: vec![BuildingLevel {
            level: 1,
            ops_min: None,
            ops_max: None,
            bonuses: vec![BonusEntry {
                stat: "weapon_damage".into(),
                value: 0.5,
                operator: "add".into(),
                conditions: vec!["syndicate_level_at_least_35".into()],
                notes: None,
            }],
        }],
    };
    let building_profile = PlayerProfile {
        bonuses: cumulative_building_level_bonuses(&generator, 1),
        ..Default::default()
    };
    let mut building_attacker = canary_combatant("generator_player");
    building_attacker.attack = 131.0;
    let building_attacker = apply_profile_to_attacker(
        building_attacker,
        &building_profile,
        None,
        OfficerStatRuntimeBonus::default(),
    );
    let mut building_defender = canary_combatant("generator_hostile");
    building_defender.attack = 83.0;

    let shots_spec = CombatEffectSpec {
        id: "canary:round_start_shots".into(),
        source: EffectSource::LcarsOfficer,
        source_ref: None,
        text: None,
        trigger: AbilityTriggerSpec::RoundStart,
        target: AbilityTargetSpec::AttackerSelf,
        modifier: AbilityModifierSpec::ShotsBonus,
        operation: AbilityOperationSpec::Add,
        value: Some(ValueSpec {
            scalar: Some(1.0),
            by_rank: None,
            unit: None,
            officer_stat_scaling: None,
        }),
        chance: Some(ChanceSpec {
            scalar: Some(0.30),
            by_rank: None,
        }),
        duration: Some(DurationSpec::Rounds { rounds: 2 }),
        decay: None,
        accumulate: None,
        conditions: Vec::new(),
        attributes: Default::default(),
        officer: Default::default(),
        stacking: None,
        category: None,
        confidence: None,
    };
    let (timing, effect, condition) =
        compile_officer_combat_spec(&shots_spec).expect("round-start shot canary compiles");
    let mut shots = canary_seat(
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        timing,
        effect,
    );
    shots.ability.condition = condition;
    let mut shots_attacker = canary_combatant("canary_t_attacker");
    shots_attacker.attack = 31.0;
    let mut shots_defender = canary_combatant("canary_t_defender");
    shots_defender.attack = 13.0;

    // Breach-gated hull weapon damage at RoundStart.
    let mut breach_attacker = canary_combatant("canary_v_player");
    breach_attacker.attack = 100.0;
    let mut breach_growth = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::RoundStart,
        AbilityEffect::AccumulatingAttackMultiplier {
            initial: 1.0,
            growth_per_round: 0.25,
            ceiling: 2.0,
        },
    );
    breach_growth.ability.condition = Some(AbilityCondition::DefenderHullBreach);
    let breach_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::RoundStart,
                AbilityEffect::HullBreach {
                    chance: 1.0,
                    duration_rounds: 2,
                    requires_critical: false,
                },
            ),
            breach_growth,
        ],
    };

    let mut opening_attacker = canary_combatant("opening_salvo_attacker");
    opening_attacker.attack = 100.0;
    let mut opening_salvo = canary_seat(
        CrewSeat::Captain,
        AbilityClass::CaptainManeuver,
        TimingWindow::CombatBegin,
        AbilityEffect::AttackMultiplier(0.1),
    );
    opening_salvo.ability.condition = Some(AbilityCondition::RoundRange { min: 1, max: 1 });

    // Fleet Commander weapon damage uses the unexpired active rank only.
    let commander_buffs = [
        GlobalBuffEntry {
            bid: 251_122_208,
            level: 1,
            expiry_time: Some(101),
        },
        GlobalBuffEntry {
            bid: 251_122_208,
            level: 15,
            expiry_time: Some(100),
        },
    ];
    let commander_profile = crate::data::profile::PlayerProfile {
        bonuses: crate::data::native_research_buffs::research_bonuses_for_imported_buffs(
            &commander_buffs,
            None,
            100,
        )
        .unwrap_or_default(),
        ..Default::default()
    };
    let mut commander_attacker = canary_combatant("commander_player");
    commander_attacker.attack = 29.0;
    let commander_attacker = crate::data::profile::apply_profile_to_attacker(
        commander_attacker,
        &commander_profile,
        None,
        Default::default(),
    );
    let mut commander_defender = canary_combatant("commander_target");
    commander_defender.hull_health = 20_000.0;

    // Morale can lapse between cumulative Galaxy Class grants.
    let mut galaxy_attacker = canary_combatant("canary_galaxy_attacker");
    galaxy_attacker.attack = 90.0;
    galaxy_attacker.hull_health = 3_000.0;
    let mut galaxy_defender = canary_combatant("canary_galaxy_defender");
    galaxy_defender.hull_health = 9_000.0;
    let mut growth = canary_seat(
        CrewSeat::Ship,
        AbilityClass::ShipAbility,
        TimingWindow::RoundStart,
        AbilityEffect::GalaxyAdditiveWeaponDamageGrowth {
            growth_per_round: 0.85,
            ceiling: 100.0,
        },
    );
    growth.ability.condition = Some(crate::combat::AbilityCondition::MoraleActive);
    let galaxy_crew = CrewConfiguration {
        seats: vec![
            growth,
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::RoundStart,
                AbilityEffect::Morale(0.5),
            ),
        ],
    };

    // Alternating phasers and a slower torpedo must honor their schedules.
    let mut scheduled_attacker = canary_combatant("canary_scheduled_attacker");
    scheduled_attacker.hull_health = 4_000.0;
    scheduled_attacker.weapons = [(6, 1, 2, 20.0), (6, 2, 2, 20.0), (2, 2, 3, 90.0)]
        .into_iter()
        .map(
            |(shots, first_round, interval_rounds, attack)| crate::combat::WeaponStats {
                attack,
                shots: Some(shots),
                firing_schedule: Some(crate::combat::WeaponFiringSchedule {
                    first_round,
                    interval_rounds,
                }),
                ..Default::default()
            },
        )
        .collect();
    let mut scheduled_defender = canary_combatant("canary_scheduled_defender");
    scheduled_defender.hull_health = 9_000.0;
    let scheduled_crew = CrewConfiguration {
        seats: vec![canary_seat(
            CrewSeat::Bridge,
            AbilityClass::BridgeAbility,
            TimingWindow::AttackPhase,
            AbilityEffect::OnHitTimedBonus {
                crit_chance: 0.0,
                cascade_damage: 0.1,
                duration_rounds: 3,
            },
        )],
    };

    // Track C (1): weapon-scoped OpponentWeaponDamageReduction + reactive HullDamageTaken stacks.
    let mut track_c1_attacker = canary_combatant("canary_track_c1_attacker");
    track_c1_attacker.hull_health = 10_000.0;
    track_c1_attacker.shield_health = 5_000.0;
    track_c1_attacker.shield_mitigation = 0.8;
    track_c1_attacker.weapons = [
        crate::combat::WeaponType::Kinetic,
        crate::combat::WeaponType::Energy,
    ]
    .into_iter()
    .map(|weapon_type| crate::combat::WeaponStats {
        attack: 120.0,
        shots: Some(1),
        crit_chance: Some(0.1),
        crit_multiplier: Some(2.0),
        weapon_type,
        ..Default::default()
    })
    .collect();
    let mut track_c1_defender = track_c1_attacker.clone();
    track_c1_defender.id = "canary_track_c1_defender".to_string();
    let mut opp_wd_seat = canary_seat(
        CrewSeat::Bridge,
        AbilityClass::BridgeAbility,
        TimingWindow::CombatBegin,
        AbilityEffect::OpponentWeaponDamageReduction {
            reduction: 0.25,
            cumulative: false,
        },
    );
    opp_wd_seat.ability.weapon_scope = crate::combat::WeaponTypeScope::EnergyOnly;
    let track_c1_crew = CrewConfiguration {
        seats: vec![
            opp_wd_seat,
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::DefensePhase,
                AbilityEffect::ReactiveDamageTaken {
                    trigger: crate::combat::DamageTakenTrigger::HullDamage,
                    kind: crate::combat::ReactiveDamageTakenKind::CritChanceBonus(0.10),
                    chance: 1.0,
                    duration_rounds: 0,
                },
            ),
            canary_seat(
                CrewSeat::BelowDeck,
                AbilityClass::BelowDeck,
                TimingWindow::DefensePhase,
                AbilityEffect::ReactiveDamageTaken {
                    trigger: crate::combat::DamageTakenTrigger::HullDamage,
                    kind: crate::combat::ReactiveDamageTakenKind::AttackerShieldMitigationBonus(
                        0.08,
                    ),
                    chance: 1.0,
                    duration_rounds: 1,
                },
            ),
        ],
    };

    // Track C (2): CombatBegin BaseMitigationStatsBonus, BasePiercingStatsBonus (with profile
    // dilution), HostileBaseDefenseStatsReduction, and TimedHostileCritChanceReduction.
    let mut track_c2_attacker = canary_combatant("canary_track_c2_attacker");
    track_c2_attacker.attack = 350.0;
    track_c2_attacker.hull_health = 12_000.0;
    track_c2_attacker.hostile_mitigation_params = Some(crate::combat::HostileMitigationParams {
        round_native_defense_stats: false,
        defender_stats: crate::combat::DefenderStats {
            armor: 900.0,
            shield_deflection: 900.0,
            dodge: 900.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: crate::combat::AttackerStats {
            armor_piercing: 1_100.0,
            shield_piercing: 1_100.0,
            accuracy: 1_100.0,
        },
        profile_piercing_fractions: None,
        base_attacker_weapon_stats: Vec::new(),
        ship_type: crate::combat::ShipType::Battleship,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.72,
    });
    let mut track_c2_defender = canary_combatant("canary_track_c2_defender");
    track_c2_defender.attack = 280.0;
    track_c2_defender.crit_chance = 0.85;
    track_c2_defender.crit_multiplier = 2.5;
    track_c2_defender.hull_health = 14_000.0;
    track_c2_defender.hostile_mitigation_params = Some(crate::combat::HostileMitigationParams {
        round_native_defense_stats: false,
        defender_stats: crate::combat::DefenderStats {
            armor: 2_200.0,
            shield_deflection: 2_200.0,
            dodge: 2_200.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: crate::combat::AttackerStats {
            armor_piercing: 1_000.0,
            shield_piercing: 1_000.0,
            accuracy: 1_000.0,
        },
        profile_piercing_fractions: Some(crate::combat::AttackerStats {
            armor_piercing: 0.5,
            shield_piercing: 0.5,
            accuracy: 0.5,
        }),
        base_attacker_weapon_stats: Vec::new(),
        ship_type: crate::combat::ShipType::Battleship,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.72,
    });
    let track_c2_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::BelowDeck,
                AbilityClass::BelowDeck,
                TimingWindow::CombatBegin,
                AbilityEffect::BaseMitigationStatsBonus(0.50),
            ),
            canary_seat(
                CrewSeat::BelowDeck,
                AbilityClass::BelowDeck,
                TimingWindow::CombatBegin,
                AbilityEffect::BasePiercingStatsBonus(0.65),
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileBaseDefenseStatsReduction(0.20),
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::RoundStart,
                AbilityEffect::TimedHostileCritChanceReduction {
                    reduction: 0.35,
                    duration_rounds: 2,
                    cumulative: false,
                },
            ),
        ],
    };

    // Unified op 5 (MultiplyBaseAdd): PRE_ALL defenses & piercing, POST_ALL pre-crit flat weapon
    // damage, on-hit cumulative PRE_ALL piercing, and 1R reactive PRE_ALL piercing.
    let mut op5_attacker = canary_combatant("canary_op5_attacker");
    op5_attacker.attack = 400.0;
    op5_attacker.hull_health = 15_000.0;
    op5_attacker.shield_health = 8_000.0;
    op5_attacker.armor = 3.0;
    op5_attacker.shield_deflection = 3.0;
    op5_attacker.dodge = 3.0;
    op5_attacker.hostile_mitigation_params = Some(crate::combat::HostileMitigationParams {
        round_native_defense_stats: false,
        defender_stats: crate::combat::DefenderStats {
            armor: 200.0,
            shield_deflection: 200.0,
            dodge: 200.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: crate::combat::AttackerStats {
            armor_piercing: 4_000.0,
            shield_piercing: 4_000.0,
            accuracy: 4_000.0,
        },
        profile_piercing_fractions: None,
        base_attacker_weapon_stats: Vec::new(),
        ship_type: crate::combat::ShipType::Interceptor,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.72,
    });
    let mut op5_defender = canary_combatant("canary_op5_defender");
    op5_defender.attack = 320.0;
    op5_defender.hull_health = 22_000.0;
    op5_defender.hostile_mitigation_params = Some(crate::combat::HostileMitigationParams {
        round_native_defense_stats: false,
        defender_stats: crate::combat::DefenderStats {
            armor: 12_000.0,
            shield_deflection: 12_000.0,
            dodge: 12_000.0,
        },
        weapon_pierce_defender_stats: None,
        base_attacker_stats: crate::combat::AttackerStats {
            armor_piercing: 500.0,
            shield_piercing: 500.0,
            accuracy: 500.0,
        },
        profile_piercing_fractions: Some(crate::combat::AttackerStats {
            armor_piercing: 4.0,
            shield_piercing: 4.0,
            accuracy: 4.0,
        }),
        base_attacker_weapon_stats: Vec::new(),
        ship_type: crate::combat::ShipType::Explorer,
        mystery_mitigation_factor: 0.0,
        floor: 0.16,
        ceiling: 0.72,
    });
    let op5_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::BelowDeck,
                AbilityClass::BelowDeck,
                TimingWindow::CombatBegin,
                AbilityEffect::PreAllDefenseAdd {
                    armor: 800.0,
                    shield_deflection: 800.0,
                    dodge: 800.0,
                    cumulative: false,
                },
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::FlatWeaponDamagePreCrit {
                    amount: 250.0,
                    chance: 1.0,
                    cumulative: false,
                },
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::DefensePhase,
                AbilityEffect::ReactiveDamageTaken {
                    trigger: crate::combat::DamageTakenTrigger::AnyDamage,
                    kind: crate::combat::ReactiveDamageTakenKind::PreAllAllPiercingAdd(1_200.0),
                    chance: 1.0,
                    duration_rounds: 1,
                },
            ),
            canary_seat(
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                TimingWindow::AttackPhase,
                AbilityEffect::OnHitPreAllPiercingAdd {
                    armor_piercing: 400.0,
                    shield_piercing: 400.0,
                    accuracy: 400.0,
                },
            ),
        ],
    };

    // Canary ah: Batch B verified mechanics — op 6 flat enemy shield drain (RoundStart & OnHit),
    // Annorax reactive flat Apex Barrier, single-axis ArmorPiercing (launch & on-hit & enemy
    // reduction), and Defensive Tactics + Chen additive opponent weapon-damage reduction.
    let mut batch_b_attacker = op5_attacker.clone();
    batch_b_attacker.id = "canary_batch_b_attacker".to_string();
    batch_b_attacker.apex_barrier = 800.0;
    let mut batch_b_defender = op5_defender.clone();
    batch_b_defender.id = "canary_batch_b_defender".to_string();
    batch_b_defender.shield_health = 8_000.0;
    let batch_b_crew = CrewConfiguration {
        seats: vec![
            canary_seat(
                CrewSeat::Ship,
                AbilityClass::ShipAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::OpponentWeaponDamageReduction {
                    reduction: 0.30,
                    cumulative: false,
                },
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::RoundStart,
                AbilityEffect::DefenderShieldDrainFlat(600.0),
            ),
            canary_seat(
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                TimingWindow::AttackPhase,
                AbilityEffect::OnHitDefenderShieldDrainFlat(250.0),
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::DefensePhase,
                AbilityEffect::ReactiveDamageTaken {
                    trigger: crate::combat::DamageTakenTrigger::AnyDamage,
                    kind: crate::combat::ReactiveDamageTakenKind::ApexBarrierFlatBonus(2_000.0),
                    chance: 1.0,
                    duration_rounds: 2,
                },
            ),
            canary_seat(
                CrewSeat::Captain,
                AbilityClass::CaptainManeuver,
                TimingWindow::CombatBegin,
                AbilityEffect::BaseArmorPiercingBonus(0.35),
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::AttackPhase,
                AbilityEffect::OnHitBaseArmorPiercingBonus(0.40),
            ),
            canary_seat(
                CrewSeat::Bridge,
                AbilityClass::BridgeAbility,
                TimingWindow::CombatBegin,
                AbilityEffect::HostileBaseArmorPiercingReduction(0.50),
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
        (
            "d_conditional_defense_repair",
            d_attacker,
            d_defender,
            canary_config(10, 17),
            d_crew,
        ),
        (
            "e_captain_post_hit",
            e_attacker,
            e_defender,
            canary_config(8, 19),
            e_crew,
        ),
        (
            "f_below_decks_defense",
            f_attacker,
            f_defender,
            canary_config(8, 23),
            CrewConfiguration::default(),
        ),
        (
            "g_native_offense",
            g_attacker,
            canary_combatant("canary_g_defender"),
            canary_config(8, 29),
            g_crew,
        ),
        (
            "h_conditional_max_hull",
            h_attacker,
            h_defender,
            h_config,
            CrewConfiguration {
                seats: vec![h_ability],
            },
        ),
        (
            "i_hostile_crit_reduction",
            i_attacker,
            i_defender,
            canary_config(5, 37),
            i_crew,
        ),
        (
            "j_component_base_damage_and_shot_deflection",
            j_attacker,
            j_defender,
            canary_config(7, 41),
            j_crew,
        ),
        (
            "k_piercing_without_extra_bonus",
            k_attacker,
            k_defender,
            canary_config(3, 43),
            CrewConfiguration::default(),
        ),
        (
            "l_native_syndicate_piercing",
            l_attacker,
            l_defender,
            canary_config(4, 47),
            CrewConfiguration::default(),
        ),
        (
            "m_native_archive_piercing",
            m_attacker,
            m_defender,
            canary_config(5, 53),
            CrewConfiguration::default(),
        ),
        (
            "n_native_monaveen_apex",
            n_attacker,
            n_defender,
            canary_config(6, 59),
            CrewConfiguration::default(),
        ),
        (
            "npc_counter_piercing",
            arix_attacker.clone(),
            arix_defender.clone(),
            canary_config(2, 73),
            CrewConfiguration {
                seats: vec![counter_piercing],
            },
        ),
        (
            "q_powered_armor",
            arix_attacker,
            arix_defender,
            arix_config,
            CrewConfiguration {
                seats: vec![arix_armor],
            },
        ),
        (
            "r_locked_building_weapon_damage",
            building_attacker,
            building_defender,
            canary_config(4, 73),
            CrewConfiguration::default(),
        ),
        (
            "u_recurring_officer_shots",
            shots_attacker,
            shots_defender,
            canary_config(9, 3),
            CrewConfiguration { seats: vec![shots] },
        ),
        (
            "v_breach_weapon_growth",
            breach_attacker,
            canary_combatant("canary_v_hostile"),
            canary_config(3, 79),
            breach_crew,
        ),
        (
            "w_first_round_weapon_damage",
            opening_attacker,
            canary_combatant("opening_salvo_defender"),
            canary_config(3, 97),
            CrewConfiguration {
                seats: vec![opening_salvo],
            },
        ),
        (
            "x_fleet_commander_active_buff",
            commander_attacker,
            commander_defender,
            canary_config(3, 73),
            CrewConfiguration::default(),
        ),
        (
            "enterprise_d_morale_growth",
            galaxy_attacker,
            galaxy_defender,
            canary_config(12, 7),
            galaxy_crew,
        ),
        (
            "enterprise_d_scheduled_volleys",
            scheduled_attacker,
            scheduled_defender,
            canary_config(8, 43),
            scheduled_crew,
        ),
        (
            "track_c1_opp_wd_and_reactive",
            track_c1_attacker,
            track_c1_defender,
            canary_config(6, 53),
            track_c1_crew,
        ),
        (
            "track_c2_raw_defense_piercing_crit_red",
            track_c2_attacker,
            track_c2_defender,
            canary_config(6, 59),
            track_c2_crew,
        ),
        (
            "ag_op5_crew_scaled",
            op5_attacker,
            op5_defender,
            canary_config(6, 61),
            op5_crew,
        ),
        (
            "ah_batch_b_mechanics",
            batch_b_attacker,
            batch_b_defender,
            canary_config(6, 67),
            batch_b_crew,
        ),
    ]
}

// --- data ------------------------------------------------------------------------------------

static DATA_FINGERPRINT: OnceLock<u64> = OnceLock::new();

/// Digest of catalog data used by the registry and profile setup paths. Most catalogs are hashed
/// from the loaded registry; building records and starbase translations are read from disk during
/// profile construction, so their source files are included explicitly. The result is cached for
/// the process because [`DataRegistry::load`] runs once at startup behind an `Arc`.
/// Restart after editing disk-backed catalogs; live catalog edits are not covered by this cache.
pub fn data_catalog_fingerprint(registry: &DataRegistry) -> u64 {
    *DATA_FINGERPRINT.get_or_init(|| {
        compute_data_catalog_fingerprint(
            registry,
            Path::new("data/buildings"),
            Path::new(
                crate::data::building_bid_resolver::DEFAULT_STARBASE_MODULES_TRANSLATIONS_PATH,
            ),
        )
    })
}

/// Build the data segment with explicit inputs for the building files read outside the registry.
/// Keeping this as the production assembly path lets tests verify that those files affect the
/// actual reuse fingerprint rather than only testing the building sub-digest in isolation.
fn compute_data_catalog_fingerprint(
    registry: &DataRegistry,
    buildings_dir: &Path,
    translations_path: &Path,
) -> u64 {
    let mut canonical = String::with_capacity(4096);

    // LCARS officers are the sole ability source and are built in-process from the canonical
    // catalog + upstream stats + translations, sorted by id — so one digest covers all of them.
    // Include runtime-only source bindings as well as the deterministic serialized catalog.
    match registry.lcars_officers() {
        Some(officers) => {
            canonical.push_str(&format!("lcars_count={};", officers.len()));
            if let Ok(digest) = lcars_officer_catalog_digest(officers) {
                canonical.push_str(&format!("lcars={digest:016x};"));
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
    canonical.push_str(&format!(
        "native_consumables={:016x};",
        stable_text_hash(include_str!(
            "../../data/native_consumable_buff_catalog.json"
        ))
    ));
    canonical.push_str(&format!(
        "native_buildings={:016x};",
        stable_text_hash(include_str!("../../data/native_building_buff_catalog.json"))
    ));
    canonical.push_str(&format!(
        "building_catalog={:016x};",
        building_catalog_fingerprint_for_paths(buildings_dir, translations_path)
    ));
    canonical.push_str(&format!(
        "native_research={:016x};",
        stable_text_hash(include_str!(
            "../../data/research/client_buff_semantics_2026-10-20.json"
        ))
    ));
    canonical.push_str(&format!(
        "native_critical_units={:016x};",
        stable_text_hash(include_str!(
            "../../data/native_critical_damage_sources.json"
        ))
    ));
    canonical.push_str(&format!(
        "native_officer_core_stats={:016x};",
        stable_text_hash(include_str!("../../data/native_officer_core_stats.json"))
    ));

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
}

/// Officer-catalog digest alone, for `GET /api/data/version`.
pub fn officer_catalog_digest(registry: &DataRegistry) -> u64 {
    match registry.lcars_officers() {
        Some(officers) => lcars_officer_catalog_digest(officers).unwrap_or(0),
        None => 0,
    }
}

/// Officer resolution also depends on a serde-skipped captain source binding.
/// Keep bindings in the same order as their serialized officers so presence or
/// content changes invalidate both optimizer reuse and the public data version.
/// LCARS contains no HashMaps; this tuple serialization is deterministic.
fn lcars_officer_catalog_digest(
    officers: &[crate::lcars::LcarsOfficer],
) -> Result<u64, serde_json::Error> {
    let source_bindings: Vec<_> = officers
        .iter()
        .map(|o| {
            o.native_synergy
                .as_ref()
                .and_then(|s| s.source_captain_ability.as_ref())
        })
        .collect();
    serde_json::to_string(&(officers, source_bindings)).map(|json| stable_text_hash(&json))
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
    let buffs =
        crate::data::import::load_imported_buffs(&dir.join(BUFFS_IMPORTED).to_string_lossy())
            .unwrap_or_default();
    canonical.push_str(&format!(
        "active_buff_expiry={:016x};",
        active_buff_expiry_membership_digest(&buffs, chrono::Utc::now().timestamp())
    ));
    stable_text_hash(&canonical)
}

/// Time affects a compiled profile only when an expiry boundary is crossed.
/// Hash the active/expired membership, not the clock itself, so cached results
/// stay reusable within an interval but expire even if the mod is offline and
/// the imported JSON bytes have not changed.
fn active_buff_expiry_membership_digest(buffs: &[GlobalBuffEntry], now: i64) -> u64 {
    let mut membership: Vec<_> = buffs
        .iter()
        .filter(|buff| buff.bid > 0 && buff.level > 0)
        .filter_map(|buff| {
            buff.expiry_time
                .map(|expiry| (buff.bid, expiry, expiry > now))
        })
        .collect();
    membership.sort_unstable();
    let mut canonical = String::new();
    for (bid, expiry, active) in membership {
        canonical.push_str(&format!("{bid}:{expiry}:{};", u8::from(active)));
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

/// Digest the files consumed by the building combat bonus path. Building records
/// are read from disk during profile construction rather than retained in
/// `DataRegistry`, so the general in-memory catalog digest cannot cover them.
fn building_catalog_fingerprint_for_paths(buildings_dir: &Path, translations_path: &Path) -> u64 {
    let mut canonical = String::new();
    match std::fs::read_dir(buildings_dir) {
        Ok(entries) => {
            let mut paths = Vec::new();
            let mut entry_errors = 0usize;
            for entry in entries {
                let Ok(entry) = entry else {
                    entry_errors += 1;
                    continue;
                };
                let path = entry.path();
                if path.is_file() && path.extension().is_some_and(|ext| ext == "json") {
                    paths.push(path);
                }
            }
            paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
            canonical.push_str(&format!(
                "building_files={};building_entry_errors={entry_errors};",
                paths.len()
            ));
            for path in paths {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                canonical.push_str(&format!("{name}:{:016x};", file_digest(&path)));
            }
        }
        Err(_) => canonical.push_str("building_files=unavailable;"),
    }
    canonical.push_str(&format!(
        "starbase_module_translations={:016x};",
        file_digest(translations_path)
    ));
    stable_text_hash(&canonical)
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
    let pvp = inputs.defender_ship.is_some_and(|s| !s.trim().is_empty());
    let defender_opponent = if pvp || inputs.defender_opponent.eq_ignore_ascii_case("player") {
        crate::optimizer::monte_carlo::DefenderOpponent::Player
    } else {
        crate::optimizer::monte_carlo::DefenderOpponent::Hostile
    };
    let hostile_rec = if pvp {
        None
    } else {
        registry.resolve_hostile(inputs.hostile)
    };
    let explicit_enemy_type = inputs
        .enemy_type
        .and_then(crate::data::officer_eligibility::enemy_type_from_str);
    let game_context = inputs
        .game_context
        .and_then(crate::combat::CombatGameContext::from_data_slug)
        .unwrap_or_default();
    let resolved_ctx = crate::data::officer_eligibility::resolve_combat_context_with_tactical(
        explicit_enemy_type,
        inputs.wave_defense,
        pvp || defender_opponent.defender_is_player_ship(),
        hostile_rec.as_ref(),
        None,
        game_context,
        inputs.attacker_is_defending,
        inputs.attacker_at_station,
    );
    let engagement_keys = resolved_ctx.canonical_engagement_keys().join(",");
    let canonical_opponent = if resolved_ctx.opponent_kind.is_player() {
        "player"
    } else {
        "hostile"
    };
    let raw_unknown_enemy_type = inputs
        .enemy_type
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto"))
        .filter(|_| explicit_enemy_type.is_none())
        .unwrap_or("");
    let battle_type_key = resolved_ctx
        .combat_battle_type_id
        .map_or_else(|| "none".to_string(), |id| id.to_string());

    let mut canonical = format!(
        "ship={};hostile={};tier={};level={};bd_slots={};engagement={};raw_enemy_type={};defender_opponent={};battle_type={};game_context={};defending={};at_station={};",
        inputs.ship,
        inputs.hostile,
        inputs.ship_tier.unwrap_or(0),
        inputs.ship_level.unwrap_or(0),
        inputs.below_decks_slots,
        engagement_keys,
        raw_unknown_enemy_type,
        canonical_opponent,
        battle_type_key,
        resolved_ctx.combat_game_context.as_data_slug(),
        u8::from(resolved_ctx.attacker_is_defending),
        u8::from(resolved_ctx.attacker_at_station),
    );

    push_ship_record_digest(
        &mut canonical,
        registry,
        "ship_record",
        inputs.ship,
        inputs.ship_tier,
        inputs.ship_level,
    );

    match hostile_rec.and_then(|r| serde_json::to_string(&r).ok()) {
        Some(json) => {
            canonical.push_str(&format!("hostile_record={:016x};", stable_text_hash(&json)))
        }
        // Include the id: every unresolved hostile used to collapse to one marker, while the
        // engine's synthetic fallback derives a *different* fight from each id string — so two
        // different unknown hostiles fingerprinted alike and could reuse each other's results.
        // Ingress now rejects unresolvable ids, but non-server callers still reach this.
        None => canonical.push_str(&format!(
            "hostile_record=unresolved:{};",
            inputs.hostile.trim()
        )),
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
        // Same reasoning as the unresolved hostile above: keep distinct unresolved ships distinct.
        None => out.push_str(&format!(
            "{label}=unresolved:{}:{}:{};",
            ship.trim(),
            tier.map_or_else(|| "-".to_string(), |t| t.to_string()),
            level.map_or_else(|| "-".to_string(), |l| l.to_string())
        )),
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
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn lcars_catalog_digest_includes_hidden_captain_source_binding() {
        let officer: crate::lcars::LcarsOfficer = serde_json::from_value(serde_json::json!({
            "id":"synthetic", "name":"Synthetic",
            "captain_ability": {"name":"Captain", "effects":[{
                "type":"stat_modify", "stat":"weapon_damage", "value":0.1
            }]},
            "native_synergy": {"identity":{"group_id":7,"officer_class":1}}
        }))
        .unwrap();
        let mut officers = vec![officer];
        let serialized = serde_json::to_string(&officers).unwrap();
        let absent = lcars_officer_catalog_digest(&officers).unwrap();
        let original = officers[0].captain_ability.clone().unwrap();
        officers[0]
            .native_synergy
            .as_mut()
            .unwrap()
            .source_captain_ability = Some(original.clone());
        let matching = lcars_officer_catalog_digest(&officers).unwrap();
        assert_eq!(serialized, serde_json::to_string(&officers).unwrap());
        assert_ne!(
            absent, matching,
            "binding presence changes variant eligibility"
        );
        let mut edited = original;
        edited.effects[0].value = Some(0.2);
        officers[0]
            .native_synergy
            .as_mut()
            .unwrap()
            .source_captain_ability = Some(edited);
        let mismatched = lcars_officer_catalog_digest(&officers).unwrap();
        assert_eq!(serialized, serde_json::to_string(&officers).unwrap());
        assert_ne!(
            matching, mismatched,
            "binding content changes variant eligibility"
        );
        assert_ne!(absent, mismatched);
        assert_eq!(mismatched, lcars_officer_catalog_digest(&officers).unwrap());
    }

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
    fn building_catalog_fingerprint_tracks_index_records_and_starbase_translations() {
        let root = std::env::temp_dir().join(format!(
            "kobayashi-building-fingerprint-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let buildings = root.join("buildings");
        std::fs::create_dir_all(&buildings).unwrap();
        struct RemoveDir(std::path::PathBuf);
        impl Drop for RemoveDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _remove_dir = RemoveDir(root.clone());
        let index = buildings.join("index.json");
        let record = buildings.join("75_court_of_q.json");
        let translations = root.join("translations-starbase_modules.json");
        let registry = DataRegistry::load().expect("repository registry should load");
        std::fs::write(
            &index,
            br#"{"buildings":[{"id":"building_75","file":"75_court_of_q"}]}"#,
        )
        .unwrap();
        std::fs::write(
            &record,
            br#"{"id":"building_75","levels":[{"level":1,"bonuses":[] }]}"#,
        )
        .unwrap();
        std::fs::write(
            &translations,
            br#"[{"id":75,"key":"starbase_module_name","text":"Court of Q"}]"#,
        )
        .unwrap();

        let baseline =
            compute_data_catalog_fingerprint(registry.as_ref(), &buildings, &translations);
        assert_eq!(
            baseline,
            compute_data_catalog_fingerprint(registry.as_ref(), &buildings, &translations)
        );

        std::fs::write(
            &record,
            br#"{"id":"building_75","levels":[{"level":1,"bonuses":[{"stat":"weapon_damage","value":0.01}]}]}"#,
        )
        .unwrap();
        assert_ne!(
            baseline,
            compute_data_catalog_fingerprint(registry.as_ref(), &buildings, &translations)
        );

        std::fs::write(
            &record,
            br#"{"id":"building_75","levels":[{"level":1,"bonuses":[] }]}"#,
        )
        .unwrap();
        std::fs::write(
            &index,
            br#"{"buildings":[{"id":"building_75","file":"75_court_of_q","bid":7500}]}"#,
        )
        .unwrap();
        assert_ne!(
            baseline,
            compute_data_catalog_fingerprint(registry.as_ref(), &buildings, &translations)
        );

        std::fs::write(
            &index,
            br#"{"buildings":[{"id":"building_75","file":"75_court_of_q"}]}"#,
        )
        .unwrap();
        std::fs::write(
            &translations,
            br#"[{"id":75,"key":"starbase_module_name","text":"Changed name"}]"#,
        )
        .unwrap();
        assert_ne!(
            baseline,
            compute_data_catalog_fingerprint(registry.as_ref(), &buildings, &translations)
        );
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

    #[test]
    fn piercing_canary_detects_the_removed_automatic_bonus_in_both_directions() {
        let (_, attacker, defender, config, crew) = canary_fights()
            .into_iter()
            .find(|(id, ..)| *id == "k_piercing_without_extra_bonus")
            .unwrap();
        assert_eq!((attacker.pierce, defender.pierce), (0.0, 0.0));
        let corrected = simulate_combat(&attacker, &defender, &config, &crew);
        let mut legacy_outbound = attacker.clone();
        legacy_outbound.pierce = 0.25 * (1.0 - defender.mitigation);
        let outbound = simulate_combat(&legacy_outbound, &defender, &config, &crew);
        assert_ne!(corrected.total_damage, outbound.total_damage);
        let mut legacy_counter = defender.clone();
        legacy_counter.pierce = 0.25 * (1.0 - attacker.mitigation);
        let counter = simulate_combat(&attacker, &legacy_counter, &config, &crew);
        assert_ne!(
            corrected.attacker_hull_remaining,
            counter.attacker_hull_remaining
        );
    }

    #[test]
    fn recurring_shots_canary_detects_old_roll_window_and_chance() {
        let (_, attacker, defender, config, crew) = canary_fights()
            .into_iter()
            .find(|(id, ..)| *id == "u_recurring_officer_shots")
            .unwrap();
        let correct = simulate_combat(&attacker, &defender, &config, &crew).total_damage;
        let mut clipped = crew.clone();
        clipped.seats[0].ability.condition = Some(AbilityCondition::RoundRange { min: 1, max: 2 });
        let clipped = simulate_combat(&attacker, &defender, &config, &clipped).total_damage;
        let mut guaranteed = crew;
        if let AbilityEffect::ShotsBonus { chance, .. } = &mut guaranteed.seats[0].ability.effect {
            *chance = 1.0;
        }
        let guaranteed = simulate_combat(&attacker, &defender, &config, &guaranteed).total_damage;
        assert_ne!(correct, clipped);
        assert_ne!(correct, guaranteed);
    }

    /// A suite whose fights all end the same way would hash the same after most engine changes.
    /// Every fight must reach a distinct outcome, and none may end instantly.
    #[test]
    fn canary_fights_are_not_degenerate() {
        let summaries = canary_fight_summaries();
        assert_eq!(summaries.len(), 43, "expected forty-three canary fights");
        assert!(summaries.iter().any(|s| s.starts_with(
            "af_battle_type_and_pvp_defender_perspective:dmg=1250.000000;ahull=9400.000000;dhull=8750.000000;rounds=2;"
        )));
        assert!(summaries
            .iter()
            .any(|s| s.starts_with("ab_crit_mitigation_rating:ahull=9000.000000;")));
        assert!(summaries.iter().any(|s| s
            .starts_with("ac_infusion_inertial_diverters:dhull=9000.000000;dshield=5000.000000;")));
        assert!(summaries
            .iter()
            .any(|s| s.starts_with("ad_polaron_weapon_mitigation:")
                && s.contains("ashield=5000.000000;")));
        let decay = summaries
            .iter()
            .find(|summary| summary.starts_with("y_hostile_hyperthermic_decay:"))
            .expect("decay canary");
        assert!(
            decay.contains("rounds=2;ahull=6500.000000;"),
            "expected 25% then 10% max-hull decay: {decay}"
        );
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

    #[test]
    fn profile_expiry_digest_changes_only_at_active_buff_boundaries() {
        let buffs = vec![
            GlobalBuffEntry {
                bid: 1,
                level: 1,
                expiry_time: Some(100),
            },
            GlobalBuffEntry {
                bid: 2,
                level: 2,
                expiry_time: None,
            },
            GlobalBuffEntry {
                bid: 3,
                level: 0,
                expiry_time: Some(50),
            },
        ];
        let before = active_buff_expiry_membership_digest(&buffs, 1);
        assert_eq!(before, active_buff_expiry_membership_digest(&buffs, 99));
        let expired = active_buff_expiry_membership_digest(&buffs, 100);
        assert_ne!(before, expired);
        assert_eq!(expired, active_buff_expiry_membership_digest(&buffs, 1_000));
        let mut reordered = buffs.clone();
        reordered.reverse();
        assert_eq!(before, active_buff_expiry_membership_digest(&reordered, 99));
        assert_eq!(
            active_buff_expiry_membership_digest(&[], 1),
            active_buff_expiry_membership_digest(&[], i64::MAX)
        );
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
