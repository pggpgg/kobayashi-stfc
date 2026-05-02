//! Alliance / ship support buff definitions (`data/support_buffs.json`).
//! Virtual research rows and static combat keys apply only in-memory during scenario build.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::data::import::ResearchEntry;
use crate::data::profile::{
    combat_research_bonuses_for_rid_subset, merge_research_bonuses_into_profile,
    profile_combat_bonuses_to_static_style, PlayerProfile, SupportBuffResearchGateState,
    CERRITOS_SUPPORT_GATED_RESEARCH_RIDS, DEFIANT_REINFORCE_GATED_RESEARCH_RIDS,
    TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS, TITAN_CERRITOS_FORTIFIED_DUAL_RESEARCH_RID,
    TITAN_MAX_FORTIFICATION_GATED_RESEARCH_RIDS,
};
use crate::data::research::ResearchCatalog;

pub const DEFAULT_SUPPORT_BUFFS_PATH: &str = "data/support_buffs.json";

/// Max selectable buff ids per request (abuse guard).
pub const MAX_SUPPORT_BUFFS_PER_REQUEST: usize = 8;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SupportBuffResearchLevel {
    pub rid: i64,
    pub level: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SupportBuffStatTarget {
    pub stat: String,
    pub value: f64,
    pub stacking: String,
    #[serde(default)]
    pub layer: Option<String>,
}

/// Where [`SupportBuffDef::static_bonuses`] merge in combat ([`crate::optimizer::monte_carlo::scenario`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupportBuffStaticBonusTarget {
    /// Merge with crew static buffs on the optimizing attacker (default when field omitted).
    #[default]
    Attacker,
    /// Merge only onto the defender [`crate::combat::Combatant`] when [`DefenderOpponent::Player`] (PvP-shaped).
    DefenderIfPlayerOpponent,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SupportBuffDef {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub provenance_notes: Vec<String>,
    #[serde(default)]
    pub exclusive_group: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub stat_targets: Vec<SupportBuffStatTarget>,
    #[serde(default)]
    pub research_levels: Vec<SupportBuffResearchLevel>,
    #[serde(default)]
    pub static_bonuses: HashMap<String, f64>,
    /// When set, [`Self::static_bonuses`] route to attacker vs defender (see [`SupportBuffStaticBonusTarget`]).
    #[serde(default)]
    pub static_bonus_target: Option<SupportBuffStaticBonusTarget>,
}

impl SupportBuffDef {
    #[inline]
    pub fn static_bonus_target_effective(&self) -> SupportBuffStaticBonusTarget {
        self.static_bonus_target.unwrap_or_default()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AppliedSupportBuffTrace {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stat_targets: Vec<SupportBuffStatTarget>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub static_bonuses: BTreeMap<String, f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub research_levels: Vec<SupportBuffResearchLevel>,
}

#[derive(Debug, Clone, Deserialize)]
struct SupportBuffsFile {
    buffs: HashMap<String, SupportBuffDef>,
}

/// Loaded catalog of support buff ids → definitions.
#[derive(Debug, Clone)]
pub struct SupportBuffCatalog {
    buffs: HashMap<String, SupportBuffDef>,
}

impl SupportBuffCatalog {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        let raw = std::fs::read_to_string(path.as_ref())?;
        let file: SupportBuffsFile = serde_json::from_str(&raw)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Self { buffs: file.buffs })
    }

    pub fn empty() -> Self {
        Self {
            buffs: HashMap::new(),
        }
    }

    pub fn known_ids(&self) -> impl Iterator<Item = &String> {
        self.buffs.keys()
    }

    pub fn get(&self, id: &str) -> Option<&SupportBuffDef> {
        self.buffs.get(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &SupportBuffDef)> {
        self.buffs.iter()
    }
}

fn support_static_key_is_supported(key: &str) -> bool {
    matches!(
        key,
        "weapon_damage"
            | "hull_hp"
            | "shield_hp"
            | "crit_chance"
            | "crit_damage"
            | "shield_pierce"
            | "armor_pierce"
            | "shield_mitigation"
            | "armor"
            | "dodge"
            | "damage_reduction"
            | "isolytic_damage"
            | "isolytic_defense"
            | "isolytic_cascade"
            | "isolytic_cascade_damage"
            | "apex_shred"
            | "apex_barrier"
            | "accuracy"
            | "accuracy_cb_mult"
    )
}

/// Semantic validation for `data/support_buffs.json`.
///
/// The catalog is request-scoped combat data, so invalid static keys are errors: otherwise a typo
/// can silently become an unapplied combat modifier.
pub fn support_buff_catalog_validation_issues(catalog: &SupportBuffCatalog) -> Vec<String> {
    let mut issues = Vec::new();

    for (id, def) in catalog.iter() {
        let context = format!("support_buffs.{id}");
        if id.trim().is_empty() {
            issues.push("support_buffs: buff id key must not be empty".to_string());
        }
        match def.id.as_deref().map(str::trim) {
            Some(value) if value == id => {}
            Some("") => {
                issues.push(format!("{context}: field `id` must not be empty"));
            }
            Some(value) => {
                issues.push(format!(
                    "{context}: field `id` must match map key `{id}` (got `{value}`)"
                ));
            }
            None => {
                issues.push(format!("{context}: missing field `id`"));
            }
        }

        for (field, value) in [
            ("display_name", def.display_name.as_deref()),
            ("source", def.source.as_deref()),
        ] {
            if value.is_none_or(|s| s.trim().is_empty()) {
                issues.push(format!("{context}: `{field}` must not be empty"));
            }
        }
        if def
            .provenance_notes
            .iter()
            .all(|note| note.trim().is_empty())
        {
            issues.push(format!(
                "{context}: `provenance_notes` must include at least one non-empty note"
            ));
        }
        if def
            .exclusive_group
            .as_deref()
            .is_some_and(|group| group.trim().is_empty())
        {
            issues.push(format!(
                "{context}: `exclusive_group` must not be empty when set"
            ));
        }

        for row in &def.research_levels {
            if row.rid <= 0 {
                issues.push(format!(
                    "{context}: research_levels contains non-positive rid {}",
                    row.rid
                ));
            }
            if row.level == 0 {
                issues.push(format!(
                    "{context}: research_levels rid {} must use a non-zero level",
                    row.rid
                ));
            }
        }

        let mut target_by_stat: HashMap<&str, &SupportBuffStatTarget> = HashMap::new();
        for target in &def.stat_targets {
            let stat = target.stat.trim();
            if stat.is_empty() {
                issues.push(format!("{context}: stat_targets contains an empty stat"));
                continue;
            }
            if !target.value.is_finite() {
                issues.push(format!(
                    "{context}: stat target `{stat}` value must be finite"
                ));
            }
            if target
                .layer
                .as_deref()
                .is_some_and(|layer| layer != "static_bonuses")
            {
                issues.push(format!(
                    "{context}: stat target `{stat}` has unsupported layer {:?}",
                    target.layer
                ));
            }
            if target_by_stat.insert(stat, target).is_some() {
                issues.push(format!("{context}: duplicate stat target `{stat}`"));
            }
            if target.layer.as_deref() == Some("static_bonuses")
                && !def.static_bonuses.contains_key(stat)
            {
                issues.push(format!(
                    "{context}: stat target `{stat}` is missing matching static_bonuses value"
                ));
            }
        }

        for (stat, value) in &def.static_bonuses {
            let stat = stat.trim();
            if stat.is_empty() {
                issues.push(format!(
                    "{context}: static_bonuses contains an empty stat key"
                ));
                continue;
            }
            if !support_static_key_is_supported(stat) {
                issues.push(format!(
                    "{context}: static bonus `{stat}` is not consumed by the static combat layer"
                ));
            }
            if !value.is_finite() {
                issues.push(format!(
                    "{context}: static bonus `{stat}` value must be finite"
                ));
            }
            let Some(target) = target_by_stat.get(stat).copied() else {
                issues.push(format!(
                    "{context}: static bonus `{stat}` is missing stat_targets metadata"
                ));
                continue;
            };
            if target.layer.as_deref() != Some("static_bonuses") {
                issues.push(format!(
                    "{context}: stat target `{stat}` must declare layer `static_bonuses`"
                ));
            }
            if target.value != *value {
                issues.push(format!(
                    "{context}: stat target `{stat}` value {} does not match static bonus {}",
                    target.value, value
                ));
            }
            let expected_stacking = if is_static_mult_key(stat) {
                "multiplicative"
            } else {
                "additive"
            };
            if target.stacking != expected_stacking {
                issues.push(format!(
                    "{context}: stat target `{stat}` stacking must be `{expected_stacking}`"
                ));
            }
        }
    }

    issues
}

/// Normalize selection: cap length, known ids only, apply exclusive_group (highest priority wins).
pub fn resolve_selected_support_buff_ids(
    catalog: &SupportBuffCatalog,
    requested: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut unknown = Vec::new();
    let mut known: Vec<&str> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for id in requested.iter().take(MAX_SUPPORT_BUFFS_PER_REQUEST) {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if catalog.get(trimmed).is_some() {
            if seen.insert(trimmed.to_string()) {
                known.push(trimmed);
            }
        } else {
            unknown.push(trimmed.to_string());
        }
    }

    // Exclusive groups: one winner per `exclusive_group` — highest `priority`; tie: later in `known` wins.
    let mut group_members: HashMap<String, Vec<&str>> = HashMap::new();
    for id in &known {
        let def = catalog.get(id).unwrap();
        if let Some(ref g) = def.exclusive_group {
            group_members.entry(g.clone()).or_default().push(*id);
        }
    }
    let mut remove: HashSet<String> = HashSet::new();
    for (_g, members) in group_members {
        if members.len() <= 1 {
            continue;
        }
        let mut best = members[0];
        let mut best_idx = known.iter().position(|x| *x == best).unwrap();
        let mut best_pr = catalog.get(best).unwrap().priority;
        for &id in members.iter().skip(1) {
            let pr = catalog.get(id).unwrap().priority;
            let idx = known.iter().position(|x| *x == id).unwrap();
            if pr > best_pr || (pr == best_pr && idx > best_idx) {
                best = id;
                best_idx = idx;
                best_pr = pr;
            }
        }
        for &id in &members {
            if id != best {
                remove.insert(id.to_string());
            }
        }
    }

    let mut resolved: Vec<String> = known
        .iter()
        .filter(|id| !remove.contains(**id))
        .map(|s| (*s).to_string())
        .collect();
    resolved.sort();
    resolved.dedup();
    (resolved, unknown)
}

/// Merge virtual research from selected buffs into `profile` (in-memory only).
pub fn apply_support_buff_research_to_profile(
    profile: &mut PlayerProfile,
    catalog: &SupportBuffCatalog,
    resolved_ids: &[String],
    research_catalog: Option<&ResearchCatalog>,
) {
    let Some(rc) = research_catalog else {
        return;
    };
    if rc.items.is_empty() {
        return;
    }

    let mut synthetic: Vec<ResearchEntry> = Vec::new();
    for id in resolved_ids {
        let Some(def) = catalog.get(id) else {
            continue;
        };
        for row in &def.research_levels {
            if row.level == 0 {
                continue;
            }
            synthetic.push(ResearchEntry {
                rid: row.rid,
                level: i64::from(row.level),
            });
        }
    }
    if !synthetic.is_empty() {
        // Support-buff virtual `rid`s are not Titan-A Fortify–gated; Fortify only affects synced tree rids.
        merge_research_bonuses_into_profile(profile, &synthetic, rc);
    }
}

/// Keys where LCARS/static buff maps use multiplicative stacking (see `apply_static_buffs_to_combatant`).
const STATIC_MULT_KEYS: &[&str] = &[
    "weapon_damage",
    "hull_hp",
    "shield_hp",
    "crit_damage",
    "accuracy_cb_mult",
];

fn is_static_mult_key(k: &str) -> bool {
    STATIC_MULT_KEYS.contains(&k)
}

fn merge_def_static_bonuses_into(out: &mut HashMap<String, f64>, def: &SupportBuffDef) {
    for (k, v) in &def.static_bonuses {
        out.entry(k.clone())
            .and_modify(|e| {
                if is_static_mult_key(k) {
                    *e *= *v;
                } else {
                    *e += *v;
                }
            })
            .or_insert(*v);
    }
}

/// Split `static_bonuses` by [`SupportBuffDef::static_bonus_target_effective`] for attacker vs PvP defender merge paths.
pub fn aggregate_support_static_bonuses_split(
    catalog: &SupportBuffCatalog,
    resolved_ids: &[String],
) -> (HashMap<String, f64>, HashMap<String, f64>) {
    let mut attacker: HashMap<String, f64> = HashMap::new();
    let mut defender_player: HashMap<String, f64> = HashMap::new();
    for id in resolved_ids {
        let Some(def) = catalog.get(id) else {
            continue;
        };
        match def.static_bonus_target_effective() {
            SupportBuffStaticBonusTarget::Attacker => {
                merge_def_static_bonuses_into(&mut attacker, def)
            }
            SupportBuffStaticBonusTarget::DefenderIfPlayerOpponent => {
                merge_def_static_bonuses_into(&mut defender_player, def);
            }
        }
    }
    (attacker, defender_player)
}

/// Aggregate attacker-routed `static_bonuses` only (same as split `.0`).
pub fn aggregate_support_static_bonuses(
    catalog: &SupportBuffCatalog,
    resolved_ids: &[String],
) -> HashMap<String, f64> {
    aggregate_support_static_bonuses_split(catalog, resolved_ids).0
}

/// Resolved ids whose `static_bonuses` apply only vs a player-shaped defender (`defender_opponent: player`).
pub fn resolved_defender_routed_support_buff_ids(
    catalog: &SupportBuffCatalog,
    resolved_ids: &[String],
) -> Vec<String> {
    resolved_ids
        .iter()
        .filter(|id| {
            catalog.get(id.as_str()).is_some_and(|d| {
                d.static_bonus_target_effective()
                    == SupportBuffStaticBonusTarget::DefenderIfPlayerOpponent
            })
        })
        .cloned()
        .collect()
}

/// Human-readable labels for [`SupportBuffStaticBonusTarget::DefenderIfPlayerOpponent`] entries (warnings / notes).
pub fn inactive_defender_static_support_buff_labels(
    catalog: &SupportBuffCatalog,
    requested: &[String],
    defender_is_player_ship: bool,
) -> Vec<String> {
    if defender_is_player_ship {
        return Vec::new();
    }
    let (resolved, _) = resolve_selected_support_buff_ids(catalog, requested);
    let mut labels: Vec<String> = Vec::new();
    for id in resolved {
        let Some(def) = catalog.get(&id) else {
            continue;
        };
        if def.static_bonus_target_effective()
            != SupportBuffStaticBonusTarget::DefenderIfPlayerOpponent
        {
            continue;
        }
        if def.static_bonuses.is_empty() {
            continue;
        }
        let label = def
            .display_name
            .clone()
            .or_else(|| def.label.clone())
            .unwrap_or_else(|| id.clone());
        labels.push(label);
    }
    labels.sort();
    labels.dedup();
    labels
}

pub fn describe_resolved_support_buffs(
    catalog: &SupportBuffCatalog,
    resolved_ids: &[String],
) -> Vec<AppliedSupportBuffTrace> {
    resolved_ids
        .iter()
        .filter_map(|id| {
            let def = catalog.get(id)?;
            Some(AppliedSupportBuffTrace {
                id: id.clone(),
                display_name: def.display_name.clone().or_else(|| def.label.clone()),
                source: def.source.clone(),
                stat_targets: def.stat_targets.clone(),
                static_bonuses: def
                    .static_bonuses
                    .iter()
                    .map(|(stat, value)| (stat.clone(), *value))
                    .collect(),
                research_levels: def.research_levels.clone(),
            })
        })
        .collect()
}

/// Merge crew LCARS static buffs with support static bonuses for mitigation + combatant application.
pub fn merge_static_buff_maps(
    crew: &HashMap<String, f64>,
    support: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    let mut keys: HashSet<String> = crew.keys().cloned().collect();
    keys.extend(support.keys().cloned());

    let mut out = HashMap::new();
    for k in keys {
        let a = crew.get(&k).copied();
        let b = support.get(&k).copied();
        if a.is_none() && b.is_none() {
            continue;
        }
        let v = if is_static_mult_key(&k) {
            let x = a.unwrap_or(1.0);
            let y = b.unwrap_or(1.0);
            if x.is_finite() && y.is_finite() {
                x * y
            } else {
                1.0
            }
        } else {
            a.unwrap_or(0.0) + b.unwrap_or(0.0)
        };
        out.insert(k, v);
    }
    out
}

/// Merges catalog combat stats from support-buff–gated research into `support_static` (same layer as
/// `static_bonuses` from `data/support_buffs.json`), using [`merge_static_buff_maps`] per stat key.
pub fn augment_static_buffs_with_support_gated_research(
    support_static: &mut HashMap<String, f64>,
    imported: &[ResearchEntry],
    catalog: &ResearchCatalog,
    gates: &SupportBuffResearchGateState,
) {
    let mut merge_layer = |rids: &[i64]| {
        let m = combat_research_bonuses_for_rid_subset(imported, catalog, rids);
        if m.is_empty() {
            return;
        }
        let layer = profile_combat_bonuses_to_static_style(&m);
        *support_static = merge_static_buff_maps(support_static, &layer);
    };
    if gates.cerritos_support {
        merge_layer(CERRITOS_SUPPORT_GATED_RESEARCH_RIDS);
    }
    if gates.titan_fortify {
        merge_layer(TITAN_A_FORTIFY_GATED_COMBAT_RESEARCH_RIDS);
    }
    if gates.titan_max_fortification {
        merge_layer(TITAN_MAX_FORTIFICATION_GATED_RESEARCH_RIDS);
    }
    if gates.defiant_reinforce {
        merge_layer(DEFIANT_REINFORCE_GATED_RESEARCH_RIDS);
    }
    if gates.cerritos_support && gates.titan_fortify {
        merge_layer(&[TITAN_CERRITOS_FORTIFIED_DUAL_RESEARCH_RID]);
    }
}

/// Load catalog for server/registry; returns `None` if file missing (caller may use empty).
pub fn load_support_buff_catalog(path: impl AsRef<Path>) -> Option<Arc<SupportBuffCatalog>> {
    match SupportBuffCatalog::load(path) {
        Ok(c) => Some(Arc::new(c)),
        Err(_) => None,
    }
}

/// Resolve request ids, merge virtual research into `profile`, return static map for combat merge.
/// Returns `(resolved_ids, support_static, unknown_ids)`.
pub fn apply_support_buffs_for_request(
    profile: &mut PlayerProfile,
    catalog: Option<&SupportBuffCatalog>,
    research_catalog: Option<&ResearchCatalog>,
    requested: Option<&[String]>,
) -> (Vec<String>, HashMap<String, f64>, Vec<String>) {
    let Some(cat) = catalog else {
        return (Vec::new(), HashMap::new(), Vec::new());
    };
    let Some(req) = requested.filter(|r| !r.is_empty()) else {
        return (Vec::new(), HashMap::new(), Vec::new());
    };
    let (resolved, unknown) = resolve_selected_support_buff_ids(cat, req);
    apply_support_buff_research_to_profile(profile, cat, &resolved, research_catalog);
    let (support_static, _) = aggregate_support_static_bonuses_split(cat, &resolved);
    (resolved, support_static, unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_catalog() -> SupportBuffCatalog {
        let mut buffs = HashMap::new();
        buffs.insert(
            "a".into(),
            SupportBuffDef {
                exclusive_group: Some("g1".into()),
                priority: 1,
                ..Default::default()
            },
        );
        buffs.insert(
            "b".into(),
            SupportBuffDef {
                exclusive_group: Some("g1".into()),
                priority: 2,
                ..Default::default()
            },
        );
        SupportBuffCatalog { buffs }
    }

    #[test]
    fn exclusive_group_keeps_higher_priority() {
        let c = tiny_catalog();
        let (resolved, _) = resolve_selected_support_buff_ids(&c, &["a".into(), "b".into()]);
        assert_eq!(resolved, vec!["b".to_string()]);
    }

    #[test]
    fn aggregate_split_routes_fortify_static_to_defender_map() {
        let c = SupportBuffCatalog::load(DEFAULT_SUPPORT_BUFFS_PATH).unwrap();
        let (att, def) =
            aggregate_support_static_bonuses_split(&c, &["titan_a_fortification".to_string()]);
        assert!(!att.contains_key("crit_damage"));
        assert!((def.get("crit_damage").copied().unwrap_or(0.0) - 1.25).abs() < 1e-9);
    }

    #[test]
    fn aggregate_split_cerritos_stays_on_attacker() {
        let c = SupportBuffCatalog::load(DEFAULT_SUPPORT_BUFFS_PATH).unwrap();
        let (att, def) =
            aggregate_support_static_bonuses_split(&c, &["cerritos_support".to_string()]);
        assert!(def.is_empty());
        assert!(att.contains_key("weapon_damage"));
    }

    #[test]
    fn inactive_defender_labels_only_when_static_present_and_npc_defender() {
        let c = SupportBuffCatalog::load(DEFAULT_SUPPORT_BUFFS_PATH).unwrap();
        let labels = inactive_defender_static_support_buff_labels(
            &c,
            &["titan_a_fortification".to_string()],
            false,
        );
        assert_eq!(labels, vec!["Fortification".to_string()]);
        assert!(inactive_defender_static_support_buff_labels(
            &c,
            &["titan_a_fortification".to_string()],
            true,
        )
        .is_empty());
    }

    #[test]
    fn catalog_loads_display_metadata() {
        let c = SupportBuffCatalog::load(DEFAULT_SUPPORT_BUFFS_PATH).unwrap();
        let fortify = c.get("titan_a_fortification").unwrap();
        assert_eq!(fortify.id.as_deref(), Some("titan_a_fortification"));
        assert_eq!(fortify.label.as_deref(), Some("Fortification"));
        assert_eq!(fortify.display_name.as_deref(), Some("Fortification"));
        assert_eq!(
            fortify.source.as_deref(),
            Some("Titan-A Fortify alliance support")
        );
        assert!(!fortify.provenance_notes.is_empty());
        assert!(fortify
            .description
            .as_deref()
            .unwrap()
            .contains("Titan-A Fortify"));

        let cerritos = c.get("cerritos_support").unwrap();
        assert_eq!(cerritos.label.as_deref(), Some("Cerritos Support"));
        assert_eq!(cerritos.display_name.as_deref(), Some("Cerritos Support"));
    }

    #[test]
    fn catalog_entries_define_canonical_schema_metadata() {
        let c = SupportBuffCatalog::load(DEFAULT_SUPPORT_BUFFS_PATH).unwrap();
        for (id, def) in &c.buffs {
            assert_eq!(def.id.as_deref(), Some(id.as_str()));
            assert!(def.display_name.as_deref().is_some_and(|s| !s.is_empty()));
            assert!(def.source.as_deref().is_some_and(|s| !s.is_empty()));
            assert!(
                def.provenance_notes.iter().any(|note| !note.is_empty()),
                "{id} should include provenance notes"
            );

            for (stat, value) in &def.static_bonuses {
                let target = def
                    .stat_targets
                    .iter()
                    .find(|target| target.stat == *stat)
                    .unwrap_or_else(|| panic!("{id} missing stat target metadata for {stat}"));
                assert_eq!(target.value, *value);
                assert_eq!(target.layer.as_deref(), Some("static_bonuses"));
                if is_static_mult_key(stat) {
                    assert_eq!(target.stacking, "multiplicative");
                } else {
                    assert_eq!(target.stacking, "additive");
                }
            }
        }
    }

    #[test]
    fn bundled_catalog_passes_semantic_validation() {
        let c = SupportBuffCatalog::load(DEFAULT_SUPPORT_BUFFS_PATH).unwrap();
        let issues = support_buff_catalog_validation_issues(&c);
        assert!(
            issues.is_empty(),
            "support buff validation issues: {issues:?}"
        );
    }

    #[test]
    fn merge_static_mult_multiplies() {
        let mut crew = HashMap::new();
        crew.insert("weapon_damage".into(), 1.1);
        let mut sup = HashMap::new();
        sup.insert("weapon_damage".into(), 1.05);
        let m = merge_static_buff_maps(&crew, &sup);
        assert!((m.get("weapon_damage").copied().unwrap() - 1.155).abs() < 1e-6);
    }

    #[test]
    fn merge_static_additive_sums_accuracy() {
        let mut crew = HashMap::new();
        crew.insert("accuracy".into(), 100.0);
        let mut sup = HashMap::new();
        sup.insert("accuracy".into(), 50.0);
        let m = merge_static_buff_maps(&crew, &sup);
        assert!((m.get("accuracy").copied().unwrap() - 150.0).abs() < 1e-6);
    }
}
