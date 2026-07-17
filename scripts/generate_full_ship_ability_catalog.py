#!/usr/bin/env python3
"""
Regenerate data/upstream/data-stfc-space/ship_ability_catalog.json (one entry per ability id).

Uses each ability row's `loca_id` when present, else the ship-level `loca_id`, with
translations-ship_buffs `ship_ability_desc`.

After classification, merges data/upstream/data-stfc-space/ship_ability_catalog_overrides.json
(entries dict) so hand-tuned rows survive regeneration. See docs/SHIP_ABILITY_COMBAT_NOOP_AUDIT.md.

Optional catalog fields (omit when false):
  condition_morale, condition_defender_burning, condition_defender_hull_breach,
  condition_opponent_faction (slug matching OpponentFactionTag serde names, e.g. klingon),
  condition_opponent_ship_class (battleship | explorer | interceptor; defender hull class in PvE)

Usage (repo root):  python3 scripts/generate_full_ship_ability_catalog.py
"""

from __future__ import annotations

import json
import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
UPSTREAM = REPO / "data/upstream/data-stfc-space"
SHIPS_DIR = UPSTREAM / "ships"
TRANS = UPSTREAM / "translations-ship_buffs.json"
OUT = UPSTREAM / "ship_ability_catalog.json"
# Optional: full entry replacement per ability id after heuristics (hand-tuned rows survive regen).
CATALOG_OVERRIDES_JSON = UPSTREAM / "ship_ability_catalog_overrides.json"

NOOP = {
    "timing": "combat_begin",
    "effect_type": "combat_noop",
    "value_is_percentage": False,
    "ignore_upstream_value_is_percentage": True,
    "value_override": 0.0,
}


def plain(txt: str) -> str:
    return re.sub(r"<[^>]+>", "", txt or "").lower()


def load_desc_by_loca() -> dict[int, str]:
    with open(TRANS, encoding="utf-8") as f:
        rows = json.load(f)
    out: dict[int, str] = {}
    for r in rows:
        if r.get("key") == "ship_ability_desc" and r.get("id") is not None:
            out[int(r["id"])] = r.get("text") or ""
    return out


def load_post_classify_overrides() -> dict[str, dict]:
    """Entries merged after classify_single_ability; keys are ability id strings."""
    if not CATALOG_OVERRIDES_JSON.is_file():
        return {}
    with open(CATALOG_OVERRIDES_JSON, encoding="utf-8") as f:
        root = json.load(f)
    raw = root.get("entries")
    if not isinstance(raw, dict):
        return {}
    out: dict[str, dict] = {}
    for k, v in raw.items():
        if isinstance(v, dict):
            try:
                out[str(int(str(k).strip()))] = v
            except (TypeError, ValueError):
                continue
    return out


def modeled(
    timing: str,
    effect_type: str,
    *,
    value_is_percentage: bool = False,
    ignore_upstream_value_is_percentage: bool = True,
    duration_rounds: int | None = None,
    condition_morale: bool = False,
    condition_defender_burning: bool = False,
    condition_defender_hull_breach: bool = False,
    condition_opponent_faction: str | None = None,
    condition_opponent_ship_class: str | None = None,
    condition_opponent_hostile_tags: list[str] | None = None,
    round_cap: int | None = None,
    values_scale_with_ship_level: bool = False,
) -> dict:
    d: dict = {
        "timing": timing,
        "effect_type": effect_type,
        "value_is_percentage": value_is_percentage,
        "ignore_upstream_value_is_percentage": ignore_upstream_value_is_percentage,
    }
    if duration_rounds is not None:
        d["duration_rounds"] = duration_rounds
    if condition_morale:
        d["condition_morale"] = True
    if condition_defender_burning:
        d["condition_defender_burning"] = True
    if condition_defender_hull_breach:
        d["condition_defender_hull_breach"] = True
    if condition_opponent_faction:
        d["condition_opponent_faction"] = condition_opponent_faction
    if condition_opponent_ship_class:
        d["condition_opponent_ship_class"] = condition_opponent_ship_class
    cap = None if duration_rounds is not None else round_cap
    if cap is not None and cap > 0:
        d["round_cap"] = int(cap)
    if values_scale_with_ship_level:
        d["values_scale_with_ship_level"] = True
    if condition_opponent_hostile_tags:
        d["condition_opponent_hostile_tags"] = list(condition_opponent_hostile_tags)
    return d


def first_n_rounds_cap(p: str) -> int | None:
    """Parse 'first 5 rounds of combat' / 'first 3 rounds' → cap N (inclusive from round 1)."""
    m = re.search(r"first\s+(\d+)\s+rounds", p)
    if not m:
        return None
    try:
        n = int(m.group(1))
    except ValueError:
        return None
    return n if n > 0 else None


