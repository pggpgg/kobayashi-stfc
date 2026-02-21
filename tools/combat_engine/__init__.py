"""Combat engine primitives."""

from .mitigation import (
    AttackerStats,
    DefenderStats,
    ShipType,
    component_mitigation,
    isolytic_mitigation,
    mitigation,
)

from .validation import validate_mechanics

__all__ = [
    "AttackerStats",
    "DefenderStats",
    "ShipType",
    "component_mitigation",
    "isolytic_mitigation",
    "mitigation",
    "validate_mechanics",
]
