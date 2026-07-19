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
    value_source: str | None = None,
    negate_value: bool = False,
    condition_attacker_ship_type: str | None = None,
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
    if value_source is not None:
        d["value_source"] = value_source
    if negate_value:
        d["negate_value"] = True
    if condition_attacker_ship_type is not None:
        d["condition_attacker_ship_type"] = condition_attacker_ship_type
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
    if "hyperthermic decay" in p and "apex barrier" in p and "isolytic" not in p:
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

    # Dilithium Destabilization: chance-gated instant kill at combat start.
    # Chance lives in upstream values[0].chance (not value); catalog carries effect_type only.
    if (
        "chance" in p
        and ("start of combat" in p or "at the start of combat" in p)
        and ("instantly destroying" in p or "instantly destroy" in p)
        and ("warp core" in p or "destabiliz" in p)
    ):
        return (
            modeled(
                "combat_begin",
                "hostile_lethal_combat_begin",
                value_is_percentage=False,
                ignore_upstream_value_is_percentage=True,
            ),
            "dilithium_destabilization",
        )

    # Intraluminary: hostile applies Morale to itself for the rest of combat.
    # Duration is MAX_COMBAT_ROUNDS (100); engine sets defender_morale_rounds_remaining.
    if (
        ("combat start" in p or "start of combat" in p or "at the start of combat" in p)
        and "morale" in p
        and ("itself" in p or "this ship" in p)
    ):
        return (
            modeled(
                "combat_begin",
                "hostile_self_morale",
                value_override=1.0,
                duration_rounds=100,
            ),
            "hostile_self_morale",
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

    # ── Isolytic (value-scale conventions ground-truthed 2026-07-16, backlog #13) ──
    # C#-style % placeholders ({0:0.#%} / {1:#.#%}) render the upstream number ×100, so the
    # upstream value is already an engine-unit FRACTION regardless of the row's
    # value_is_percentage flag (68.1 renders "6810%"). Multi-stat texts reuse values[0].chance
    # as the second placeholder {1} (Double Down: value=apex barrier, chance=isolytic defense).
    if "isolytic" in p:
        # Something To Prove: damage {0:%} + apex shred of its target {1:%} (from chance).
        if re.search(r"increases isolytic damage by \{0[^}]*%\}", p) and re.search(
            r"shreds the apex barrier of its target by \{1[^}]*%\}", p
        ):
            return m(
                "combat_begin",
                "isolytic_damage",
                extra_seats=[modeled("combat_begin", "apex_shred", value_source="chance")],
                _bucket="isolytic_combat",
            )
        # Double Down: apex barrier {0} (flat, from value) + isolytic defense {1:%} (from chance)
        # + hardcoded crit-damage floor.
        if re.search(r"increases isolytic defen[cs]e by \{1[^}]*%\}", p) and re.search(
            r"apex barrier by \{0[^}]*\}", p
        ):
            extras = [modeled("combat_begin", "apex_barrier")]
            floor_m = re.search(r"critical damage cannot fall below (\d+(?:\.\d+)?)%", p)
            if floor_m:
                extras.append(
                    modeled(
                        "combat_begin",
                        "hostile_crit_damage_floor",
                        value_override=float(floor_m.group(1)) / 100.0,
                    )
                )
            return m(
                "combat_begin",
                "isolytic_defense",
                value_source="chance",
                extra_seats=extras,
                _bucket="isolytic_combat",
            )
        # Isolytic Dampeners bundles (ACAD wave-defense drones / Programmable Matter):
        # hardcoded "increases its Isolytic Defense by 1000%" + per-variant extras.
        damp_m = re.search(r"increases its isolytic defen[cs]e by (\d+(?:\.\d+)?)%", p)
        if damp_m:
            extras = []
            if "can only be damaged by isolytic damage" in p:
                extras.append(
                    modeled("combat_begin", "hostile_isolytic_vulnerability", value_override=0)
                )
            if re.search(r"increases apex barrier by \{0[^}]*\}", p):
                extras.append(modeled("combat_begin", "apex_barrier"))
            # Programmable Matter: round-start final-damage reduction {0:%} + full player
            # shield drain (modeled as forced-zero player shield mitigation, as Strike Down).
            if re.search(r"reduces the final damage done by player weapons by \{0[^}]*%\}", p):
                extras.append(modeled("combat_begin", "hostile_final_damage_reduction"))
                extras.append(
                    modeled(
                        "combat_begin",
                        "hostile_attacker_shield_mitigation_zero",
                        value_override=0,
                    )
                )
            hyper_m = re.search(
                r"applies (\d+(?:\.\d+)?)% hyperthermic decay to the player'?s hull", p
            )
            if hyper_m and ("first round" in p or "for 1 round" in p):
                extras.append(
                    modeled(
                        "round_start",
                        "hostile_hyperthermic_decay",
                        value_override=float(hyper_m.group(1)) / 100.0,
                        round_cap=1,
                    )
                )
            return m(
                "combat_begin",
                "isolytic_defense",
                value_override=float(damp_m.group(1)) / 100.0,
                extra_seats=extras or None,
                _bucket="isolytic_combat",
            )
        # Conditional self-debuffs: "this hostile's Isolytic Defense is reduced/lowered by X"
        # when fighting a specific player hull class (Mutually Assured Destruction, Burned in a
        # Fire, Assimilator Data Cube / Explorer Isolytic Vulnerability).
        red_m = re.search(
            r"isolytic defen[cs]e is (?:reduced|lowered) by (?:\{\d[^}]*%\}|(\d+(?:\.\d+)?)%)", p
        )
        if red_m:
            hull_class = next(
                (c for c in ("battleship", "explorer", "interceptor") if c in p), None
            )
            if hull_class:
                kwargs = {}
                if red_m.group(1) is not None:
                    kwargs["value_override"] = float(red_m.group(1)) / 100.0
                return m(
                    "combat_begin",
                    "isolytic_defense",
                    negate_value=True,
                    condition_attacker_ship_type=hull_class,
                    _bucket="isolytic_combat",
                    **kwargs,
                )
        # Replicated Honorguard Apex: 4-stat bundle with no numbers and no placeholders;
        # upstream value (0.01, flag=true) cannot be attributed to any single stat.
        if "honorguard apex" in p:
            return dict(NOOP), "isolytic_multi_review"
        # Take the Shot: hardcoded self cascade bonus ("increases their isolytic cascade
        # damage by 100% for 2 rounds" — rolling round-start refresh ≈ static for the fight).
        casc_m = re.search(r"isolytic cascade damage by (\d+(?:\.\d+)?)%", p)
        if casc_m:
            return m(
                "combat_begin",
                "isolytic_cascade",
                value_override=float(casc_m.group(1)) / 100.0,
                _bucket="isolytic_combat",
            )
        # Hardcoded single-value damage texts ("Isolytic Damage is increased by 100%",
        # "increases Isolytic Damage by 1500% and Apex Barrier by 20000, ...").
        hard_m = re.search(r"isolytic damage (?:is )?increased by (\d+(?:\.\d+)?)%", p) or re.search(
            r"increases isolytic damage by (\d+(?:\.\d+)?)%", p
        )
        if hard_m:
            extras = []
            ab_m = re.search(r"apex barrier by (\d[\d,]*)\b", p)
            if ab_m:
                extras.append(
                    modeled(
                        "combat_begin",
                        "apex_barrier",
                        value_override=float(ab_m.group(1).replace(",", "")),
                    )
                )
            cd_m = re.search(r"critical damage by (\d+(?:\.\d+)?)%", p)
            if cd_m:
                extras.append(
                    modeled(
                        "combat_begin",
                        "crit_damage",
                        value_override=float(cd_m.group(1)) / 100.0,
                    )
                )
            return m(
                "combat_begin",
                "isolytic_damage",
                value_override=float(hard_m.group(1)) / 100.0,
                extra_seats=extras or None,
                _bucket="isolytic_combat",
            )
        # Single %-placeholder rows: upstream value is a fraction — never divide by 100
        # (fixes the flag=true subfamily, e.g. loca 86307, previously scaled 100× too small).
        if re.search(r"isolytic damage by \{\d[^}]*%\}", p):
            extras = None
            shred_m = re.search(r"shreds the apex barrier of its target by (\d+(?:\.\d+)?)%", p)
            if shred_m:
                extras = [
                    modeled(
                        "combat_begin",
                        "apex_shred",
                        value_override=float(shred_m.group(1)) / 100.0,
                    )
                ]
            return m(
                "combat_begin",
                "isolytic_damage",
                extra_seats=extras,
                _bucket="isolytic_combat",
            )
        if re.search(r"isolytic defen[cs]e by \{\d[^}]*%\}", p):
            return m("combat_begin", "isolytic_defense", _bucket="isolytic_combat")
        # Fallback (no placeholder, no hardcoded number): keep the legacy flag-driven scale.
        # Covers Black Market Armaments / Krenim Temporal Core / Static Displacer-style
        # multi-stat texts whose per-stat values cannot be read off the text; unvalidated.
        if "defense" in p or "defence" in p:
            return m(
                "combat_begin",
                "isolytic_defense",
                value_is_percentage=True,
                ignore_upstream_value_is_percentage=False,
                _bucket="isolytic_combat",
            )
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

    # Breen Warship "Energy-Dampening Field": "directs 100% of incoming damage to its Shields
    # and regenerates 25% of its shield health at the start of each round. This cannot be
    # altered by officers, Forbidden Tech, etc." All outbound damage routes into the shield
    # pool (overflow spills to hull); the regen fraction comes from the text — the upstream
    # values[] are noise (uniform 50s) — so value_override pins it.
    if "directs 100% of incoming damage to its shields" in p and "regenerates" in p:
        regen_match = re.search(r"regenerates\s+(\d+(?:\.\d+)?)%\s+of its shield health", p)
        regen_fraction = float(regen_match.group(1)) / 100.0 if regen_match else 0.25
        return m(
            "combat_begin",
            "hostile_shield_damage_routing",
            value_override=regen_fraction,
            _bucket="shield_routing_combat",
        )

    # Plausible Deniability (S31-era hostiles): "Recovers {0:#.#%} of total SHP for the first
    # 5 rounds of combat." Fraction of MAX shield HP restored at the end of each round while
    # the round cap holds (round_cap → RoundRange gates rounds 1..=N).
    if "recovers" in p and "of total shp" in p and rc is not None:
        return m(
            "round_end",
            "shield_regen_max_fraction",
            value_is_percentage=not frac,
            ignore_upstream_value_is_percentage=frac,
            _bucket="shield_regen_combat",
        )

    # Q Trials (Q Junior's Twist): flavor dialogue with one mechanical clause per variant.
    # loca 73055 "defeat the Borg Polygon within 20 rounds" → engagement round limit (the engine
    # caps rounds_to_simulate; a still-alive hostile at the cap is a timeout loss, DESIGN.md §4.4).
    # loca 73051 is the 1v1 restriction only — no modelable single-ship mechanic (documented noop).
    if "q junior's twist" in p:
        lim = re.search(r"within\s+(\d+)\s+rounds", p)
        if lim and int(lim.group(1)) > 0:
            return m(
                "combat_begin",
                "hostile_engagement_round_limit",
                value_override=int(lim.group(1)),
                _bucket="engagement_limit_combat",
            )
        return dict(NOOP), "q_trials_flavor"

    # ── other_review triage slice (backlog item 12, 2026-07-19) ──────────────────────────

    # Exploitation / Pre-Assimilation Tactics (loca 35014-16 / 36008-10): "Increases damage
    # against {Interceptors|Battleships|Explorers} by {0:#.#%} for the first 5 rounds of
    # combat." Counter-fire damage bonus gated on the ATTACKING player's hull class;
    # {0:#.#%} → upstream value is a fraction (values[0] = 1 → +100%); rc → RoundRange 1..=5.
    ship_class_damage = re.search(
        r"increases damage against (interceptor|battleship|explorer)s by", p
    )
    if ship_class_damage and rc is not None:
        return m(
            "combat_begin",
            "attack_multiplier",
            value_is_percentage=not frac,
            ignore_upstream_value_is_percentage=frac,
            condition_attacker_ship_type=ship_class_damage.group(1),
            _bucket="ship_class_damage_combat",
        )

    # Ravager's Lance (loca 52051-56): "Freebooters have a buff of +{500|1500}% to all their
    # piercing stats." Counter-fire pierce percentage multiplier (Pen of Kahless hook,
    # ×(1+X)); upstream values are meaningful bonus multipliers matching the text (5 / 15).
    if re.search(r"buff of \+\d+% to all their piercing stats", p):
        return m(
            "combat_begin",
            "hostile_counter_pierce_multiplier",
            value_is_percentage=False,
            ignore_upstream_value_is_percentage=True,
            _bucket="pierce_combat",
        )

    # Species 8472 Energy Focused Beam (loca 55050): charges from combat start, fires after
    # N rounds destroying the opponent. hostile_lethal_end_of_round fires when
    # round_index % interval == 0, so interval N = lethal at the END of round N — kill it
    # within N-1 rounds or lose. Upstream values are all-zero; the mechanic is text-only.
    beam = re.search(r"after (\d+) rounds the beam fires", p)
    if beam and "destroy" in p:
        return m(
            "round_end",
            "hostile_lethal_end_of_round",
            round_interval=int(beam.group(1)),
            round_cap=None,
            _bucket="scheduled_lethal_combat",
        )

    # Q Trials Borg Defense Protocol α (loca 73050/73054): the Cutting Beam fires on the
    # hostile's SECOND weapon of every round, dealing lethal damage. round_interval 1 would
    # fire at the end of EVERY round including one where the attacker already destroyed the
    # hostile (hostile_lethal_end_of_round has no defender-alive gate), turning legitimate
    # round-1 kills into mutual-death losses — and weapon slot 2 already carries a flat
    # 2M x 4/round component in the hostile record. Kept noop pending a defender-alive gate
    # on the lethal hook (engine extension; see backlog item 12 leftovers).
    if "cutting beam" in p and "every round" in p and "lethal damage" in p:
        return dict(NOOP), "scheduled_lethal_review"

    # Victory Is Life (loca 47001): post-victory hull restore — no in-combat effect.
    if "fully restores hull health when victorious" in p:
        return dict(NOOP), "post_combat_flavor"

    # Shield Disruptors (loca 55049): every hostile weapon hit reduces the PLAYER's shield
    # mitigation by 10% for 1 round. Needs a new DefenderOnHitStack stat + an ungated
    # trigger — deferred engine-extension candidate (backlog item 12 leftovers); upstream
    # values are real (0.10).
    if "reduces the target ship" in p and "shield mitigation" in p:
        return dict(NOOP), "on_hit_mitigation_review"

    # Oppressive Resilience (loca 46001): stacking crit chance at the END of each round —
    # needs hostile-side accumulate mechanics (engine work); deferred.
    if "increases critical chance" in p and "at the end of each round" in p:
        return dict(NOOP), "stacking_crit_review"

    # Adapt, Overcome (loca 88101/03/05): fleet-composition (one of each hull class) +
    # station-building gate — a multi-ship requirement outside the single-ship scenario.
    if "unless the attacking player includes one of each ship type" in p:
        return dict(NOOP), "fleet_composition_gate"

    # Quantum Resonance Beam (loca 89101/03/05) / Evolutionary Assimilation (loca 89106):
    # already modeled OUT of catalog (lane B — conqueror-borg hostile tags gate the
    # quantum-beam / evo-assim instant-loss paths in src/combat/); the catalog rows stay
    # noop so the lane is not double-applied.
    if "immediately destroys any player ship that is not a borg sphere" in p:
        return dict(NOOP), "conqueror_borg_lane_b"
    if "evolutionary assimilation" in p and "officers are present" in p:
        return dict(NOOP), "conqueror_borg_lane_b"

    # Gravimetric Torpedoes (loca 51050): narrative Foreknowledge / Vi'dar Talios mechanic,
    # no in-combat scalar to model.
    if "foreknowledge" in p and "borg" in p:
        return dict(NOOP), "borg_cube_flavor"

    # Reflections (loca 69501): "On round start against players … Assimilate" — PvP-scoped
    # (the existing PvP guard keys on "enemy player", which this wording misses).
    if "against players" in p and "assimilate" in p:
        return dict(NOOP), "pvp_player_target"

    # ── end item-12 slice ─────────────────────────────────────────────────────────────────

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
