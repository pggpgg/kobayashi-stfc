from __future__ import annotations

from itertools import permutations
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from tools.combat_engine.stacking import EffectCategory, EffectContribution, stack_effects


def test_additive_only_stack() -> None:
    effects = [
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.BASE, 100.0),
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.BASE, 25.0),
    ]

    result = stack_effects(effects)[("stat_modify", "weapon_damage")]

    assert result.base_total == pytest.approx(125.0)
    assert result.modifier_total == pytest.approx(0.0)
    assert result.flat_total == pytest.approx(0.0)
    assert result.total == pytest.approx(125.0)


def test_modifier_only_stack() -> None:
    effects = [
        EffectContribution("stat_modify", "crit_chance", EffectCategory.MODIFIER, 0.15),
        EffectContribution("stat_modify", "crit_chance", EffectCategory.MODIFIER, 0.05),
    ]

    result = stack_effects(effects)[("stat_modify", "crit_chance")]

    assert result.base_total == pytest.approx(0.0)
    assert result.modifier_total == pytest.approx(0.20)
    assert result.flat_total == pytest.approx(0.0)
    assert result.total == pytest.approx(0.0)


def test_mixed_stack_uses_canonical_formula() -> None:
    effects = [
        EffectContribution("stat_modify", "shield_health", EffectCategory.BASE, 1000.0),
        EffectContribution("stat_modify", "shield_health", EffectCategory.MODIFIER, 0.30),
        EffectContribution("stat_modify", "shield_health", EffectCategory.MODIFIER, 0.10),
        EffectContribution("stat_modify", "shield_health", EffectCategory.FLAT, 75.0),
    ]

    result = stack_effects(effects)[("stat_modify", "shield_health")]

    assert result.total == pytest.approx(1475.0)


def test_grouping_isolation_by_effect_kind_and_stat_key() -> None:
    effects = [
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.BASE, 200.0),
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.MODIFIER, 0.25),
        EffectContribution("proc_chance", "weapon_damage", EffectCategory.FLAT, 0.10),
        EffectContribution("stat_modify", "armor", EffectCategory.FLAT, 50.0),
    ]

    grouped = stack_effects(effects)

    assert grouped[("stat_modify", "weapon_damage")].total == pytest.approx(250.0)
    assert grouped[("proc_chance", "weapon_damage")].total == pytest.approx(0.10)
    assert grouped[("stat_modify", "armor")].total == pytest.approx(50.0)


def test_ordering_independence_within_categories() -> None:
    effects = [
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.BASE, 100.0),
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.BASE, 40.0),
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.MODIFIER, 0.20),
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.MODIFIER, 0.05),
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.FLAT, 12.0),
        EffectContribution("stat_modify", "weapon_damage", EffectCategory.FLAT, 3.0),
    ]

    expected = stack_effects(effects)[("stat_modify", "weapon_damage")]

    for perm in permutations(effects):
        got = stack_effects(list(perm))[("stat_modify", "weapon_damage")]
        assert got.base_total == pytest.approx(expected.base_total)
        assert got.modifier_total == pytest.approx(expected.modifier_total)
        assert got.flat_total == pytest.approx(expected.flat_total)
        assert got.total == pytest.approx(expected.total)
