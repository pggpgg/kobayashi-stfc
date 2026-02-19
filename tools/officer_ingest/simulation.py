from __future__ import annotations

import json
import random
from dataclasses import asdict, dataclass
from typing import Any, Literal


@dataclass(frozen=True)
class EventSource:
    officer_id: str | None = None
    ship_ability_id: str | None = None
    hostile_ability_id: str | None = None
    player_bonus_source: str | None = None


@dataclass(frozen=True)
class CombatEvent:
    event_type: str
    round_index: int
    phase: str
    source: EventSource
    values: dict[str, Any]


@dataclass(frozen=True)
class Combatant:
    id: str
    attack: float
    mitigation: float
    pierce: float
    crit_chance: float
    crit_multiplier: float
    proc_chance: float
    proc_multiplier: float
    end_of_round_damage: float


@dataclass(frozen=True)
class SimulationConfig:
    rounds: int = 3
    seed: int = 7
    trace_mode: Literal["off", "events"] = "off"


@dataclass(frozen=True)
class SimulationResult:
    total_damage: float
    events: list[CombatEvent]


class TraceCollector:
    def __init__(self, enabled: bool) -> None:
        self.enabled = enabled
        self._events: list[CombatEvent] = []

    def record(self, event: CombatEvent) -> None:
        if self.enabled:
            self._events.append(event)

    @property
    def events(self) -> list[CombatEvent]:
        return self._events


def _serialize_source(source: EventSource) -> dict[str, str]:
    return {k: v for k, v in asdict(source).items() if v is not None}


def serialize_events_json(events: list[CombatEvent]) -> str:
    payload = [
        {
            "event_type": event.event_type,
            "round_index": event.round_index,
            "phase": event.phase,
            "source": _serialize_source(event.source),
            "values": event.values,
        }
        for event in events
    ]
    return json.dumps(payload, indent=2, sort_keys=True)


def format_events_human(events: list[CombatEvent]) -> str:
    lines = []
    for event in events:
        source = ", ".join(f"{k}={v}" for k, v in _serialize_source(event.source).items()) or "source=n/a"
        values = ", ".join(f"{k}={v}" for k, v in event.values.items())
        lines.append(f"R{event.round_index:02d} [{event.phase}] {event.event_type} ({source}) -> {values}")
    return "\n".join(lines)


def simulate_combat(attacker: Combatant, defender: Combatant, config: SimulationConfig) -> SimulationResult:
    rng = random.Random(config.seed)
    trace = TraceCollector(enabled=config.trace_mode == "events")
    total_damage = 0.0

    for round_index in range(1, config.rounds + 1):
        trace.record(
            CombatEvent(
                event_type="round_start",
                round_index=round_index,
                phase="round",
                source=EventSource(ship_ability_id="baseline_round"),
                values={"attacker": attacker.id, "defender": defender.id},
            )
        )

        roll = rng.random()
        trace.record(
            CombatEvent(
                event_type="attack_roll",
                round_index=round_index,
                phase="attack",
                source=EventSource(officer_id=attacker.id),
                values={"roll": round(roll, 6), "base_attack": attacker.attack},
            )
        )

        mitigation_multiplier = max(0.0, 1.0 - defender.mitigation)
        trace.record(
            CombatEvent(
                event_type="mitigation_calc",
                round_index=round_index,
                phase="defense",
                source=EventSource(hostile_ability_id=f"{defender.id}_mitigation"),
                values={"mitigation": defender.mitigation, "multiplier": round(mitigation_multiplier, 6)},
            )
        )

        effective_mitigation = max(0.0, mitigation_multiplier + attacker.pierce)
        trace.record(
            CombatEvent(
                event_type="pierce_calc",
                round_index=round_index,
                phase="attack",
                source=EventSource(officer_id=attacker.id, player_bonus_source="research:weapon_tech"),
                values={"pierce": attacker.pierce, "effective_mitigation": round(effective_mitigation, 6)},
            )
        )

        crit_roll = rng.random()
        is_crit = crit_roll < attacker.crit_chance
        crit_multiplier = attacker.crit_multiplier if is_crit else 1.0
        trace.record(
            CombatEvent(
                event_type="crit_resolution",
                round_index=round_index,
                phase="attack",
                source=EventSource(officer_id=attacker.id, ship_ability_id="crit_matrix"),
                values={"roll": round(crit_roll, 6), "is_crit": is_crit, "multiplier": crit_multiplier},
            )
        )

        proc_roll = rng.random()
        did_proc = proc_roll < attacker.proc_chance
        proc_multiplier = attacker.proc_multiplier if did_proc else 1.0
        trace.record(
            CombatEvent(
                event_type="proc_triggers",
                round_index=round_index,
                phase="proc",
                source=EventSource(officer_id=attacker.id, ship_ability_id="officer_proc"),
                values={"roll": round(proc_roll, 6), "triggered": did_proc, "multiplier": proc_multiplier},
            )
        )

        damage = attacker.attack * effective_mitigation * crit_multiplier * proc_multiplier
        total_damage += damage
        trace.record(
            CombatEvent(
                event_type="damage_application",
                round_index=round_index,
                phase="damage",
                source=EventSource(officer_id=attacker.id, hostile_ability_id=f"{defender.id}_hull"),
                values={"final_damage": round(damage, 6), "running_total": round(total_damage, 6)},
            )
        )

        eor_damage = attacker.end_of_round_damage
        total_damage += eor_damage
        trace.record(
            CombatEvent(
                event_type="end_of_round_effects",
                round_index=round_index,
                phase="end",
                source=EventSource(player_bonus_source="artifact:radiation_array"),
                values={"bonus_damage": eor_damage, "running_total": round(total_damage, 6)},
            )
        )

    return SimulationResult(total_damage=round(total_damage, 6), events=trace.events)
