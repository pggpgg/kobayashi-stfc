//! Map hostile `loca_id` to English display strings from data.stfc.space translation exports
//! (`translations-ships.json`, `translations-officer_names.json`, `translations-navigation.json`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Deserialize;

const TRANSLATIONS_SHIPS: &str = "data/upstream/data-stfc-space/translations-ships.json";
const TRANSLATIONS_OFFICER_NAMES: &str =
    "data/upstream/data-stfc-space/translations-officer_names.json";
const TRANSLATIONS_NAVIGATION: &str = "data/upstream/data-stfc-space/translations-navigation.json";
const TRANSLATIONS_SYSTEMS: &str = "data/upstream/data-stfc-space/translations-systems.json";
const TRANSLATIONS_MATERIALS: &str = "data/upstream/data-stfc-space/translations-materials.json";
const SUMMARY_RESOURCES: &str = "data/upstream/data-stfc-space/summary-ressource.json";
const SUMMARY_HOSTILES: &str = "data/upstream/data-stfc-space/summary-hostile.json";

/// Compact spawn & loot metadata for `/api/hostiles` disambiguation and Loops farming resolution (#414).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostileCatalogMeta {
    pub system_names: Vec<String>,
    pub warp: u32,
    pub has_loot: bool,
    pub primary_resource_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TranslationRow {
    #[serde(default)]
    id: Option<i64>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

fn key_score(key: &str) -> u8 {
    match key {
        "marauder_name_only" => 3,
        "ship_name" => 2,
        "officer_name" => 1,
        _ => 0,
    }
}

/// Remove `<color=...>` / `</color>` tags, numeric HTML character references (e.g. `&#128128;`
/// skull markers on hazard system names), and collapse whitespace (game localization format).
pub fn strip_stfc_color_tags(s: &str) -> String {
    let mut s = s.to_string();
    while let Some(i) = s.find("<color") {
        if let Some(rel) = s[i..].find('>') {
            let end = i + rel + 1;
            s.replace_range(i..end, "");
        } else {
            break;
        }
    }
    s = s.replace("</color>", "");
    while let Some(i) = s.find("&#") {
        if let Some(rel) = s[i..].find(';') {
            let end = i + rel + 1;
            let digits = &s[i + 2..i + rel];
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                s.replace_range(i..end, " ");
                continue;
            }
        }
        break;
    }
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

/// Build loca_id → best display name from bundled upstream translation files.
pub fn load_hostile_loca_display_names(data_root: &Path) -> HashMap<u64, String> {
    let mut best: HashMap<u64, (u8, String)> = HashMap::new();
    for rel in [TRANSLATIONS_SHIPS, TRANSLATIONS_OFFICER_NAMES] {
        merge_translation_file(data_root, rel, &mut best);
    }
    // Navigation `marauder_name_only` outranks `ship_name`; use only when primary sources miss.
    let nav = load_translation_file(data_root, TRANSLATIONS_NAVIGATION);
    for (id, (_, text)) in nav {
        best.entry(id).or_insert((0, text));
    }
    best.into_iter().map(|(id, (_, t))| (id, t)).collect()
}

fn load_translation_file(data_root: &Path, rel: &str) -> HashMap<u64, (u8, String)> {
    let mut best = HashMap::new();
    merge_translation_file(data_root, rel, &mut best);
    best
}

fn merge_translation_file(data_root: &Path, rel: &str, best: &mut HashMap<u64, (u8, String)>) {
    let path = data_root.join(rel);
    if !path.is_file() {
        return;
    }
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(rows) = serde_json::from_str::<Vec<TranslationRow>>(&raw) else {
        return;
    };
    for row in rows {
        let Some(id) = row.id.filter(|&x| x >= 0) else {
            continue;
        };
        let id = id as u64;
        let key = row.key.as_deref().unwrap_or("");
        let Some(text) = row
            .text
            .as_ref()
            .map(|t| strip_stfc_color_tags(t))
            .filter(|t| !t.is_empty())
        else {
            continue;
        };
        if text.eq_ignore_ascii_case("missing description") || text == "-" {
            continue;
        }
        let sc = key_score(key);
        let replace = match best.get(&id) {
            None => true,
            Some((prev, _)) => sc > *prev,
        };
        if replace {
            best.insert(id, (sc, text));
        }
    }
}

pub fn resolve_hostile_display_name(
    map: &HashMap<u64, String>,
    loca_id: Option<u64>,
    fallback_name: &str,
) -> String {
    if let Some(id) = loca_id {
        if let Some(s) = map.get(&id) {
            return s.clone();
        }
    }
    fallback_name.to_string()
}

/// Basic common resources that should yield to specialty farming currencies (e.g. Tech Module,
/// Xindi Scraps, Actian Venom, Volatile Isomatter) when naming a hostile's primary loot drop.
pub fn is_basic_currency_name(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "parsteel" | "tritanium" | "dilithium" | "latinum"
    )
}

