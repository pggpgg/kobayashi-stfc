#!/usr/bin/env python3
"""
Regenerate data/upstream/data-stfc-space/hostile_ability_catalog.json (one entry per ability id).

Scans data/upstream/data-stfc-space/hostiles/*.json, dedupes by ability[].id, resolves text via
ability.loca_id → translations-ship_buffs.json (ship_ability_desc).

After classification, merges hostile_ability_catalog_overrides.json (entries dict).
See docs/HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md.

Usage (repo root):  python3 scripts/generate_full_hostile_ability_catalog.py
"""

from __future__ import annotations

import json
import re
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
UPSTREAM = REPO / "data/upstream/data-stfc-space"
HOSTILES_DIR = UPSTREAM / "hostiles"
TRANS = UPSTREAM / "translations-ship_buffs.json"
OUT = UPSTREAM / "hostile_ability_catalog.json"
CATALOG_OVERRIDES_JSON = UPSTREAM / "hostile_ability_catalog_overrides.json"
AUDIT_META_JSON = REPO / "data/upstream/data-stfc-space/hostile_ability_audit_meta.json"

NOOP = {
    "timing": "combat_begin",
    "effect_type": "combat_noop",
    "value_is_percentage": False,
    "ignore_upstream_value_is_percentage": True,
    "value_override": 0,
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
    condition_defender_hull_breach: bool = False,
    condition_defender_burning: bool = False,
    round_cap: int | None = None,
    round_interval: int | None = None,
    shots: int | None = None,
    crit_reduction_additive_points: bool = False,
    crit_debuff_stacks: bool = False,
    prevent_when_defender_assimilated: bool = False,
    value_override: float | None = None,
    weapon_index: int | None = None,
    allowed_attacker_factions: list[str] | None = None,
    allowed_attacker_ship_ids: list[str] | None = None,
    extra_seats: list[dict] | None = None,
) -> dict:
    d: dict = {
        "timing": timing,
        "effect_type": effect_type,
        "value_is_percentage": value_is_percentage,
        "ignore_upstream_value_is_percentage": ignore_upstream_value_is_percentage,
    }
    if duration_rounds is not None:
        d["duration_rounds"] = duration_rounds
    if condition_defender_hull_breach:
        d["condition_defender_hull_breach"] = True
    if condition_defender_burning:
        d["condition_defender_burning"] = True
    if round_interval is not None and round_interval > 0:
        d["round_interval"] = int(round_interval)
    if shots is not None and shots > 0:
        d["shots"] = int(shots)
    if crit_reduction_additive_points:
        d["crit_reduction_additive_points"] = True
    if crit_debuff_stacks:
        d["crit_debuff_stacks"] = True
    if prevent_when_defender_assimilated:
        d["prevent_when_defender_assimilated"] = True
    if value_override is not None:
        d["value_override"] = value_override
    if weapon_index is not None and weapon_index > 0:
        d["weapon_index"] = int(weapon_index)
    cap = None if duration_rounds is not None else round_cap
    if cap is not None and cap > 0:
        d["round_cap"] = int(cap)
    if allowed_attacker_factions:
        d["allowed_attacker_factions"] = list(allowed_attacker_factions)
    if allowed_attacker_ship_ids:
        d["allowed_attacker_ship_ids"] = list(allowed_attacker_ship_ids)
    if extra_seats:
        d["extra_seats"] = extra_seats
    return d


def first_n_rounds_cap(p: str) -> int | None:
    m = re.search(r"first\s+(\d+)\s+rounds", p)
    if not m:
        return None
    try:
        n = int(m.group(1))
    except ValueError:
        return None
    return n if n > 0 else None


