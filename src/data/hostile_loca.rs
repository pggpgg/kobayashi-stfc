//! Map hostile `loca_id` to English display strings from data.stfc.space translation exports
//! (`translations-ships.json`, `translations-officer_names.json`).

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

const TRANSLATIONS_SHIPS: &str = "data/upstream/data-stfc-space/translations-ships.json";
const TRANSLATIONS_OFFICER_NAMES: &str = "data/upstream/data-stfc-space/translations-officer_names.json";

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

/// Remove `<color=...>` / `</color>` tags and collapse whitespace (game localization format).
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
    s.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

/// Build loca_id → best display name from bundled upstream translation files.
pub fn load_hostile_loca_display_names(data_root: &Path) -> HashMap<u64, String> {
    let mut best: HashMap<u64, (u8, String)> = HashMap::new();
    for rel in [TRANSLATIONS_SHIPS, TRANSLATIONS_OFFICER_NAMES] {
        let path = data_root.join(rel);
        if !path.is_file() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(rows) = serde_json::from_str::<Vec<TranslationRow>>(&raw) else {
            continue;
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
    best.into_iter().map(|(id, (_, t))| (id, t)).collect()
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
    fn strips_color_tags() {
        assert_eq!(
            strip_stfc_color_tags("<color=#fff>BORG</color> CUBE"),
            "BORG CUBE"
        );
    }
}
