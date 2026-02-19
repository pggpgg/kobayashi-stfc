"""Combat engine primitives."""

from .mitigation import (
    AttackerStats,
    DefenderStats,
    ShipType,
    component_mitigation,
    mitigation,
)
from .stacking import (
    EffectCategory,
    EffectContribution,
    StackBreakdown,
    stack_effects,
)

__all__ = [
    "AttackerStats",
    "DefenderStats",
    "ShipType",
    "component_mitigation",
    "mitigation",
    "EffectCategory",
    "EffectContribution",
    "StackBreakdown",
    "stack_effects",
]