#[derive(Debug, Deserialize)]
struct StringOrIntTranslationRow {
    #[serde(default)]
    id: Option<serde_json::Value>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

fn json_u64(v: &serde_json::Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().filter(|&i| i >= 0).map(|i| i as u64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
}

/// Build `system_id -> display name` from `translations-systems.json` (`key == "title"`).
pub fn load_system_display_names(data_root: &Path) -> HashMap<u64, String> {
    let path = data_root.join(TRANSLATIONS_SYSTEMS);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let Ok(rows) = serde_json::from_str::<Vec<StringOrIntTranslationRow>>(&raw) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for row in rows {
        if row.key.as_deref() != Some("title") {
            continue;
        }
        let Some(id) = row.id.as_ref().and_then(json_u64) else {
            continue;
        };
        let Some(text) = row
            .text
            .as_deref()
            .map(strip_stfc_color_tags)
            .filter(|t| !t.is_empty() && t != "." && t != "-")
        else {
            continue;
        };
        out.entry(id).or_insert(text);
    }
    out
}

#[derive(Debug, Deserialize)]
struct ResourceSummaryRow {
    id: u64,
    #[serde(default)]
    loca_id: Option<u64>,
    #[serde(default)]
    resource_id: Option<String>,
}

/// Build `resource_id -> display name` from `summary-ressource.json` + `translations-materials.json`.
pub fn load_resource_display_names(data_root: &Path) -> HashMap<u64, String> {
    let mut loca_names: HashMap<u64, String> = HashMap::new();
    if let Ok(raw) = std::fs::read_to_string(data_root.join(TRANSLATIONS_MATERIALS)) {
        if let Ok(rows) = serde_json::from_str::<Vec<StringOrIntTranslationRow>>(&raw) {
            for row in rows {
                if row.key.as_deref() != Some("resource_name") {
                    continue;
                }
                let Some(id) = row.id.as_ref().and_then(json_u64) else {
                    continue;
                };
                let Some(text) = row
                    .text
                    .as_deref()
                    .map(strip_stfc_color_tags)
                    .filter(|t| !t.is_empty() && t != "-")
                else {
                    continue;
                };
                loca_names.entry(id).or_insert(text);
            }
        }
    }

    let Ok(raw) = std::fs::read_to_string(data_root.join(SUMMARY_RESOURCES)) else {
        return HashMap::new();
    };
    let Ok(rows) = serde_json::from_str::<Vec<ResourceSummaryRow>>(&raw) else {
        return HashMap::new();
    };
    let mut out = HashMap::new();
    for row in rows {
        let name = row
            .loca_id
            .and_then(|lid| loca_names.get(&lid).cloned())
            .or(row.resource_id)
            .unwrap_or_else(|| format!("Resource {}", row.id));
        out.insert(row.id, name);
    }
    out
}

/// Process-cached [`load_resource_display_names`] keyed by `data_root` so standalone scenario
/// builders and benchmarks do not re-parse `translations-materials.json` on every invocation.
pub fn load_resource_display_names_cached(data_root: &Path) -> Arc<HashMap<u64, String>> {
    type ResourceNamesCache = Mutex<HashMap<PathBuf, Arc<HashMap<u64, String>>>>;
    static CACHE: OnceLock<ResourceNamesCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = data_root.to_path_buf();
    if let Ok(guard) = cache.lock() {
        if let Some(existing) = guard.get(&key) {
            return Arc::clone(existing);
        }
    }
    let loaded = Arc::new(load_resource_display_names(data_root));
    if let Ok(mut guard) = cache.lock() {
        return Arc::clone(guard.entry(key).or_insert_with(|| Arc::clone(&loaded)));
    }
    loaded
}

#[derive(Debug, Deserialize)]
struct SummaryHostileDrop {
    resource_id: u64,
    #[serde(default)]
    min: i64,
    #[serde(default)]
    max: i64,
}

#[derive(Debug, Deserialize)]
struct SummaryHostileRow {
    id: u64,
    #[serde(default)]
    systems: Vec<u64>,
    #[serde(default)]
    warp: u32,
    #[serde(default)]
    resources: Vec<SummaryHostileDrop>,
}

/// Select the primary farming resource name from positive drops: prefers specialty loop currencies
/// over basic resources (Parsteel/Tritanium/Dilithium), breaking ties by mean drop quantity desc.
pub fn select_primary_resource_name(
    drops: &[(u64, f64)],
    resource_names: &HashMap<u64, String>,
) -> Option<String> {
    let mut best: Option<(bool, f64, u64, String)> = None;
    for &(resource_id, mean_qty) in drops {
        if !mean_qty.is_finite() || mean_qty <= 0.0 {
            continue;
        }
        let name = resource_names
            .get(&resource_id)
            .cloned()
            .unwrap_or_else(|| format!("Resource {resource_id}"));
        let is_specialty = !is_basic_currency_name(&name);
        let replace = match &best {
            None => true,
            Some((best_spec, best_qty, best_id, _)) => {
                (is_specialty, mean_qty, std::cmp::Reverse(resource_id))
                    > (*best_spec, *best_qty, std::cmp::Reverse(*best_id))
            }
        };
        if replace {
            best = Some((is_specialty, mean_qty, resource_id, name));
        }
    }
    best.map(|(_, _, _, name)| name)
}

/// Load compact spawn-system, warp, and primary-loot metadata per hostile id from `summary-hostile.json`.
pub fn load_hostile_catalog_meta(
    data_root: &Path,
    system_names: &HashMap<u64, String>,
    resource_names: &HashMap<u64, String>,
) -> HashMap<String, HostileCatalogMeta> {
    let path = data_root.join(SUMMARY_HOSTILES);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let Ok(rows) = serde_json::from_str::<Vec<SummaryHostileRow>>(&raw) else {
        return HashMap::new();
    };
    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        let mut names: Vec<String> = Vec::new();
        for sid in &row.systems {
            let label = system_names
                .get(sid)
                .cloned()
                .unwrap_or_else(|| format!("System {sid}"));
            if !names.contains(&label) {
                names.push(label);
            }
        }
        let positive_drops: Vec<(u64, f64)> = row
            .resources
            .iter()
            .filter_map(|d| {
                let lo = d.min.max(0) as f64;
                let hi = d.max.max(0) as f64;
                let mean = (lo + hi) * 0.5;
                (mean > 0.0).then_some((d.resource_id, mean))
            })
            .collect();
        let has_loot = !positive_drops.is_empty();
        let primary_resource_name = select_primary_resource_name(&positive_drops, resource_names);
        out.insert(
            row.id.to_string(),
            HostileCatalogMeta {
                system_names: names,
                warp: row.warp,
                has_loot,
                primary_resource_name,
            },
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn galor_class_loca_from_bundled_translations() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let m = load_hostile_loca_display_names(root);
        assert_eq!(m.get(&50150).map(|s| s.as_str()), Some("GALOR CLASS"));
    }

    #[test]
    fn navigation_marauder_names_resolve_for_unmapped_hostile_loca_ids() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let m = load_hostile_loca_display_names(root);
        assert_eq!(
            m.get(&60059).map(|s| s.as_str()),
            Some("Romulan Imperial Legion HAZ")
        );
        assert_eq!(
            m.get(&404).map(|s| s.as_str()),
            Some("Exchange Secure Lockup")
        );
    }

    #[test]
    fn strips_color_tags() {
        assert_eq!(
            strip_stfc_color_tags("<color=#fff>BORG</color> CUBE"),
            "BORG CUBE"
        );
        assert_eq!(
            strip_stfc_color_tags("Koitera <color=#FF8080>&#128128; </color>"),
            "Koitera"
        );
    }

    #[test]
    fn yoton_xindi_aquatic_cruiser_carries_system_warp_and_tech_module_meta() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let systems = load_system_display_names(root);
        let resources = load_resource_display_names(root);
        let meta = load_hostile_catalog_meta(root, &systems, &resources);

        assert_eq!(systems.get(&589_242_343).map(String::as_str), Some("Yoton"));
        assert_eq!(
            resources.get(&103_783_761).map(String::as_str),
            Some("Tech Module")
        );

        let yoton = meta.get("3919830203").expect("Yoton L62 cruiser meta");
        assert_eq!(yoton.system_names, vec!["Yoton".to_string()]);
        assert_eq!(yoton.warp, 1000);
        assert!(yoton.has_loot);
        assert_eq!(yoton.primary_resource_name.as_deref(), Some("Tech Module"));

        let ghost = meta.get("3294438884").expect("no-loot L62 cruiser meta");
        assert!(ghost.system_names.is_empty());
        assert_eq!(ghost.warp, 0);
        assert!(!ghost.has_loot);
        assert_eq!(ghost.primary_resource_name, None);
    }
}