def classify_hostile_ability(_loca: int, text: str) -> tuple[dict, str]:
    """Return (catalog_row, audit_bucket)."""
    p = plain(text)
    if not p.strip():
        return dict(NOOP), "empty_translation"

    rc = first_n_rounds_cap(p)
    # C#-style percent placeholder ({0:#.#%}) multiplies by 100 at render time, so the upstream
    # value is a FRACTION (0.75 → "75%"). Rows without it use percent units or hardcoded text.
    frac = re.search(r"\{0[^}]*%\}", p) is not None

    def m(timing: str, effect_type: str, **kwargs) -> tuple[dict, str]:
        if kwargs.get("duration_rounds") is None:
            kwargs.setdefault("round_cap", rc)
        bucket = kwargs.pop("_bucket", "modeled_combat")
        return modeled(timing, effect_type, **kwargs), bucket

    # Combat-start player burn/breach (Xindi Hole Puncher / Immolator). Must run BEFORE the
    # broad "enemy player" PvP short-circuit so these NPC texts are not bucketed as PvP.
    if (
        "on combat start" in p
        and "hull breach" in p
        and "enemy player" in p
        and "rest of combat" in p
    ):
        return (
            modeled(
                "combat_begin",
                "hull_breach",
                value_override=1.0,
                duration_rounds=100,  # MAX_COMBAT_ROUNDS
            ),
            "player_hull_breach_combat_start",
        )
    if (
        "on combat start" in p
        and "burning" in p
        and "enemy player" in p
        and "rest of combat" in p
    ):
        return (
            modeled(
                "combat_begin",
                "burning",
                value_override=1.0,
                duration_rounds=100,  # MAX_COMBAT_ROUNDS
            ),
            "player_burning_combat_start",
        )

    # PvP-only (default PvE path is ship vs hostile NPC). Word-boundary match so Xindi NPC
    # text ("enemy players ship") is not misclassified.
    if re.search(r"\benemy player\b", p) or re.search(r"\bopponent player\b", p):
        return dict(NOOP), "pvp_player_target"

    # Xindi round-start crit debuff (Doomed Species / Be Like Water).
    # Doomed Species + Xindi Weaponry particle beam: separate round-end instant lethal seat.
    # Be Like Water + Xindi Might text: weapon component only (9×20B), no extra lethal seat.
    if "critical hit damage" in p and "start of the round" in p and "reduces" in p:
        stacks = "can stack" in p
        value_override = 25 if "2500" in p else None
        extras = None
        if "doomed species" in p and ("xindi weaponry" in p or "particle beam" in p):
            extras = [
                modeled(
                    "round_end",
                    "hostile_lethal_end_of_round",
                    round_interval=1,
                    shots=1,
                )
            ]
        elif "denticle blade" in p and (
            "heavy artillery" in p or "5th weapon" in p
        ):
            extras = [
                modeled(
                    "combat_begin",
                    "hostile_denticle_blade_heavy_artillery",
                    value_override=0.3,
                    weapon_index=5,
                )
            ]
        return (
            modeled(
                "round_start",
                "hostile_crit_damage_reduction",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                value_override=value_override,
                duration_rounds=2,
                crit_reduction_additive_points=True,
                crit_debuff_stacks=stacks,
                extra_seats=extras,
            ),
            "xindi_crit_debuff",
        )

    # Kemocite Weaponry — Xindi group armadas: +30% weapon damage at round end when not burning.
    if "kemocite" in p or (
        "weapon damage" in p and "stacks infinitely" in p and "end of the round" in p
    ):
        return (
            modeled(
                "round_end",
                "hostile_kemocite_weaponry",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                value_override=0.3,
            ),
            "xindi_kemocite",
        )

    # Standalone scheduled lethal (No Mercy every 8th round; assimilated prevents 100%).
    if "no mercy" in p or (
        "every 8th round" in p and "assimilated" in p and "lethal" in p
    ):
        return (
            modeled(
                "round_end",
                "hostile_lethal_end_of_round",
                round_interval=8,
                shots=1,
                prevent_when_defender_assimilated=True,
            ),
            "xindi_lethal_round_end",
        )

    # Outpost / station scope (not ship-vs-hostile PvE)
    if "outpost abilities" in p or "outpost ability" in p:
        return dict(NOOP), "outpost_scope"

    # Armada / wave defense
    if "armada" in p or "wave defense" in p:
        return dict(NOOP), "armada_scope"

    # Economy / progression / intel
    if any(
        k in p
        for k in (
            "mining rate",
            "mining bonus",
            "mining laser",
            "when mining",
            "resources from hostile",
            "more resources",
            "loot dropped",
            "loot token",
            "encrypted intelligence",
            "intel",
            "blueprint",
            "reward you get",
            "radiation resistance",
            "ion storm",
            "asteroid field",
        )
    ):
        return dict(NOOP), "economy"

    # Aggregation family: multi-stat rows are hand-maintained in hostile_ability_catalog.json
    # (hyperthermic decay + mitigation inflation + offense bundles). Do not first-match-wins here.
    if "hyperthermic decay" in p and "mitigation stat" in p:
        return dict(NOOP), "aggregation_hyperthermic_manual"
    if "hyperthermic decay" in p and "apex barrier" in p:
        return dict(NOOP), "aggregation_hyperthermic_manual"
    if (
        "weapon damage" in p
        and "isolytic damage" in p
        and "critical damage" in p
        and "hyperthermic" not in p
    ):
        return dict(NOOP), "aggregation_offense_manual"

    # Psionic Assault: "deals {0:#.#%} hyperthermic decay to its hull health every round" —
    # per-round fraction of the player's max hull (same hook as the Aggregation hyperthermic seat).
    # Multi-stat bundles (isolytic / final-damage texts) keep their existing classification below.
    if (
        "hyperthermic decay" in p
        and ("every round" in p or "each round" in p)
        and frac
        and "isolytic" not in p
        and "final damage" not in p
    ):
        return (
            modeled(
                "round_start",
                "hostile_hyperthermic_decay",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
            ),
            "hyperthermic_decay_combat",
        )
    # Remaining pure-hyperthermic texts need manual review (value scale is not uniform).
    if "hyperthermic decay" in p and "isolytic" not in p and "final damage" not in p:
        return dict(NOOP), "hyperthermic_review"

    # Ruthless Pursuit family (multi-part text, hardcoded percentages):
    #   "Increases Critical Chance by 100% for the first 4 rounds." +
    #   "Increases Critical Damage by 350% at the start of each round." +
    #   "This hostile's Critical Damage cannot be reduced below 50%."
    if (
        "increases critical chance" in p
        and "for the first" in p
        and "increases critical damage" in p
        and "cannot be reduced below" in p
    ):
        chance_m = re.search(r"increases critical chance by (\d+(?:\.\d+)?)%", p)
        dmg_m = re.search(r"increases critical damage by (\d+(?:\.\d+)?)%", p)
        floor_m = re.search(r"cannot be reduced below (\d+(?:\.\d+)?)%", p)
        if chance_m and dmg_m and floor_m and rc:
            return (
                modeled(
                    "combat_begin",
                    "crit_chance",
                    value_override=float(chance_m.group(1)),
                    round_cap=rc,
                    extra_seats=[
                        modeled(
                            "combat_begin",
                            "crit_damage",
                            value_override=float(dmg_m.group(1)) / 100.0,
                        ),
                        modeled(
                            "combat_begin",
                            "hostile_crit_damage_floor",
                            value_override=float(floor_m.group(1)) / 100.0,
                        ),
                    ],
                ),
                "crit_multi_stat_modeled",
            )

    # Faction-gated lethal strikes (Tal Shiar / Mo'Kai / S31 Elite, Q Almost Omnipotent / Strike Down).
    # Gate is on hull design faction ("designed ships"), not player reputation.
    if (
        ("lethally struck" in p or "fatally struck" in p)
        and ("can engage" in p or "engaged in battle" in p)
    ):
        factions: list[str] = []
        if "federation" in p:
            factions.append("federation")
        if "klingon" in p:
            factions.append("klingon")
        if "romulan" in p:
            factions.append("romulan")
        ship_ids: list[str] = []
        if "vengeance" in p:
            ship_ids.append("uss_vengeance")
        extras: list[dict] = []
        # Text says "Critical Damage Floor of 300%" — multiplier floor 3.0 (not upstream level curve).
        if "critical damage floor" in p or "crit damage floor" in p:
            extras.append(
                modeled(
                    "combat_begin",
                    "hostile_crit_damage_floor",
                    value_is_percentage=False,
                    ignore_upstream_value_is_percentage=True,
                    value_override=3.0,
                )
            )
        if "shield mitigation to 0" in p or "shield mitigation to 0%" in p:
            extras.append(
                modeled(
                    "combat_begin",
                    "hostile_attacker_shield_mitigation_zero",
                    value_override=0,
                )
            )
        if factions or ship_ids:
            return (
                modeled(
                    "combat_begin",
                    "hostile_lethal_unless_attacker_faction",
                    value_override=0,
                    allowed_attacker_factions=factions,
                    allowed_attacker_ship_ids=ship_ids or None,
                    extra_seats=extras or None,
                ),
                "faction_gate_lethal",
            )

    # Burning applied to the player at combat start (Persistence Hunter).
    burn_m = re.search(r"applies burning for (\d+) rounds? at the start of combat", p)
    if burn_m:
        return (
            modeled(
                "combat_begin",
                "burning",
                value_override=1.0,
                duration_rounds=int(burn_m.group(1)),
            ),
            "burning_combat_start",
        )

    # Multi-stat crit rows (Critical Training: chance + damage + floor in one ability)
    if "critical hit chance" in p and "critical hit damage" in p and "critical damage floor" in p:
        return (
            modeled(
                "combat_begin",
                "crit_chance",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                value_override=1000.0,
                extra_seats=[
                    modeled(
                        "combat_begin",
                        "crit_damage",
                        value_is_percentage=False,
                        ignore_upstream_value_is_percentage=True,
                    ),
                    modeled(
                        "combat_begin",
                        "hostile_crit_damage_floor",
                        value_is_percentage=False,
                        ignore_upstream_value_is_percentage=True,
                    ),
                ],
            ),
            "crit_multi_stat_modeled",
        )

    # Crit damage floor only (Diverted Power)
    if "critical hit damage cannot fall below" in p or "crit damage cannot fall below" in p:
        return (
            modeled(
                "combat_begin",
                "hostile_crit_damage_floor",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
            ),
            "crit_floor_modeled",
        )

    # Isolytic
    if "isolytic" in p and ("defense" in p or "defence" in p):
        return m(
            "combat_begin",
            "isolytic_defense",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            _bucket="isolytic_combat",
        )
    if "isolytic" in p:
        return m(
            "combat_begin",
            "isolytic_damage",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            _bucket="isolytic_combat",
        )

    # Apex
    if "apex shred" in p and "increas" in p:
        return m(
            "combat_begin",
            "apex_shred",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
            _bucket="apex_combat",
        )
    if "apex barrier" in p:
        return m(
            "combat_begin",
            "apex_barrier",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
            _bucket="apex_combat",
        )

    # Hull breach conditional weapon damage (Imperial Starfleet Dismantlement-style)
    if "hull breach" in p and "weapon damage" in p and "increas" in p:
        if "round" in p:
            return m(
                "round_start",
                "attack_multiplier",
                value_is_percentage=True,
                ignore_upstream_value_is_percentage=False,
                duration_rounds=1,
                condition_defender_hull_breach=True,
                _bucket="weapon_damage_combat",
            )

    # Per-hit stacking counter buffs (Critical Breach / Rising Fire). Before generic
    # weapon-damage / crit branches so "every time it hits" texts are not misclassified.
    if (
        "every time it hits" in p
        and "hull breached" in p
        and "critical chance" in p
    ):
        extras = []
        if "150%" in p or "cannot fall below" in p:
            extras.append(
                modeled(
                    "combat_begin",
                    "hostile_crit_damage_floor",
                    value_is_percentage=False,
                    ignore_upstream_value_is_percentage=True,
                    value_override=1.5,
                )
            )
        return (
            modeled(
                "combat_begin",
                "defender_on_hit_crit_chance_stack",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                duration_rounds=2,
                extra_seats=extras or None,
            ),
            "defender_on_hit_stack",
        )
    if (
        "every time it hits" in p
        and "burning" in p
        and "standard damage" in p
    ):
        return (
            modeled(
                "combat_begin",
                "defender_on_hit_weapon_damage_stack",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                duration_rounds=2,
            ),
            "defender_on_hit_stack",
        )

    # Combat-start or first-N-rounds crit chance / damage (single-stat,
    # e.g. Revolutionary Spirit "Increases Critical Hit Damage by X% for the first 5 rounds")
    if (
        "combat start" in p
        or "start of combat" in p
        or "at the start of combat" in p
        or (rc is not None and "increas" in p)
    ) and "critical" in p:
        if "chance" in p and "damage" not in p:
            return m(
                "combat_begin",
                "crit_chance",
                value_is_percentage=not frac,
                ignore_upstream_value_is_percentage=frac,
                _bucket="crit_combat",
            )
        if "damage" in p and "floor" not in p:
            return m(
                "combat_begin",
                "crit_damage",
                value_is_percentage=not frac,
                ignore_upstream_value_is_percentage=frac,
                _bucket="crit_combat",
            )

    # Pierce at combat start or for the first N rounds (Pen of Kahless: shield piercing,
    # armor piercing, and accuracy collapse to the engine's uniform pierce stack).
    if (
        "combat start" in p
        or "start of combat" in p
        or (rc is not None and "increas" in p)
    ) and "pierc" in p:
        if frac:
            # Fraction rows are percentage *increases* of the hostile's pierce stats —
            # flat pierce_bonus would be a noop against absolute pierce; use the
            # counter-pierce multiplier hook instead.
            return m(
                "combat_begin",
                "hostile_counter_pierce_multiplier",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                _bucket="pierce_combat",
            )
        return m(
            "combat_begin",
            "pierce_bonus",
            value_is_percentage=True,
            ignore_upstream_value_is_percentage=False,
            _bucket="pierce_combat",
        )

    # Weapon damage at combat start / round start
    if "weapon damage" in p and "increas" in p:
        timing = "round_start" if "start of the round" in p or "at the start of the round" in p else "combat_begin"
        return m(
            timing,
            "attack_multiplier",
            value_is_percentage=not frac,
            ignore_upstream_value_is_percentage=frac,
            _bucket="weapon_damage_combat",
        )

    # Generic combat-start damage increase
    if (
        ("combat start" in p or "start of combat" in p or "at the start of combat" in p)
        and "increas" in p
        and "damage" in p
    ):
        return m(
            "combat_begin",
            "attack_multiplier",
            value_is_percentage=not frac,
            ignore_upstream_value_is_percentage=frac,
            _bucket="weapon_damage_combat",
        )

    # Hostile counter-fire ignores player shields (Xindi Strength of the Ibix, Blade's Tip, …).
    if "ignores player shields" in p or (
        "completely ignores" in p and "player shields" in p
    ):
        return (
            modeled(
                "combat_begin",
                "shield_mitigation_bypass",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
                value_override=1,
            ),
            "hostile_shield_bypass",
        )

    # Shield-related (drain, restore) — defer unless clear pattern
    if "shield" in p and ("drain" in p or "decreas" in p):
        return dict(NOOP), "shield_combat_review"

    # Defense stats
    if any(k in p for k in ("dodge", "armor", "mitigation", "deflect")) and "increas" in p:
        return dict(NOOP), "defense_combat_review"

    return dict(NOOP), "other_review"