def opponent_ship_class_slug(p: str) -> str | None:
    """Plain lowercased ability text → defender hull class (ShipType serde name)."""
    if not any(
        cls in p
        for cls in (
            "battleship",
            "explorer",
            "interceptor",
        )
    ):
        return None
    if not (
        "if the opponent" in p
        or "opponent's ship" in p
        or "against " in p
        or p.startswith("against ")
    ):
        return None
    if "battleship" in p:
        return "battleship"
    if "explorer" in p:
        return "explorer"
    if "interceptor" in p:
        return "interceptor"
    return None


def opponent_faction_slug_from_against_clause(p: str) -> str | None:
    """Plain lowercased ability text → catalog `condition_opponent_faction` slug (serde snake_case)."""
    # Longer / compound phrases before single tokens where needed
    if "against mirror" in p or "mirror universe" in p:
        return "mirror_universe"
    if "against cardassian" in p:
        return "cardassian"
    if "against romulan" in p:
        return "romulan"
    if "against klingon" in p:
        return "klingon"
    if "against federation" in p or "against federat" in p:
        return "federation"
    if "against borg" in p:
        return "borg"
    if "against augment" in p:
        return "augment"
    if "against dominion" in p:
        return "dominion"
    return None


def classify_single_ability(_loca: int, text: str) -> dict:
    p = plain(text)
    if not p.strip():
        return dict(NOOP)

    rc = first_n_rounds_cap(p)

    def m(timing: str, effect_type: str, **kwargs) -> dict:
        if kwargs.get("duration_rounds") is None:
            kwargs.setdefault("round_cap", rc)
        return modeled(timing, effect_type, **kwargs)

    # U.S.S. Crozier — hostile crit reduction (before economy; "diplomacy" in name)
    if "hostile" in p and "crit" in p and ("decreas" in p or "reduc" in p):
        return m(
            "combat_begin",
            "hostile_crit_damage_reduction",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
            duration_rounds=5,
        )

    # Galaxy Class — cumulative weapon damage while Morale
    if (
        "morale" in p
        and "weapon damage" in p
        and "cumulative" in p
        and ("each round" in p or "per round" in p)
    ):
        return m(
            "round_start",
            "accumulating_attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            condition_morale=True,
        )

    # U.S.S. Enterprise (TOS) — shield heal on hit while Morale (hull hits only in engine)
    if (
        "morale" in p
        and "shield" in p
        and ("heal" in p or "restor" in p)
        and ("hit" in p or "gets hit" in p)
    ):
        return m(
            "receive_damage",
            "shield_regen",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            condition_morale=True,
        )

    # U.S.S. Enterprise-E — kinetic damage + Morale + per-weapon-hit cumulative (approx: accumulating)
    if "morale" in p and "cumulative" in p and "weapon" in p and "enterprise-e" in p:
        return m(
            "round_start",
            "accumulating_attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            condition_morale=True,
        )

    # U.S.S. Enterprise-A — weapon damage on hull hit while Morale
    if (
        "morale" in p
        and ("hit" in p or "gets hit" in p)
        and "weapon damage" in p
        and "increas" in p
    ):
        return m(
            "receive_damage",
            "attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            condition_morale=True,
        )

    # Hit-taken stacking weapon damage (e.g. U.S.S. Northcutt)
    if (
        "each time" in p
        and "hit" in p
        and "cumulative" in p
        and "weapon damage" in p
    ):
        return m(
            "receive_damage",
            "attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
        )

    # Hit-taken stacking crit damage (e.g. Vor'cha) — attack mult as stand-in for crit damage
    if (
        "each time" in p
        and "hit" in p
        and "cumulative" in p
        and "critical" in p
        and "damage" in p
    ):
        return m(
            "receive_damage",
            "attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
        )

    # Economy / progression / non-ship-combat
    if any(
        k in p
        for k in (
            "mining rate",
            "mining bonus",
            "mining laser",
            "mining speed",
            "when mining",
            "resources from hostile",
            "more resources",
            "loot dropped",
            "aggregation plunder",
            "blueprint",
            "parts, materials",
            "parts and resources",
            "cost efficiency",
            "mycelium",
            "harvesting speed",
            "unlocks the cloaking",
            "radiation resistance",
            "borg cutting beam",
            "reward you get",
            "armada targets",
            "loot token",
            "encrypted intelligence",
        )
    ):
        return dict(NOOP)

    # Hull breach + crit + cumulative — proc chains not modeled (Rotarran, etc.)
    if "hull breach" in p and "critical" in p and "cumulative" in p:
        return dict(NOOP)
    if "hull breach" in p and "critical hit" in p and "every time" in p:
        return dict(NOOP)

    # D4-style: hull breach + weapon damage + every round cumulative
    if (
        "hull breach" in p
        and "every round" in p
        and "cumulative" in p
        and "weapon damage" in p
    ):
        return m(
            "round_start",
            "accumulating_attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            condition_defender_hull_breach=True,
        )

    # Krennla-style: hull breach + per-hit cumulative weapon scaling
    if (
        "hull breach" in p
        and "cumulative" in p
        and "weapon" in p
        and "hit" in p
    ):
        return m(
            "round_start",
            "accumulating_attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            condition_defender_hull_breach=True,
        )

    # Opponent burning + extra shots (Tribune, D'Deridex, …)
    if ("opponent is burning" in p or "opponent has burning" in p or "opponent's ship is burning" in p) and "shot" in p:
        return m(
            "round_start",
            "shots",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
            duration_rounds=1,
            condition_defender_burning=True,
        )

    # Scimitar / Augur-style: burning + all-weapon cumulative damage
    if ("opponent" in p and "burning" in p) and "weapon damage" in p and "cumulative" in p:
        return m(
            "round_start",
            "accumulating_attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            condition_defender_burning=True,
        )

    # I.S.S. Jellyfish — stacking damage each combat round (no Morale gate in text)
    if (
        ("every combat round" in p or "start of every combat round" in p or "each combat round" in p)
        and ("stack" in p or "stacks" in p)
        and ("damage" in p or "weapon" in p)
        and "morale" not in p
    ):
        return m(
            "round_start",
            "accumulating_attack_multiplier",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
        )

    # Corvus — pierce on receive_damage
    if "each time" in p and "hit" in p and "piercing" in p:
        return m(
            "receive_damage",
            "pierce_bonus",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
        )

    # Gorn Eviscerator — Hunt the Hunters (isolytic vs Pteran / Acrocanth / Macronyx only).
    if "gorn hunter" in p and "isolytic" in p and "increas" in p:
        return m(
            "combat_begin",
            "isolytic_damage",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
            values_scale_with_ship_level=True,
            condition_opponent_hostile_tags=["gorn_hunter"],
        )

    # Isolytic
    if "isolytic" in p and ("defense" in p or "defence" in p):
        return m(
            "combat_begin",
            "isolytic_defense",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
        )
    if "isolytic" in p and "increas" in p:
        return m(
            "combat_begin",
            "isolytic_damage",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
        )

    # Apex
    if "apex shred" in p and "increas" in p:
        return m(
            "combat_begin",
            "apex_shred",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
        )
    if "apex barrier" in p and "increas" in p:
        return m(
            "combat_begin",
            "apex_barrier",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
        )

    # Opponent hull class (Explorer / Battleship / Interceptor) — see `CombatContext::defender_ship_type`.
    slug_sc = opponent_ship_class_slug(p)
    if slug_sc:
        if "weapon damage" in p and "increas" in p:
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_ship_class=slug_sc,
            )
        if "increas" in p and "damage against" in p:
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_ship_class=slug_sc,
            )
        if "pierc" in p and "increas" in p:
            return m(
                "combat_begin",
                "pierce_bonus",
                value_is_percentage=True,
                ignore_upstream_value_is_percentage=False,
                condition_opponent_ship_class=slug_sc,
            )
        if "accuracy" in p and "increas" in p:
            return m(
                "combat_begin",
                "accuracy",
                value_is_percentage=True,
                ignore_upstream_value_is_percentage=False,
                condition_opponent_ship_class=slug_sc,
            )

    # Faction-tagged weapon damage — gated on defender faction in sim (`condition_opponent_faction`).
    if (
        "weapon damage" in p
        and "increas" in p
        and "against" in p
        and (
            "romulan" in p
            or "klingon" in p
            or "federation" in p
            or "borg" in p
            or "cardassian" in p
            or "mirror" in p
            or "augment" in p
            or "dominion" in p
        )
        and "if the opponent" not in p
    ):
        slug = opponent_faction_slug_from_against_clause(p)
        if slug:
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_faction=slug,
            )

    # Faction-tagged base damage without the literal phrase "weapon damage" (resolver uses same attack multiplier).
    if (
        "if the opponent" not in p
        and "increas" in p
        and "damage" in p
        and "if the opponent's ship is" not in p
    ):
        if "mirror universe" in p or "against mirror" in p:
            slug = opponent_faction_slug_from_against_clause(p) or "mirror_universe"
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_faction=slug,
            )
        if "borg hostiles" in p or "against borg" in p:
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_faction="borg",
            )
        if ("swarm ships" in p or "swarm hostiles" in p) and "armada" not in p:
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_faction="swarm",
            )
        if "actian" in p and "mantis" in p:
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_faction="actian",
            )
        if "xindi-aquatic" in p or ("xindi" in p and "aquatic" in p):
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_faction="xindi",
            )

    # Opponent class / tag / role (sim does not branch)
    if "if the opponent" in p or "if the opponent's ship is" in p:
        return dict(NOOP)
    if "delta quadrant" in p or "[dq]" in p:
        return dict(NOOP)
    if "when defending" in p or "when defend" in p:
        return dict(NOOP)
    if "armada" in p:
        return dict(NOOP)

    # Hostile debuffs (Sanctus shield drain, etc.)
    if "decreas" in p and "hostile" in p:
        return dict(NOOP)

    # Gladius — generic hostile damage
    if (
        "when fighting hostiles" in p
        and "increas" in p
        and "damage" in p
        and "if the opponent" not in p
        and "as long as" not in p
        and "each time" not in p
        and "decreas" not in p
        and "crit" not in p
    ):
        return m(
            "combat_begin",
            "attack_multiplier",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
        )

    if "when fighting hostile" in p and "weapon damage" in p and "increas" in p:
        if "if the opponent" in p or "as long as" in p or "each time" in p:
            return dict(NOOP)
        return m(
            "combat_begin",
            "attack_multiplier",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
        )

    # Combat start — pierce, shield bypass, weapon damage, or accuracy (folded in scenario, not a crew seat).
    if "combat start" in p or "start of combat" in p or "on combat start" in p or "at combat start" in p:
        if "accuracy" in p or "true aim" in p:
            return m(
                "combat_begin",
                "accuracy",
                value_is_percentage=True,
                ignore_upstream_value_is_percentage=False,
            )
        if "ignor" in p and "shield" in p:
            # "ignores X% of ... shields" is a shield-mitigation bypass
            # (AbilityEffect::ShieldMitigationBypassFraction, Harrison Sabotage precedent),
            # not pierce: pierce is an additive term that also helps against shieldless
            # targets, bypass scales the target's shield mitigation. `{0:#.#%}` placeholders
            # render fractions (upstream 1 → 100%); plain `{0}%` texts carry percent points.
            return m(
                "combat_begin",
                "shield_mitigation_bypass",
                value_is_percentage="%}" not in p,
                ignore_upstream_value_is_percentage=True,
                condition_opponent_hostile_tags=(
                    ["breen_warship"] if "breen warship" in p else None
                ),
            )
        if "shield piercing" in p or "armor piercing" in p:
            return m(
                "combat_begin",
                "pierce_bonus",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
            )
        if "weapon damage" in p and "increas" in p:
            return m(
                "combat_begin",
                "attack_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
            )

    # Remaining conditional / state text without a safe mapping
    if "as long as" in p or "when the shield is depleted" in p:
        return dict(NOOP)

    return dict(NOOP)


