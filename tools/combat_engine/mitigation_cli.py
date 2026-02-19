#!/usr/bin/env python3
"""Dev CLI to print mitigation for a supplied stat block."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.combat_engine.mitigation import AttackerStats, DefenderStats, ShipType, mitigation


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Compute STFC mitigation for one stat block")
    parser.add_argument("--ship-type", choices=[s.value for s in ShipType], required=True)

    parser.add_argument("--armor", type=float, required=True)
    parser.add_argument("--shield-deflection", type=float, required=True)
    parser.add_argument("--dodge", type=float, required=True)

    parser.add_argument("--armor-piercing", type=float, required=True)
    parser.add_argument("--shield-piercing", type=float, required=True)
    parser.add_argument("--accuracy", type=float, required=True)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    defender = DefenderStats(
        armor=args.armor,
        shield_deflection=args.shield_deflection,
        dodge=args.dodge,
    )
    attacker = AttackerStats(
        armor_piercing=args.armor_piercing,
        shield_piercing=args.shield_piercing,
        accuracy=args.accuracy,
    )
    total = mitigation(defender, attacker, ShipType(args.ship_type))
    print(f"mitigation={total:.8f}")


if __name__ == "__main__":
    main()