def collect_upstream_abilities() -> dict[str, dict]:
    """ability id -> metadata from upstream hostiles."""
    meta: dict[str, dict] = {}
    for path in sorted(HOSTILES_DIR.glob("*.json")):
        with open(path, encoding="utf-8") as f:
            hostile = json.load(f)
        hid = str(hostile.get("id", path.stem))
        for ab in hostile.get("ability") or []:
            aid = ab.get("id")
            if aid is None:
                continue
            aid_str = str(int(aid))
            row = meta.setdefault(
                aid_str,
                {
                    "loca_id": ab.get("loca_id"),
                    "hostile_count": 0,
                    "sample_hostile_id": hid,
                    "sample_level": hostile.get("level"),
                    "value_is_percentage": ab.get("value_is_percentage"),
                    "first_value": None,
                    "first_chance": None,
                },
            )
            row["hostile_count"] += 1
            values = ab.get("values") or []
            if values and row["first_value"] is None:
                first = values[0]
                row["first_value"] = first.get("value")
                row["first_chance"] = first.get("chance")
            if row.get("loca_id") is None and ab.get("loca_id") is not None:
                row["loca_id"] = ab.get("loca_id")
    return meta


def main() -> None:
    by_loca = load_desc_by_loca()
    upstream = collect_upstream_abilities()
    entries: dict[str, dict] = {}
    audit_meta: dict[str, dict] = {}

    for aid_str, info in sorted(upstream.items(), key=lambda kv: int(kv[0])):
        loca = info.get("loca_id")
        text = by_loca.get(int(loca), "") if loca is not None else ""
        row, bucket = classify_hostile_ability(int(loca or 0), text)
        entries[aid_str] = row
        audit_meta[aid_str] = {
            "bucket": bucket,
            "loca_id": loca,
            "hostile_count": info["hostile_count"],
            "sample_hostile_id": info["sample_hostile_id"],
            "sample_level": info.get("sample_level"),
            "text_snippet": plain(text)[:240],
            "effect_type": row.get("effect_type"),
        }

    for oid, row in load_post_classify_overrides().items():
        entries[oid] = row
        if oid in audit_meta:
            audit_meta[oid]["effect_type"] = row.get("effect_type")
            audit_meta[oid]["overridden"] = True

    root = {
        "description": (
            "Maps upstream hostile ability id (hostiles/*.json ability[].id) to Kobayashi timing/effect_type. "
            "Regenerate: python3 scripts/generate_full_hostile_ability_catalog.py. "
            "Optional overrides (merged last): hostile_ability_catalog_overrides.json. "
            "Unmapped ids are ignored at runtime; combat_noop rows are catalogued only. "
            "See src/data/hostile_ability_resolve.rs and docs/HOSTILE_ABILITY_COMBAT_NOOP_AUDIT.md."
        ),
        "entries": dict(sorted(entries.items(), key=lambda kv: int(kv[0]))),
    }
    with open(OUT, "w", encoding="utf-8") as f:
        json.dump(root, f, indent=2)
        f.write("\n")

    with open(AUDIT_META_JSON, "w", encoding="utf-8") as f:
        json.dump(
            {
                "generated_from": "scripts/generate_full_hostile_ability_catalog.py",
                "unique_ability_ids": len(entries),
                "hostiles_with_abilities_scanned": sum(
                    1 for p in HOSTILES_DIR.glob("*.json") if json.loads(p.read_text()).get("ability")
                ),
                "entries": audit_meta,
            },
            f,
            indent=2,
        )
        f.write("\n")

    n_mod = sum(1 for v in entries.values() if v.get("effect_type") != "combat_noop")
    print(
        f"Wrote {len(entries)} hostile ability ids to {OUT} "
        f"({n_mod} modeled, {len(entries) - n_mod} combat_noop)"
    )
    print(f"Audit metadata: {AUDIT_META_JSON}")


if __name__ == "__main__":
    main()
