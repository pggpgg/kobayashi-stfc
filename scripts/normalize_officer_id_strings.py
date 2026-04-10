#!/usr/bin/env python3
"""
Maintain the officer canonical catalog after upstream (stfc.space) refreshes.

1. **Decimal ids** — Rewrite scientific-notation strings to plain integers:
   - data/officers/officers.canonical.json: source_officer_id, abilities[].ability_id
   - data/officers/id_registry.json: object keys (game id -> canonical_officer_id)

2. **Below-decks seating** — For each officer with `source_officer_id`, if
   `data/upstream/data-stfc-space/officers/<id>.json` exists and defines
   `below_decks_ability.id`, set matching `abilities[].slot` to `below_decks`
   (fixes mistaken `officer` rows so generate_lcars emits real below_decks blocks).

   Requires cached per-officer JSON from `fetch_stfcspace` (officers entity).
   Officers without a cache file are skipped.

Safe for integer game ids representable exactly as float (STFC officer/ability ids).

Usage (from repo root):
  python3 scripts/normalize_officer_id_strings.py
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
CANONICAL = REPO / "data/officers/officers.canonical.json"
REGISTRY = REPO / "data/officers/id_registry.json"
UPSTREAM_OFFICERS_DIR = REPO / "data/upstream/data-stfc-space/officers"


def normalize_id_string(s: str) -> str:
    s = s.strip()
    if not s:
        return s
    if re.fullmatch(r"-?\d+", s):
        return s
    try:
        v = float(s)
    except ValueError:
        return s
    if not v.is_integer() or abs(v) >= 2**53:
        return s
    return str(int(v))


def sync_below_decks_slots_from_upstream(data: dict) -> int:
    """Align ability slot with stfc.space below_decks_ability.id. Returns rows updated."""
    if not UPSTREAM_OFFICERS_DIR.is_dir():
        return 0
    changed = 0
    for o in data.get("officers", []):
        sid = str(o.get("source_officer_id") or "").strip()
        if not sid:
            continue
        up_path = UPSTREAM_OFFICERS_DIR / f"{sid}.json"
        if not up_path.is_file():
            continue
        up = json.loads(up_path.read_text())
        bd = up.get("below_decks_ability")
        below_id = bd.get("id") if isinstance(bd, dict) else None
        if below_id is None:
            continue
        for ab in o.get("abilities", []):
            aid = ab.get("ability_id")
            if aid is None:
                continue
            try:
                aid_int = int(str(aid).strip())
            except ValueError:
                continue
            if aid_int != below_id:
                continue
            slot = (ab.get("slot") or "").strip().lower()
            if slot in ("below_decks", "below"):
                continue
            ab["slot"] = "below_decks"
            changed += 1
    return changed


def fix_canonical(path: Path) -> tuple[int, int]:
    """Returns (id_fields_touched_estimate, below_decks_rows_synced)."""
    with path.open() as f:
        data = json.load(f)
    touched = 0
    for o in data.get("officers", []):
        sid = o.get("source_officer_id")
        if isinstance(sid, str) and sid:
            n = normalize_id_string(sid)
            if n != sid:
                touched += 1
            o["source_officer_id"] = n
        for a in o.get("abilities", []):
            aid = a.get("ability_id")
            if isinstance(aid, str) and aid:
                n = normalize_id_string(aid)
                if n != aid:
                    touched += 1
                a["ability_id"] = n
    bd = sync_below_decks_slots_from_upstream(data)
    with path.open("w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    return (touched, bd)


def fix_registry(path: Path) -> None:
    with path.open() as f:
        raw = json.load(f)
    out: dict[str, str] = {}
    for k, v in raw.items():
        nk = normalize_id_string(k)
        if nk in out and out[nk] != v:
            raise SystemExit(f"id_registry collision: {k!r} and existing {nk!r} -> {out[nk]!r} vs {v!r}")
        out[nk] = v
    with path.open("w") as f:
        json.dump(out, f, indent=2, sort_keys=True)
        f.write("\n")


def main() -> None:
    id_changes, bd_synced = fix_canonical(CANONICAL)
    fix_registry(REGISTRY)
    print(f"updated {CANONICAL.relative_to(REPO)}", file=sys.stderr)
    print(
        f"  canonical: id string tweaks ~{id_changes}, below_decks slot sync {bd_synced}",
        file=sys.stderr,
    )
    print(f"updated {REGISTRY.relative_to(REPO)}", file=sys.stderr)


if __name__ == "__main__":
    main()
