#!/usr/bin/env python3
"""
Set officers.canonical.json ability `slot` to `below_decks` when the ability id matches
stfc.space `below_decks_ability.id` for the same `source_officer_id`.

Upstream JSON: data/upstream/data-stfc-space/officers/<source_officer_id>.json

Usage:
  python3 scripts/sync_canonical_below_decks_slots.py           # apply + write
  python3 scripts/sync_canonical_below_decks_slots.py --dry-run # print only

After updating canonical, regenerate LCARS (from repo root):
  cargo run --bin generate_lcars --release
  mv data/officers/officers.lcars.yaml /tmp/officers.lcars.bak  # avoid merge duplicates
  cargo run --bin merge_lcars --release
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--canonical",
        type=Path,
        default=Path("data/officers/officers.canonical.json"),
        help="Path to officers.canonical.json",
    )
    parser.add_argument(
        "--upstream-dir",
        type=Path,
        default=Path("data/upstream/data-stfc-space/officers"),
        help="Directory of per-officer upstream JSON",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print changes but do not write",
    )
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[1]
    canon_path = args.canonical if args.canonical.is_absolute() else root / args.canonical
    upstream_dir = args.upstream_dir if args.upstream_dir.is_absolute() else root / args.upstream_dir

    if not canon_path.is_file():
        print(f"Missing {canon_path}", file=sys.stderr)
        return 1

    data = json.loads(canon_path.read_text())
    officers = data.get("officers")
    if not isinstance(officers, list):
        print("Expected top-level 'officers' array", file=sys.stderr)
        return 1

    changed: list[tuple[str, str, int, str]] = []

    for o in officers:
        sid = str(o.get("source_officer_id") or "").strip()
        if not sid:
            continue
        up_path = upstream_dir / f"{sid}.json"
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
            prev = ab.get("slot")
            ab["slot"] = "below_decks"
            changed.append((o.get("id", ""), o.get("name", ""), aid_int, str(prev)))

    print(f"Rows to set to below_decks: {len(changed)}")
    for row in changed:
        print(f"  {row[0]} ({row[1]}) ability_id={row[2]} was slot={row[3]!r}")

    if args.dry_run:
        return 0

    if changed:
        canon_path.write_text(json.dumps(data, indent=2) + "\n")
        print(f"Wrote {canon_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
