//! Alliance / ship support buff definitions (`data/support_buffs.json`).
//! Virtual research rows and static combat keys apply only in-memory during scenario build.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use serde::Deserialize;

use crate::data::import::ResearchEntry;
use crate::data::profile::{merge_research_bonuses_into_profile, PlayerProfile};
use crate::data::research::ResearchCatalog;

pub const DEFAULT_SUPPORT_BUFFS_PATH: &str = "data/support_buffs.json";

/// Max selectable buff ids per request (abuse guard).
pub const MAX_SUPPORT_BUFFS_PER_REQUEST: usize = 8;

#[derive(Debug, Clone, Deserialize)]
pub struct SupportBuffResearchLevel {
    pub rid: i64,
    pub level: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SupportBuffDef {
    #[serde(default)]
    pub exclusive_group: Option<String>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default)]
    pub research_levels: Vec<SupportBuffResearchLevel>,
    #[serde(default)]
    pub static_bonuses: HashMap<String, f64>,
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
        merge_research_bonuses_into_profile(profile, &synthetic, rc);
    }
}

/// Aggregate `static_bonuses` from resolved buff defs into one map (mult keys multiply across buffs).
pub fn aggregate_support_static_bonuses(
    catalog: &SupportBuffCatalog,
    resolved_ids: &[String],
) -> HashMap<String, f64> {
    let mut out: HashMap<String, f64> = HashMap::new();
    for id in resolved_ids {
        let Some(def) = catalog.get(id) else {
            continue;
        };
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
    out
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
    let support_static = aggregate_support_static_bonuses(cat, &resolved);
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
                research_levels: vec![],
                static_bonuses: HashMap::new(),
            },
        );
        buffs.insert(
            "b".into(),
            SupportBuffDef {
                exclusive_group: Some("g1".into()),
                priority: 2,
                research_levels: vec![],
                static_bonuses: HashMap::new(),
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
