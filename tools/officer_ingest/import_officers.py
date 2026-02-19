#!/usr/bin/env python3
"""Import officers from the STFC cheat sheet XLSX into canonical JSON."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import unicodedata
import xml.etree.ElementTree as ET
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

NS = {
    "main": "http://schemas.openxmlformats.org/spreadsheetml/2006/main",
    "rel": "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
}

RAW_OFFICERS_SHEET = "RawOfficers"
SOURCING_SHEET = "Sourcing Guide"

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


@dataclass(frozen=True)
class CanonicalAbility:
    slot: str
    modifier: str
    trigger: str
    target: str
    operation: str
    conditions: list[str]
    attributes: str
    description: str
    chance_by_rank: list[float]
    value_by_rank: list[float]


def slugify(value: str) -> str:
    normalized = unicodedata.normalize("NFKD", value)
    ascii_only = normalized.encode("ascii", "ignore").decode("ascii")
    ascii_only = re.sub(r"[^a-zA-Z0-9]+", "-", ascii_only.lower()).strip("-")
    return re.sub(r"-+", "-", ascii_only)


class XLSXReader:
    def __init__(self, path: Path):
        self.path = path
        self.archive = zipfile.ZipFile(path)
        self.shared_strings = self._load_shared_strings()
        self.sheet_paths = self._load_sheet_paths()

    def _load_shared_strings(self) -> list[str]:
        shared_path = "xl/sharedStrings.xml"
        if shared_path not in self.archive.namelist():
            return []

        root = ET.fromstring(self.archive.read(shared_path))
        values: list[str] = []
        for si in root.findall("main:si", NS):
            text = "".join(t.text or "" for t in si.iterfind(".//main:t", NS))
            values.append(text)
        return values

    def _load_sheet_paths(self) -> dict[str, str]:
        workbook = ET.fromstring(self.archive.read("xl/workbook.xml"))
        rels = ET.fromstring(self.archive.read("xl/_rels/workbook.xml.rels"))

        rel_map = {rel.attrib["Id"]: "xl/" + rel.attrib["Target"] for rel in rels}
        path_map: dict[str, str] = {}
        for sheet in workbook.find("main:sheets", NS):
            name = sheet.attrib["name"]
            rel_id = sheet.attrib[f"{{{NS['rel']}}}id"]
            path_map[name] = rel_map[rel_id]
        return path_map

    def read_sheet(self, sheet_name: str) -> list[dict[str, str]]:
        path = self.sheet_paths[sheet_name]
        root = ET.fromstring(self.archive.read(path))
        rows = []
        for row in root.findall(".//main:sheetData/main:row", NS):
            row_data: dict[str, str] = {}
            for cell in row.findall("main:c", NS):
                ref = cell.attrib["r"]
                col = "".join(c for c in ref if c.isalpha())
                value_node = cell.find("main:v", NS)
                if value_node is None:
                    continue
                value = value_node.text or ""
                if cell.attrib.get("t") == "s":
                    value = self.shared_strings[int(value)]
                row_data[col] = value
            rows.append(row_data)
        return rows


def parse_float(value: str | None) -> float | None:
    if value is None:
        return None
    if not value.strip():
        return None
    try:
        return float(value)
    except ValueError:
        return None


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


def canonical_name(raw_name: str, alias_map: dict[str, str], sourcing_names: dict[str, dict[str, str]]) -> str:
    normalized_key = raw_name.strip().upper()
    if normalized_key in alias_map:
        return alias_map[normalized_key]
    if normalized_key in sourcing_names:
        return sourcing_names[normalized_key]["name"]
    return raw_name.strip().title()


def load_alias_map(path: Path) -> dict[str, str]:
    if not path.exists():
        return {}
    return json.loads(path.read_text())


def persist_alias_map(path: Path, discovered: dict[str, str]) -> None:
    existing = load_alias_map(path)
    merged = {**existing}
    for key in sorted(discovered):
        merged.setdefault(key, discovered[key])
    path.write_text(json.dumps(merged, indent=2, sort_keys=True) + "\n")


def build_sourcing_map(reader: XLSXReader) -> dict[str, dict[str, str]]:
    rows = reader.read_sheet(SOURCING_SHEET)
    sourcing: dict[str, dict[str, str]] = {}
    for row in rows:
        raw_name = row.get("D", "").strip()
        if not raw_name or raw_name == "Officer Name":
            continue

        key = raw_name.upper()
        sourcing[key] = {
            "name": raw_name,
            "faction": row.get("F", "Unknown").strip() or "Unknown",
            "slot": row.get("G", "Unknown").strip() or "Unknown",
            "rarity": row.get("E", "Unknown").strip() or "Unknown",
        }
    return sourcing


def officer_id(raw_name: str, source_officer_id: str, registry: dict[str, str]) -> str:
    if source_officer_id in registry:
        return registry[source_officer_id]

    base = slugify(raw_name)
    disambiguator = hashlib.sha1(source_officer_id.encode()).hexdigest()[:6]
    deterministic = f"{base}-{disambiguator}"
    registry[source_officer_id] = deterministic
    return deterministic


def transform(source_path: Path, output_path: Path, alias_path: Path, registry_path: Path, report_path: Path) -> None:
    reader = XLSXReader(source_path)
    rows = reader.read_sheet(RAW_OFFICERS_SHEET)
    sourcing = build_sourcing_map(reader)

    alias_map = load_alias_map(alias_path)
    discovered_aliases: dict[str, str] = {}

    id_registry = json.loads(registry_path.read_text()) if registry_path.exists() else {}

    officers_by_source: dict[str, dict[str, Any]] = {}
    invalid_rows: list[dict[str, Any]] = []

    for index, row in enumerate(rows, start=1):
        raw_name = row.get("B", "").strip()
        source_officer_id = row.get("AT", "").strip()
        ability_id = row.get("AS", "").strip()
        if not raw_name or raw_name == "OfficerName" or not source_officer_id:
            continue

        canonical = canonical_name(raw_name, alias_map, sourcing)
        discovered_aliases[raw_name.upper()] = canonical

        source_meta = sourcing.get(raw_name.upper(), {})
        faction = source_meta.get("faction", "Unknown")
        rarity = row.get("AV", "").strip() or source_meta.get("rarity", "Unknown")
        slot = source_meta.get("slot", "Unknown")
        group = row.get("D", "").strip() or "Unspecified"

        chance_by_rank = [parse_float(row.get(col)) for col in ["AZ", "BA", "BB", "BC", "BD"]]
        value_by_rank = [parse_float(row.get(col)) for col in ["BE", "BF", "BG", "BH", "BI"]]

        chance_clean = [v for v in chance_by_rank if v is not None]
        value_clean = [v for v in value_by_rank if v is not None]

        ability = CanonicalAbility(
            slot="captain" if row.get("C", "").strip() == "CM" else "officer",
            modifier=row.get("E", "").strip(),
            trigger=row.get("G", "").strip(),
            target=row.get("H", "").strip(),
            operation=row.get("I", "").strip(),
            conditions=[c for c in row.get("F", "").split(",") if c.strip()],
            attributes=row.get("J", "").strip(),
            description=row.get("K", "").strip(),
            chance_by_rank=chance_clean,
            value_by_rank=value_clean,
        )

        if not ability.modifier:
            invalid_rows.append({"row_index": index, "reason": "missing ability modifier", "source": row})
            continue

        entry = officers_by_source.get(source_officer_id)
        if entry is None:
            cid = officer_id(canonical, source_officer_id, id_registry)
            entry = {
                "id": cid,
                "name": canonical,
                "faction": faction,
                "group": group,
                "rarity": rarity.lower(),
                "slot": slot.lower(),
                "source_officer_id": source_officer_id,
                "abilities": [],
                "scaling": {
                    "rank_chance_model": "ability.chance_by_rank",
                    "rank_value_model": "ability.value_by_rank",
                    "tier_model": "unknown",
                },
                "source": {
                    "workbook": source_path.name,
                    "sheet": RAW_OFFICERS_SHEET,
                },
            }
            officers_by_source[source_officer_id] = entry

        entry["abilities"].append({
            "ability_id": ability_id,
            "slot": ability.slot,
            "modifier": ability.modifier,
            "trigger": ability.trigger,
            "target": ability.target,
            "operation": ability.operation,
            "conditions": ability.conditions,
            "attributes": ability.attributes,
            "description": ability.description,
            "chance_by_rank": ability.chance_by_rank,
            "value_by_rank": ability.value_by_rank,
        })

    persist_alias_map(alias_path, discovered_aliases)
    registry_path.write_text(json.dumps(id_registry, indent=2, sort_keys=True) + "\n")

    officers = sorted(officers_by_source.values(), key=lambda item: item["id"])

    validation_diagnostics = validate(officers)
    invalid_rows.extend(
        {
            "id": diag.get("officer_id"),
            "ability_id": diag.get("ability_id"),
            "severity": diag.get("severity"),
            "reason": diag.get("message"),
        }
        for diag in validation_diagnostics
        if diag["severity"] == "error"
    )

    workbook_hash = hashlib.sha256(source_path.read_bytes()).hexdigest()
    data_version = f"m86-{workbook_hash[:12]}"

    payload = {
        "data_version": data_version,
        "imported_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "source_fingerprint": {
            "file": source_path.name,
            "sha256": workbook_hash,
        },
        "schema_version": 1,
        "officers": officers,
    }

    previous: dict[str, dict[str, Any]] = {}
    if output_path.exists():
        previous_data = json.loads(output_path.read_text())
        previous = {o["source_officer_id"]: o for o in previous_data.get("officers", [])}

    output_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")

    new_rows = 0
    updated_rows = 0
    unchanged_rows = 0
    for officer in officers:
        prior = previous.get(officer["source_officer_id"])
        if prior is None:
            new_rows += 1
        elif prior != officer:
            updated_rows += 1
        else:
            unchanged_rows += 1

    report = {
        "summary": {
            "total_records": len(officers),
            "new": new_rows,
            "updated": updated_rows,
            "unchanged": unchanged_rows,
            "invalid": len(invalid_rows),
            "warnings": len([diag for diag in validation_diagnostics if diag["severity"] == "warning"]),
            "info": len([diag for diag in validation_diagnostics if diag["severity"] == "info"]),
        },
        "invalid_rows": invalid_rows,
        "validation_diagnostics": validation_diagnostics,
    }
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")

    if invalid_rows:
        raise SystemExit(f"import completed with {len(invalid_rows)} invalid rows; see {report_path}")


def validate(officers: list[dict[str, Any]]) -> list[dict[str, Any]]:
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
            conditions = [normalize_token(value) for value in ability.get("conditions", []) if value and value.strip()]

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

            all_tokens = [modifier] + conditions
            matched_mechanics = classify_mechanics(all_tokens)

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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=Path("Copy of STFC Cheat Sheet - M86 (1.4RC).xlsx"))
    parser.add_argument("--output", type=Path, default=Path("data/officers/officers.canonical.json"))
    parser.add_argument("--alias", type=Path, default=Path("data/officers/name_aliases.json"))
    parser.add_argument("--id-registry", type=Path, default=Path("data/officers/id_registry.json"))
    parser.add_argument("--report", type=Path, default=Path("data/officers/import_report.json"))
    args = parser.parse_args()

    transform(args.source, args.output, args.alias, args.id_registry, args.report)


if __name__ == "__main__":
    main()
