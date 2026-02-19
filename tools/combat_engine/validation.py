from __future__ import annotations

import re
from typing import Any

REQUIRED_FIELDS = ["id", "name", "faction", "group", "rarity", "slot", "abilities", "source"]

SUPPORTED_MECHANICS: dict[str, dict[str, Any]] = {
    "mitigation": {"status": "implemented", "keywords": ["mitigation", "damage_reduction"]},
    "piercing": {"status": "implemented", "keywords": ["pierce", "armor_pierce", "shield_pierce"]},
    "armor": {"status": "implemented", "keywords": ["armor"]},
    "crit": {"status": "implemented", "keywords": ["crit"]},
    "extra_attack": {"status": "implemented", "keywords": ["extra_attack", "double_shot"]},
    "burn": {"status": "partial", "keywords": ["burn", "burning", "ignite"]},
    "regen": {"status": "partial", "keywords": ["regen", "repair", "heal"]},
    "isolytic": {"status": "planned", "keywords": ["isolytic"]},
    "apex": {"status": "planned", "keywords": ["apex"]},
}

IGNORED_NON_COMBAT_TAGS = {
    "loot",
    "mining",
    "warp",
    "cargo",
    "station",
    "armada_credits",
    "protected_cargo",
}

RECOGNIZED_CONDITIONS = {
    "on_attack",
    "on_hit",
    "on_critical",
    "on_round_start",
    "on_round_end",
    "on_kill",
    "on_combat_start",
    "on_combat_end",
    "vs_faction",
    "round_range",
    "stat_below",
    "stat_above",
    "group_count",
    "has_tag",
    "on_burning",
    "on_shield_break",
    "on_hull_breach",
}


def normalize_token(value: str | None) -> str:
    if not value:
        return ""
    return re.sub(r"[^a-z0-9]+", "_", value.strip().lower()).strip("_")


def classify_mechanics(tokens: list[str]) -> set[str]:
    mechanics: set[str] = set()
    for token in tokens:
        for mechanic, meta in SUPPORTED_MECHANICS.items():
            if any(keyword in token for keyword in meta["keywords"]):
                mechanics.add(mechanic)
    return mechanics


def add_diagnostic(
    diagnostics: list[dict[str, Any]],
    *,
    severity: str,
    officer_id: str,
    message: str,
    ability_id: str | None = None,
) -> None:
    diagnostics.append(
        {
            "severity": severity,
            "officer_id": officer_id,
            "ability_id": ability_id,
            "message": message,
        }
    )


def validate_mechanics(officers: list[dict[str, Any]]) -> list[dict[str, Any]]:
    diagnostics: list[dict[str, Any]] = []
    ids_seen: set[str] = set()

    for officer in officers:
        cid = officer.get("id", "unknown")
        for field in REQUIRED_FIELDS:
            if field not in officer or officer[field] in (None, ""):
                add_diagnostic(
                    diagnostics,
                    severity="error",
                    officer_id=cid,
                    message=f"missing required field '{field}'",
                )

        if cid in ids_seen:
            add_diagnostic(diagnostics, severity="error", officer_id=cid, message="duplicate canonical id")
        ids_seen.add(cid)

        abilities = officer.get("abilities", [])
        if not abilities:
            add_diagnostic(diagnostics, severity="error", officer_id=cid, message="officer has no ability rows")
            officer["simulation_fidelity"] = {"fidelity": "unsupported", "has_partial_mechanics": False}
            continue

        highest_severity = "info"
        has_partial_mechanics = False

        for ability in abilities:
            ability_id = ability.get("ability_id")
            modifier = normalize_token(ability.get("modifier"))
            operation = normalize_token(ability.get("operation"))
            conditions = [normalize_token(v) for v in ability.get("conditions", []) if v and v.strip()]

            if not modifier:
                add_diagnostic(
                    diagnostics,
                    severity="error",
                    officer_id=cid,
                    ability_id=ability_id,
                    message="ability modifier is malformed or missing",
                )
                highest_severity = "error"
                continue

            if not operation:
                add_diagnostic(
                    diagnostics,
                    severity="error",
                    officer_id=cid,
                    ability_id=ability_id,
                    message="ability operation is malformed or missing",
                )
                highest_severity = "error"

            matched_mechanics = classify_mechanics([modifier] + conditions)

            for condition in conditions:
                if any(tag in condition for tag in IGNORED_NON_COMBAT_TAGS):
                    add_diagnostic(
                        diagnostics,
                        severity="info",
                        officer_id=cid,
                        ability_id=ability_id,
                        message=f"ignoring non-combat condition/tag '{condition}'",
                    )
                elif condition not in RECOGNIZED_CONDITIONS:
                    add_diagnostic(
                        diagnostics,
                        severity="error",
                        officer_id=cid,
                        ability_id=ability_id,
                        message=f"unrecognized condition '{condition}'",
                    )
                    highest_severity = "error"

            if not matched_mechanics:
                add_diagnostic(
                    diagnostics,
                    severity="warning",
                    officer_id=cid,
                    ability_id=ability_id,
                    message=f"recognized ability '{modifier}' has no mapped combat mechanic",
                )
                has_partial_mechanics = True
                if highest_severity != "error":
                    highest_severity = "warning"

            for mechanic in matched_mechanics:
                status = SUPPORTED_MECHANICS[mechanic]["status"]
                if status != "implemented":
                    add_diagnostic(
                        diagnostics,
                        severity="warning",
                        officer_id=cid,
                        ability_id=ability_id,
                        message=f"mechanic '{mechanic}' is {status} in simulator",
                    )
                    has_partial_mechanics = True
                    if highest_severity != "error":
                        highest_severity = "warning"

        officer["simulation_fidelity"] = {
            "fidelity": "exact" if highest_severity == "info" and not has_partial_mechanics else "partial",
            "has_partial_mechanics": has_partial_mechanics,
            "diagnostic_severity": highest_severity,
        }

    return diagnostics
