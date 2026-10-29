//! Calibration anchors for first-class officer A/D/H — docs/OFFICER_STAT_FORMULA.md §"Examples
//! to validate the spec against" (§261-263).
//!
//! Kras's enemy-bridge routing remains modeled. Cadet Kirk's bridge-only scope is
//! quarantined, and Marla's stat effect is economic rather than a combat bonus.
//! Recorded in-game damage deltas remain pending for these anchors.

use std::collections::HashMap;

use kobayashi::combat::CrewOfficerStatTotals;
use kobayashi::data::combat_effect_spec::AbilityConditionSpec;
use kobayashi::data::profile::{
    compute_officer_stat_runtime_bonus, OfficerStatConditionContext, PlayerProfile,
};
use kobayashi::data::ship::{OfficerBonusBreakpoint, OfficerBonusTable, ShipRecord};
use kobayashi::lcars::{
    build_officer_model_file_default, index_lcars_officers_by_id, resolve_crew_to_buff_set,
    BuffSet, OfficerStatOpponentScope, PendingOfficerStatContribution, ResolveOptions,
};

/// Load the bundled production LCARS catalog. Returns `None` when the data file is absent so the
/// tests skip gracefully in stripped checkouts (mirrors `tests/officer_kirk_morale_stat.rs`).
fn bundled_officers() -> Option<(
    std::collections::HashMap<String, kobayashi::lcars::LcarsOfficer>,
    ResolveOptions,
)> {
    let file = build_officer_model_file_default().ok()?;
    let officers = index_lcars_officers_by_id(file.officers);
    let opts = ResolveOptions {
        tier: Some(1),
        officer_tiers: None,
        officer_levels: None,
    };
    Some((officers, opts))
}

/// Resolve a single officer in the captain seat (which fires both captain and bridge abilities).
fn resolve_as_captain(captain_id: &str) -> Option<BuffSet> {
    let (officers, opts) = bundled_officers()?;
    Some(resolve_crew_to_buff_set(
        captain_id,
        &[],
        &[],
        &officers,
        &opts,
    ))
}

/// Representative explorer ship with a coarse officer-bonus breakpoint table. Breakpoints double
/// every step (bonus 0.4 → 0.8 → 1.6 → 3.2 → 6.4). Not a specific in-game ship — a deterministic
/// stand-in until the anchor's real ship/crew are supplied. `shield_deflection` is the explorer
/// defense channel constant (§2c).
fn representative_explorer() -> ShipRecord {
    let table = || {
        vec![
            OfficerBonusBreakpoint {
                value: 1000.0,
                bonus: 0.4,
            },
            OfficerBonusBreakpoint {
                value: 2000.0,
                bonus: 0.8,
            },
            OfficerBonusBreakpoint {
                value: 4000.0,
                bonus: 1.6,
            },
            OfficerBonusBreakpoint {
                value: 8000.0,
                bonus: 3.2,
            },
            OfficerBonusBreakpoint {
                value: 16000.0,
                bonus: 6.4,
            },
        ]
    };
    ShipRecord {
        ship_class: "explorer".to_string(),
        armor: 1000.0,
        shield_deflection: 1000.0,
        dodge: 1000.0,
        officer_bonus: OfficerBonusTable {
            attack: table(),
            defense: table(),
            health: table(),
        },
        ..Default::default()
    }
}

// Cadet Kirk's bridge-only scope is quarantined; Marla's bonus is economic.
#[test]
fn unsupported_or_economic_stat_anchors_do_not_boost_combat_crew() {
    use kobayashi::combat::AbilityEffect;
    let kirk = resolve_as_captain("cadet-kirk-a80563").unwrap();
    assert!(!kirk.static_buffs.contains_key("officer_stat_all"));
    assert!(kirk.crew.seats.iter().any(|seat| matches!(
        seat.ability.effect,
        AbilityEffect::UnsupportedOfficerEffect { .. }
    )));
    let marla = resolve_as_captain("marla-9732c7").unwrap();
    assert!(!marla.static_buffs.contains_key("officer_stat_all"));
}

// ---------------------------------------------------------------------------
// Anchor 3 — Kras "Know your Enemy" -20% all stats on the ENEMY bridge.
// ---------------------------------------------------------------------------

