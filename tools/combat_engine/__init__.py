"""Combat engine primitives."""

from .mitigation import (
    AttackerStats,
    DefenderStats,
    ShipType,
    component_mitigation,
    mitigation,
)

__all__ = [
    "AttackerStats",
    "DefenderStats",
    "ShipType",
    "component_mitigation",
    "mitigation",
]