def main() -> None:
    by_loca = load_desc_by_loca()
    entries: dict[str, dict] = {}

    for path in sorted(SHIPS_DIR.glob("*.json")):
        with open(path, encoding="utf-8") as f:
            ship = json.load(f)
        for ab in ship.get("ability") or []:
            aid = ab.get("id")
            if aid is None:
                continue
            aid_str = str(int(aid))
            loca = ab.get("loca_id")
            if loca is None:
                loca = ship.get("loca_id")
            if loca is None:
                entries[aid_str] = dict(NOOP)
                continue
            text = by_loca.get(int(loca), "")
            entries[aid_str] = classify_single_ability(int(loca), text)

    for oid, row in load_post_classify_overrides().items():
        entries[oid] = row

    root = {
        "description": (
            "Maps upstream ship ability id (ships/*.json ability[].id) to Kobayashi timing/effect_type. "
            "Regenerate: python3 scripts/generate_full_ship_ability_catalog.py. "
            "Optional overrides (merged last): ship_ability_catalog_overrides.json. "
            "Fields: value_is_percentage, ignore_upstream_value_is_percentage, duration_rounds, value_override; "
            "optional condition_morale, condition_defender_burning, condition_defender_hull_breach, "
            "condition_opponent_faction, condition_opponent_ship_class, round_cap. "
            "combat_noop: catalogued only. See src/data/ship_ability_resolve.rs and docs/SHIP_ABILITY_COMBAT_NOOP_AUDIT.md."
        ),
        "entries": dict(sorted(entries.items(), key=lambda kv: int(kv[0]))),
    }
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(root, f, indent=2)
        f.write("\n")

    n_mod = sum(1 for v in entries.values() if v.get("effect_type") != "combat_noop")
    print(f"Wrote {len(entries)} ability ids to {OUT} ({n_mod} modeled, {len(entries) - n_mod} combat_noop)")


if __name__ == "__main__":
    main()
