#!/usr/bin/env python3
"""
Rewrite officer catalog id strings from scientific notation to decimal integers.

- data/officers/officers.canonical.json: source_officer_id, abilities[].ability_id
- data/officers/id_registry.json: object keys (game id -> canonical_officer_id)

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


def fix_canonical(path: Path) -> None:
    with path.open() as f:
        data = json.load(f)
    for o in data.get("officers", []):
        sid = o.get("source_officer_id")
        if isinstance(sid, str) and sid:
            o["source_officer_id"] = normalize_id_string(sid)
        for a in o.get("abilities", []):
            aid = a.get("ability_id")
            if isinstance(aid, str) and aid:
                a["ability_id"] = normalize_id_string(aid)
    with path.open("w") as f:
        json.dump(data, f, indent=2)
        f.write("\n")


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
    fix_canonical(CANONICAL)
    fix_registry(REGISTRY)
    print(f"updated {CANONICAL.relative_to(REPO)}", file=sys.stderr)
    print(f"updated {REGISTRY.relative_to(REPO)}", file=sys.stderr)


if __name__ == "__main__":
    main()
