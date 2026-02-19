"""Effect stacking primitives using canonical composition.

Canonical formula per stack group:
    total = A * (1 + B) + C
where:
- A: sum of base contributions
- B: sum of modifier contributions
- C: sum of flat contributions
"""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum


class EffectCategory(str, Enum):
    BASE = "base"
    MODIFIER = "modifier"
    FLAT = "flat"


@dataclass(frozen=True)
class EffectContribution:
    effect_kind: str
    stat_key: str
    category: EffectCategory
    value: float


@dataclass(frozen=True)
class StackBreakdown:
    base_total: float
    modifier_total: float
    flat_total: float

    @property
    def total(self) -> float:
        return self.base_total * (1.0 + self.modifier_total) + self.flat_total


def stack_effects(effects: list[EffectContribution]) -> dict[tuple[str, str], StackBreakdown]:
    """Group effects by (effect_kind, stat_key) and compute category totals."""
    grouped: dict[tuple[str, str], dict[EffectCategory, float]] = {}

    for effect in effects:
        key = (effect.effect_kind, effect.stat_key)
        if key not in grouped:
            grouped[key] = {
                EffectCategory.BASE: 0.0,
                EffectCategory.MODIFIER: 0.0,
                EffectCategory.FLAT: 0.0,
            }
        grouped[key][effect.category] += effect.value

    return {
        key: StackBreakdown(
            base_total=totals[EffectCategory.BASE],
            modifier_total=totals[EffectCategory.MODIFIER],
            flat_total=totals[EffectCategory.FLAT],
        )
        for key, totals in grouped.items()
    }
