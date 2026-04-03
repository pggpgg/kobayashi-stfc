#!/usr/bin/env python3
"""Same role as fetch_stfcspace_page_upstream.mjs — works without Node."""

from __future__ import annotations

import json
import sys
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "data" / "upstream" / "data-stfc-space"
BASE = "https://data.stfc.space"
DELAY_S = 0.15

SUMMARY_SEGMENTS: list[tuple[str, str]] = [
    ("ship", "summary-ship.json"),
    ("officer", "summary-officer.json"),
    ("building", "summary-building.json"),
    ("research", "summary-research.json"),
    ("system", "summary-system.json"),
    ("hostile", "summary-hostile.json"),
    ("consumable", "summary-consumable.json"),
    ("forbidden_tech", "summary-forbidden_tech.json"),
    ("hazards", "summary-hazards.json"),
    ("wave_defense", "summary-wave_defense.json"),
    ("pvp_bands", "summary-pvp_bands.json"),
    ("mission", "summary-mission.json"),
    ("resource", "summary-ressource.json"),
]

TRANSLATION_PATHS = [
    "materials",
    "ships",
    "officers",
    "officer_names",
    "officer_buffs",
    "officer_flavor_text",
    "traits",
    "research",
    "starbase_modules",
    "factions",
    "systems",
    "ship_components",
    "blueprints",
    "consumables",
    "mission_titles",
    "navigation",
    "ship_buffs",
    "loyalty",
    "forbidden_tech",
    "event_titles",
    "player_avatars",
    "hud",
]


def fetch_json(url: str) -> object:
    req = urllib.request.Request(
        url,
        headers={
            "Accept": "application/json",
            "User-Agent": "Kobayashi-upstream-fetch/1.0",
        },
    )
    with urllib.request.urlopen(req, timeout=120) as resp:
        if resp.status != 200:
            raise RuntimeError(f"{url} -> HTTP {resp.status}")
        return json.loads(resp.read().decode("utf-8"))


def write_json(rel: str, data: object) -> None:
    dest = OUT / rel
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(data, separators=(",", ":")) + "\n", encoding="utf-8")


def main() -> None:
    argv = sys.argv[1:]
    summaries_only = "--summaries-only" in argv
    ship_id = "2251018025"
    if "--ship-id" in argv:
        i = argv.index("--ship-id")
        if i + 1 < len(argv):
            ship_id = argv[i + 1].strip()

    jobs: list[tuple[str, str]] = []

    for seg, filename in SUMMARY_SEGMENTS:
        u = f"{BASE}/{seg}/summary.json"
        jobs.append((u, filename))

    if not summaries_only:
        lang = "en"
        for p in TRANSLATION_PATHS:
            u = f"{BASE}/translations/{lang}/{p}.json"
            jobs.append((u, f"translations-{p}.json"))
        jobs.append((f"{BASE}/ship/{ship_id}.json", f"ships/{ship_id}.json"))

    print(f"Fetching {len(jobs)} JSON resources from {BASE} …")
    for i, (url, rel) in enumerate(jobs):
        print(f"[{i + 1}/{len(jobs)}] {url}")
        write_json(rel, fetch_json(url))
        if i < len(jobs) - 1:
            time.sleep(DELAY_S)

    print(f"Done. Wrote under {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
