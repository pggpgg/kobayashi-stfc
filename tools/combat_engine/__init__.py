"""Combat engine primitives."""

from .mitigation import (
    PIERCE_CAP,
    AttackerStats,
    DefenderStats,
    ShipType,
    component_mitigation,
    isolytic_mitigation,
    mitigation,
    pierce_damage_through_bonus,
)

from .validation import validate_mechanics

__all__ = [
    "PIERCE_CAP",
    "AttackerStats",
    "DefenderStats",
    "ShipType",
    "component_mitigation",
    "isolytic_mitigation",
    "mitigation",
    "pierce_damage_through_bonus",
    "validate_mechanics",
]
