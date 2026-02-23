from __future__ import annotations

import pytest

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from tools.combat_engine.mitigation import (
    AttackerStats,
    DefenderStats,
    ShipType,
    apex_barrier_damage_factor,
    component_mitigation,
    isolytic_mitigation,
    mitigation,
)


@pytest.mark.parametrize(
    ("defense", "piercing", "expected"),
    [
        (100.0, 100.0, 0.4653980386),
        (0.0, 100.0, 0.1787376092),
        (500.0, 50.0, 0.9999956181),
    ],
)
def test_component_mitigation_known_vectors(defense: float, piercing: float, expected: float) -> None:
    assert component_mitigation(defense, piercing) == pytest.approx(expected, rel=1e-3)


@pytest.mark.parametrize(
    ("ship_type", "expected"),
    [
        (ShipType.SURVEY, 0.4742034942),
        (ShipType.BATTLESHIP, 0.5822290822),
        (ShipType.EXPLORER, 0.5496950140),
        (ShipType.INTERCEPTOR, 0.3933023062),
    ],
)
def test_total_mitigation_golden_vectors(ship_type: ShipType, expected: float) -> None:
    defender = DefenderStats(armor=250.0, shield_deflection=120.0, dodge=50.0)
    attacker = AttackerStats(armor_piercing=100.0, shield_piercing=60.0, accuracy=200.0)

    assert mitigation(defender, attacker, ship_type) == pytest.approx(expected, rel=1e-3)


def test_near_zero_piercing_remains_bounded() -> None:
    defender = DefenderStats(armor=1000.0, shield_deflection=1000.0, dodge=1000.0)
    attacker = AttackerStats(armor_piercing=0.0, shield_piercing=1e-12, accuracy=0.0)

    total = mitigation(defender, attacker, ShipType.BATTLESHIP)

    assert 0.0 <= total <= 1.0
    assert total == pytest.approx(0.712, rel=1e-6)


def test_zero_defenses_produce_low_mitigation() -> None:
    defender = DefenderStats(armor=0.0, shield_deflection=0.0, dodge=0.0)
    attacker = AttackerStats(armor_piercing=100.0, shield_piercing=100.0, accuracy=100.0)

    total = mitigation(defender, attacker, ShipType.SURVEY)

    assert total == pytest.approx(0.1523922966, rel=1e-3)


@pytest.mark.parametrize(
    ("isolytic_defense", "expected"),
    [
        (0.0, 1.0),
        (1.0, 0.5),
        (4.0, 0.2),
        (-5.0, 1.0),
    ],
)
def test_isolytic_mitigation_vectors(isolytic_defense: float, expected: float) -> None:
    assert isolytic_mitigation(isolytic_defense) == pytest.approx(expected)


@pytest.mark.parametrize(
    ("apex_barrier", "apex_shred", "expected"),
    [
        (0.0, 0.0, 1.0),
        (10_000.0, 0.0, 0.5),
        (10_000.0, 1.0, 2.0 / 3.0),
        (20_000.0, 0.0, 1.0 / 3.0),
    ],
)
def test_apex_barrier_damage_factor(
    apex_barrier: float, apex_shred: float, expected: float
) -> None:
    assert apex_barrier_damage_factor(apex_barrier, apex_shred) == pytest.approx(expected, rel=1e-9)
