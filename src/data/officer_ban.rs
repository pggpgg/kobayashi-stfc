//! Centralized optimizer ban list — the single source of truth for officer opt-outs.
//!
//! Data file: `data/officers/../optimizer/officer_ban_list.csv` ([`DEFAULT_OFFICER_BAN_LIST_PATH`]),
//! one row per officer keyed by canonical `officer_id`, with six ban flags:
//! `{pve,pvp}` × `{captain, bridge, below_decks}`. A non-empty truthy cell (`x` / `1` / `true` /
//! `yes`) bans that officer from that seat in that mode during **optimization**.
//!
//! This is a **curation opt-out** — "don't waste optimization budget simulating this officer here,
//! even if its ability technically works" — and is applied on top of (overriding) the functional
//! eligibility matrix in [`crate::data::officer_eligibility::is_eligible_for_optimization`]. It is
//! optimization-only; simulation never suppresses an officer the player explicitly picked.
//!
//! Replaces the former `captain_ban_list.json` and the hard-coded `PVP_BELOW_DECKS_BANNED_SOURCE_IDS`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

pub const DEFAULT_OFFICER_BAN_LIST_PATH: &str = "data/optimizer/officer_ban_list.csv";

#[derive(Debug, Default, Clone, Copy)]
struct BanFlags {
    pve_captain: bool,
    pve_bridge: bool,
    pve_below_decks: bool,
    pvp_captain: bool,
    pvp_bridge: bool,
    pvp_below_decks: bool,
}

impl BanFlags {
    fn any(&self) -> bool {
        self.pve_captain
            || self.pve_bridge
            || self.pve_below_decks
            || self.pvp_captain
            || self.pvp_bridge
            || self.pvp_below_decks
    }
}

static BANS: OnceLock<HashMap<String, BanFlags>> = OnceLock::new();

fn truthy(cell: &str) -> bool {
    matches!(
        cell.trim().to_ascii_lowercase().as_str(),
        "x" | "1" | "true" | "yes"
    )
}

fn load() -> HashMap<String, BanFlags> {
    let mut map = HashMap::new();
    let raw = std::fs::read_to_string(crate::runtime_paths::resolve(DEFAULT_OFFICER_BAN_LIST_PATH))
        .or_else(|_| std::fs::read_to_string(Path::new(DEFAULT_OFFICER_BAN_LIST_PATH)));
    let Ok(raw) = raw else {
        return map;
    };
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(raw.as_bytes());
    let Ok(headers) = reader.headers().cloned() else {
        return map;
    };
    let col = |name: &str| headers.iter().position(|h| h.trim() == name);
    let (Some(i_id), Some(i_pvec), Some(i_pveb), Some(i_pvebd), Some(i_pvpc), Some(i_pvpb), Some(i_pvpbd)) = (
        col("officer_id"),
        col("pve_captain"),
        col("pve_bridge"),
        col("pve_below_decks"),
        col("pvp_captain"),
        col("pvp_bridge"),
        col("pvp_below_decks"),
    ) else {
        eprintln!("warning: officer_ban_list.csv missing required columns; bans disabled");
        return map;
    };
    for rec in reader.records().flatten() {
        let id = rec.get(i_id).unwrap_or("").trim();
        if id.is_empty() {
            continue;
        }
        let flags = BanFlags {
            pve_captain: truthy(rec.get(i_pvec).unwrap_or("")),
            pve_bridge: truthy(rec.get(i_pveb).unwrap_or("")),
            pve_below_decks: truthy(rec.get(i_pvebd).unwrap_or("")),
            pvp_captain: truthy(rec.get(i_pvpc).unwrap_or("")),
            pvp_bridge: truthy(rec.get(i_pvpb).unwrap_or("")),
            pvp_below_decks: truthy(rec.get(i_pvpbd).unwrap_or("")),
        };
        if flags.any() {
            map.insert(id.to_string(), flags);
        }
    }
    map
}

fn bans() -> &'static HashMap<String, BanFlags> {
    BANS.get_or_init(load)
}

/// True if `officer_id` is captain-banned in **either** mode. Used to exclude an officer from the
/// captain pool at build time across all pool builders (mode-agnostic, so it also holds for
/// pool builds without a resolved scenario). Per-mode/per-seat precision for every seat is enforced
/// by [`is_banned`] inside the eligibility predicate at generation/enforcement time.
pub fn is_captain_banned_any_mode(officer_id: &str) -> bool {
    is_banned(officer_id, "captain", false) || is_banned(officer_id, "captain", true)
}

/// True if `officer_id` (canonical id) is banned from `slot` in the given mode during optimization.
/// `slot` is the ability-slot vocabulary: `captain` | `officer` (bridge) | `below_decks`.
pub fn is_banned(officer_id: &str, slot: &str, pvp_mode: bool) -> bool {
    let Some(f) = bans().get(officer_id) else {
        return false;
    };
    let s = slot.trim();
    if pvp_mode {
        (s.eq_ignore_ascii_case("captain") && f.pvp_captain)
            || (s.eq_ignore_ascii_case("officer") && f.pvp_bridge)
            || (s.eq_ignore_ascii_case("below_decks") && f.pvp_below_decks)
    } else {
        (s.eq_ignore_ascii_case("captain") && f.pve_captain)
            || (s.eq_ignore_ascii_case("officer") && f.pve_bridge)
            || (s.eq_ignore_ascii_case("below_decks") && f.pve_below_decks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quark_captain_banned_both_modes_only_captain_seat() {
        // Quark is migrated as a captain ban in both modes (and nothing else).
        assert!(is_banned("quark-2fd57b", "captain", false));
        assert!(is_banned("quark-2fd57b", "captain", true));
        assert!(!is_banned("quark-2fd57b", "officer", false));
        assert!(!is_banned("quark-2fd57b", "below_decks", true));
    }

    #[test]
    fn unknown_officer_is_not_banned() {
        assert!(!is_banned("not-a-real-officer-id", "captain", true));
        assert!(!is_banned("not-a-real-officer-id", "below_decks", false));
    }

    #[test]
    fn truthy_cells() {
        for s in ["x", "X", " x ", "1", "true", "TRUE", "yes"] {
            assert!(truthy(s), "{s:?} should be truthy");
        }
        for s in ["", " ", "0", "false", "no", "-"] {
            assert!(!truthy(s), "{s:?} should not be truthy");
        }
    }
}
