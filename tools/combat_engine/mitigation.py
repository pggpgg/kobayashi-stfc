"""Combat mitigation model for deterministic STFC combat math.

Assumptions:
- Defense and piercing stats are treated as non-negative inputs.
- Non-positive piercing is clamped to ``EPSILON`` so the ratio remains finite and deterministic.
- For general use, final mitigation is clamped to ``[0.0, 1.0]``.
- For hostile defenders, use ``mitigation_for_hostile`` with default floor 0.16 and ceiling 0.72.
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum

EPSILON = 1e-9

# Default hostile clamp (game developers use 16% floor, 72% ceiling for most hostiles).
MITIGATION_FLOOR = 0.16
MITIGATION_CEILING = 0.72


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


def mitigation_with_mystery(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
    mystery_mitigation_factor: float = 0.0,
) -> float:
    """Raw mitigation with optional mystery factor X.

    Formula: 1 - (1 - X) * (1 - cA*fA) * (1 - cS*fS) * (1 - cD*fD).
    When X = 0 this matches the classic formula. Game developers use X rarely for some hostiles.
    No clamp applied.
    """
    c_armor, c_shield, c_dodge = SHIP_TYPE_COEFFICIENTS[ship_type]

    f_armor = component_mitigation(defender.armor, attacker.armor_piercing)
    f_shield = component_mitigation(defender.shield_deflection, attacker.shield_piercing)
    f_dodge = component_mitigation(defender.dodge, attacker.accuracy)

    one_minus_x = max(0.0, 1.0 - mystery_mitigation_factor)
    total = 1.0 - one_minus_x * (1.0 - c_armor * f_armor) * (1.0 - c_shield * f_shield) * (1.0 - c_dodge * f_dodge)
    return total


def mitigation(defender: DefenderStats, attacker: AttackerStats, ship_type: ShipType) -> float:
    """Compute total mitigation using weighted multiplicative composition. Clamped to [0, 1]."""
    total = mitigation_with_mystery(defender, attacker, ship_type, 0.0)
    return max(0.0, min(1.0, total))


def mitigation_for_hostile(
    defender: DefenderStats,
    attacker: AttackerStats,
    ship_type: ShipType,
    mystery_mitigation_factor: float = 0.0,
    floor: float = MITIGATION_FLOOR,
    ceiling: float = MITIGATION_CEILING,
) -> float:
    """Mitigation for hostile defenders: applies mystery factor then clamps to [floor, ceiling]."""
    total = mitigation_with_mystery(defender, attacker, ship_type, mystery_mitigation_factor)
    return max(floor, min(ceiling, total))


def apex_barrier_damage_factor(apex_barrier: float, apex_shred: float) -> float:
    """Fraction of damage that gets through after Apex Barrier (and Apex Shred).

    effective_barrier = apex_barrier / (1 + apex_shred)
    factor = 10000 / (10000 + effective_barrier)

    Matches stfc-toolbox game-mechanics and Rust engine.
    """
    effective_barrier = apex_barrier / max(1.0 + apex_shred, EPSILON)
    return 10000.0 / (10000.0 + effective_barrier)