#[test]
fn anchor_kras_know_your_enemy_minus_20pct_enemy_bridge_routing() {
    let Some(buff) = resolve_as_captain("kras-a47042") else {
        return;
    };

    // The debuff must NOT boost Kras's own crew.
    let self_stat_all = buff
        .static_buffs
        .get("officer_stat_all")
        .copied()
        .unwrap_or(0.0);
    assert!(
        self_stat_all.abs() < 1e-9,
        "Kras 'Know your Enemy' is an enemy debuff; it must not appear in self static_buffs (got {self_stat_all})"
    );

    // It must be carried as a pending enemy-bridge contribution for PvP defender-side compute.
    let kras_pending: Vec<&PendingOfficerStatContribution> = buff
        .pending_officer_stat_contributions
        .iter()
        .filter(|p| p.stat_key == "officer_stat_all" && !p.target_attacker)
        .collect();
    assert_eq!(
        kras_pending.len(),
        1,
        "expected exactly one enemy-targeted officer_stat_all pending contribution; got {:?}",
        buff.pending_officer_stat_contributions
    );
    let kras = kras_pending[0];
    assert!(
        (kras.value - 0.20).abs() < 1e-9,
        "Kras debuff magnitude should be 0.20 (applied as a reduction); got {}",
        kras.value
    );
    assert!(
        matches!(
            kras.opponent_scope,
            OfficerStatOpponentScope::BridgeOfficers
        ),
        "Kras 'Know your Enemy' targets the enemy bridge only; got {:?}",
        kras.opponent_scope
    );
    // The debuff is PvP-gated: it only fires when the defender is a player ship. This is exactly
    // why it is a no-op versus NPC hostiles (spec §214).
    assert!(
        matches!(
            kras.conditions.as_slice(),
            [AbilityConditionSpec::DefenderIsPlayerShip]
        ),
        "Kras 'Know your Enemy' should be gated on DefenderIsPlayerShip; got {:?}",
        kras.conditions
    );
}

#[test]
fn anchor_kras_debuff_is_noop_versus_npc_hostile_but_active_in_pvp() {
    let Some(buff) = resolve_as_captain("kras-a47042") else {
        return;
    };
    let pending = buff.pending_officer_stat_contributions.clone();
    let ship = representative_explorer();
    let profile = PlayerProfile::default();

    // A player defender with a real bridge crew, sized just above the 2000 breakpoint so a -20%
    // reduction drops it back below — making any applied debuff visible as a bonus drop.
    let defender_totals = CrewOfficerStatTotals {
        attack: 2100.0,
        defense: 2100.0,
        health: 2100.0,
    };
    let bonus = |ctx: &OfficerStatConditionContext, opp: &[PendingOfficerStatContribution]| {
        compute_officer_stat_runtime_bonus(
            defender_totals,
            defender_totals,
            &ship,
            &profile,
            None,
            &HashMap::new(),
            &[],
            ctx,
            opp,
        )
    };

    // §214: vs an NPC hostile, `defender_is_player_ship = false`, so Kras's `DefenderIsPlayerShip`
    // gate evaluates false and the debuff is dropped → identical to having no debuff at all.
    let pve_ctx = OfficerStatConditionContext::default();
    let no_debuff = bonus(&pve_ctx, &[]);
    let pve_with_kras = bonus(&pve_ctx, &pending);
    assert_eq!(
        pve_with_kras, no_debuff,
        "Kras debuff must be a no-op vs an NPC hostile (DefenderIsPlayerShip gate is false)"
    );

    // PvP: defender IS a player ship → the gate passes and the -20% enemy-bridge debuff applies.
    let pvp_ctx = OfficerStatConditionContext {
        defender_is_player_ship: true,
        ..Default::default()
    };
    let with_debuff = bonus(&pvp_ctx, &pending);
    assert!(
        with_debuff.defense_shield_deflection_add <= no_debuff.defense_shield_deflection_add
            && with_debuff.attack_bonus <= no_debuff.attack_bonus
            && with_debuff.health_bonus <= no_debuff.health_bonus,
        "Kras -20% must not raise any defender axis (no_debuff={no_debuff:?}, with_debuff={with_debuff:?})"
    );
    // Recorded engine prediction (pending in-game confirmation per spec §263):
    //   defense_rating 2100 → defense_bonus 0.8 → shield_defl_add 800 ; ×0.80 = 1680 → 0.4 → 400.
    assert!(
        with_debuff.defense_shield_deflection_add < no_debuff.defense_shield_deflection_add,
        "ENGINE PREDICTION (Kras -20% PvP): defender shield_defl_add {} → {} ; attack_bonus {} → {}.",
        no_debuff.defense_shield_deflection_add,
        with_debuff.defense_shield_deflection_add,
        no_debuff.attack_bonus,
        with_debuff.attack_bonus,
    );
}

/// When the snapshot-bound suite is populated, magnitude anchors live in
/// `recorded_fight_suite.json` (`officer_anchor`: kirk | marla | kras) with observed `bands`.
#[test]
fn officer_stat_magnitude_anchors_use_recorded_fight_manifest_when_present() {
    use std::path::Path;

    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/recorded_fights/recorded_fight_suite.json");
    let suite =
        kobayashi::calibration::load_recorded_fight_suite(&manifest).expect("load manifest");
    let anchors: std::collections::BTreeMap<_, _> = suite
        .fights
        .iter()
        .filter_map(|f| {
            f.officer_anchor
                .as_ref()
                .map(|a| (a.as_str(), f.id.as_str()))
        })
        .collect();
    // Pre-freeze: empty manifest — test documents hook for post-freeze Kirk/Marla/Kras fights.
    if anchors.is_empty() {
        return;
    }
    for key in ["kirk", "marla", "kras"] {
        assert!(
            anchors.contains_key(key),
            "expected officer_anchor fight for {key} in recorded_fight_suite.json"
        );
    }
}
