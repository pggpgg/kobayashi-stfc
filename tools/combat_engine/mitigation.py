"""Combat mitigation model for deterministic STFC combat math.

Assumptions:
- Defense and piercing stats are treated as non-negative inputs.
- Non-positive piercing is clamped to ``EPSILON`` so the ratio remains finite and deterministic.
- Final mitigation is clamped to ``[0.0, 1.0]``.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum

EPSILON = 1e-9


class ShipType(str, Enum):
    SURVEY = "survey"
    BATTLESHIP = "battleship"
    EXPLORER = "explorer"
    INTERCEPTOR = "interceptor"


SHIP_TYPE_COEFFICIENTS: dict[ShipType, tuple[float, float, float]] = {
    ShipType.SURVEY: (0.3, 0.3, 0.3),
    ShipType.BATTLESHIP: (0.55, 0.2, 0.2),
    ShipType.EXPLORER: (0.2, 0.55, 0.2),
    ShipType.INTERCEPTOR: (0.2, 0.2, 0.55),
}


@dataclass(frozen=True)
class DefenderStats:
    armor: float
    shield_deflection: float
    dodge: float


@dataclass(frozen=True)
class AttackerStats:
    armor_piercing: float
    shield_piercing: float
    accuracy: float


def component_mitigation(defense: float, piercing: float) -> float:
    """Compute component mitigation f(x) = 1 / (1 + 4^(1.1 - x))."""
    safe_defense = max(0.0, defense)
    safe_piercing = max(EPSILON, piercing)
    x = safe_defense / safe_piercing
    return 1.0 / (1.0 + 4.0 ** (1.1 - x))


def isolytic_mitigation(isolytic_defense: float) -> float:
    """Compute the mitigated portion of isolytic damage as 1 / (1 + defense)."""
    safe_defense = max(0.0, isolytic_defense)
    return 1.0 / (1.0 + safe_defense)


def mitigation(defender: DefenderStats, attacker: AttackerStats, ship_type: ShipType) -> float:
    """Compute total mitigation using weighted multiplicative composition."""
    c_armor, c_shield, c_dodge = SHIP_TYPE_COEFFICIENTS[ship_type]

    f_armor = component_mitigation(defender.armor, attacker.armor_piercing)
    f_shield = component_mitigation(defender.shield_deflection, attacker.shield_piercing)
    f_dodge = component_mitigation(defender.dodge, attacker.accuracy)

    total = 1.0 - (1.0 - c_armor * f_armor) * (1.0 - c_shield * f_shield) * (1.0 - c_dodge * f_dodge)
    return max(0.0, min(1.0, total))
