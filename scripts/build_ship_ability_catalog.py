#!/usr/bin/env python3
"""
Merge heuristic ship-ability catalog entries into ship_ability_catalog.json.

For a **full** catalog (every upstream ability id), use:
  python3 scripts/generate_full_ship_ability_catalog.py

This script only prints/merges the small heuristic subset (legacy helper).

Usage:
  python3 scripts/build_ship_ability_catalog.py           # print suggested entries JSON
  python3 scripts/build_ship_ability_catalog.py --write # merge into catalog (keeps manual keys)
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
UPSTREAM = REPO / "data/upstream/data-stfc-space"
CATALOG_PATH = UPSTREAM / "ship_ability_catalog.json"
SHIPS_DIR = UPSTREAM / "ships"
TRANS_BUFFS = UPSTREAM / "translations-ship_buffs.json"


def plain(txt: str) -> str:
    return re.sub(r"<[^>]+>", "", txt).lower()


def load_translations() -> dict[int, str]:
    with open(TRANS_BUFFS, encoding="utf-8") as f:
        data = json.load(f)
    out: dict[int, str] = {}
    for row in data:
        if row.get("key") == "ship_ability_desc" and row.get("id") is not None:
            out[int(row["id"])] = row.get("text") or ""
    return out


def classify(loca_id: int, text: str) -> dict | None:
    p = plain(text)
    if "hostile" in p and "crit" in p and ("decreas" in p or "reduc" in p):
        return {
            "timing": "combat_begin",
            "effect_type": "hostile_crit_damage_reduction",
            "value_is_percentage": False,
            "ignore_upstream_value_is_percentage": True,
            "duration_rounds": 5,
        }
    if "each time" in p and "hit" in p and "piercing" in p:
        return {
            "timing": "receive_damage",
            "effect_type": "pierce_bonus",
            "value_is_percentage": False,
            "ignore_upstream_value_is_percentage": True,
        }
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
        return {
            "timing": "combat_begin",
            "effect_type": "attack_multiplier",
            "value_is_percentage": False,
            "ignore_upstream_value_is_percentage": True,
        }
    return None


def discover_entries() -> dict[str, dict]:
    by_loca = load_translations()
    out: dict[str, dict] = {}
    for path in sorted(SHIPS_DIR.glob("*.json")):
        with open(path, encoding="utf-8") as f:
            ship = json.load(f)
        loca = ship.get("loca_id")
        if loca is None:
            continue
        text = by_loca.get(int(loca), "")
        if not text.strip():
            continue
        row = classify(int(loca), text)
        if row is None:
            continue
        for ab in ship.get("ability") or []:
            aid = ab.get("id")
            if aid is not None:
                out[str(int(aid))] = dict(row)
            break
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="Merge into ship_ability_catalog.json")
    args = ap.parse_args()
    auto = discover_entries()
    if not args.write:
        print(json.dumps(auto, indent=2, sort_keys=True))
        return
    if not CATALOG_PATH.is_file():
        print("missing catalog", CATALOG_PATH, file=sys.stderr)
        sys.exit(1)
    with open(CATALOG_PATH, encoding="utf-8") as f:
        root = json.load(f)
    manual = root.get("entries") or {}
    merged = dict(manual)
    merged.update(auto)
    root["entries"] = merged
    with open(CATALOG_PATH, "w", encoding="utf-8") as f:
        json.dump(root, f, indent=2)
        f.write("\n")
    print(f"Wrote {len(merged)} entries to {CATALOG_PATH}")


if __name__ == "__main__":
    main()
