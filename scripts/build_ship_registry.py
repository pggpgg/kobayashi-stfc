#!/usr/bin/env python3
"""Build ship_id_registry.json from summary-ship.json and translations-ships.json.
Maps numeric data.stfc.space ship id -> canonical id, ship_name, ship_class.
Run from project root. Output: data/upstream/data-stfc-space/ship_id_registry.json
"""
import json
import re
import sys
from pathlib import Path

# hull_type from data.stfc.space: 0=Survey, 1=Explorer, 2=Battleship, 3=Interceptor
HULL_TO_CLASS = {0: "survey", 1: "explorer", 2: "battleship", 3: "interceptor"}


def name_to_canonical_id(text: str) -> str:
    """Convert display name to canonical id: U.S.S. CROZIER -> uss_crozier."""
    s = text.upper().strip()
    # Normalize common variants
    s = re.sub(r"U\.S\.S\.", "USS", s)
    s = re.sub(r"I\.S\.S\.", "ISS", s)
    s = re.sub(r"E\.C\.S\.", "ECS", s)
    s = re.sub(r"K'T'INGA", "K_T_INGA", s)
    s = re.sub(r"B'CHOR", "B_CHOR", s)
    s = re.sub(r"B'REL", "B_REL", s)
    s = re.sub(r"D'DERIDEX", "D_DERIDEX", s)
    s = re.sub(r"D'VOR", "D_VOR", s)
    s = re.sub(r"VOR'CHA", "VOR_CHA", s)
    s = re.sub(r"K'VORT", "K_VORT", s)
    s = re.sub(r"QUV'SOMPEK", "QUV_SOMPEK", s)
    s = re.sub(r"VI'DAR", "VI_DAR", s)
    s = re.sub(r"HEGH'TA", "HEGH_TA", s)
    s = re.sub(r"MOW'GA", "MOW_GA", s)
    # Replace non-alphanumeric with underscore, collapse
    s = re.sub(r"[^A-Z0-9]", "_", s)
    s = re.sub(r"_+", "_", s).strip("_")
    return s.lower() if s else "unknown"


def main() -> int:
    repo = Path(__file__).resolve().parent.parent
    summary_path = repo / "data/upstream/data-stfc-space/summary-ship.json"
    translations_path = repo / "data/upstream/data-stfc-space/translations-ships.json"
    ships_dir = repo / "data/upstream/data-stfc-space/ships"
    out_path = repo / "data/upstream/data-stfc-space/ship_id_registry.json"

    if not summary_path.exists():
        print(f"error: {summary_path} not found", file=sys.stderr)
        return 1
    if not translations_path.exists():
        print(f"error: {translations_path} not found", file=sys.stderr)
        return 1

    with open(summary_path) as f:
        summary = json.load(f)
    with open(translations_path) as f:
        translations = json.load(f)

    # Build loca_id -> ship_name (use ship_name key)
    loca_to_name: dict[int, str] = {}
    for t in translations:
        if isinstance(t, dict) and t.get("key") == "ship_name" and t.get("id") is not None:
            loca_to_name[int(t["id"])] = str(t["text"]).strip()

    # Collect numeric ids from ship files
    ship_file_ids: set[int] = set()
    if ships_dir.is_dir():
        for p in ships_dir.glob("*.json"):
            try:
                ship_file_ids.add(int(p.stem))
            except ValueError:
                pass

    entries = []
    seen_canonical: set[str] = set()
    for s in summary:
        if not isinstance(s, dict):
            continue
        numeric_id = s.get("id")
        loca_id = s.get("loca_id")
        hull_type = s.get("hull_type", 0)
        if numeric_id is None or loca_id is None:
            continue
        numeric_id = int(numeric_id)
        loca_id = int(loca_id)
        ship_name = loca_to_name.get(loca_id)
        if not ship_name:
            # Skip if we have no translation (e.g. loca_id 0)
            if numeric_id not in ship_file_ids:
                continue
            ship_name = f"Ship_{numeric_id}"
        ship_class = HULL_TO_CLASS.get(int(hull_type), "battleship")
        canonical_id = name_to_canonical_id(ship_name)
        # Deduplicate canonical ids (e.g. faction variants)
        base_id = canonical_id
        suffix = 0
        while canonical_id in seen_canonical:
            suffix += 1
            canonical_id = f"{base_id}_{suffix}"
        seen_canonical.add(canonical_id)
        entries.append({
            "numeric_id": numeric_id,
            "id": canonical_id,
            "ship_name": ship_name.upper(),
            "ship_class": ship_class,
        })

    entries.sort(key=lambda e: (e["ship_class"], e["ship_name"]))
    registry = {
        "data_version": "data-stfc-space",
        "source_note": "Generated from summary-ship.json + translations-ships.json by scripts/build_ship_registry.py",
        "ships": entries,
    }
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(registry, f, indent=2)
    print(f"Wrote {len(entries)} ships to {out_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
