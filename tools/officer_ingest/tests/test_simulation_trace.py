import json
import unittest
from pathlib import Path

from tools.officer_ingest.simulation import (
    Combatant,
    SimulationConfig,
    format_events_human,
    serialize_events_json,
    simulate_combat,
)


class SimulationTraceTests(unittest.TestCase):
    def test_trace_snapshot_order_and_numeric_fields(self):
        fixture = Path("tools/officer_ingest/tests/fixtures/simulation_trace_snapshot.json")
        expected = json.loads(fixture.read_text())

        attacker = Combatant(
            id="officer_khan",
            attack=100,
            mitigation=0.1,
            pierce=0.15,
            crit_chance=0.2,
            crit_multiplier=1.5,
            proc_chance=0.25,
            proc_multiplier=1.2,
            end_of_round_damage=5,
        )
        defender = Combatant(
            id="hostile_interceptor",
            attack=50,
            mitigation=0.35,
            pierce=0,
            crit_chance=0.05,
            crit_multiplier=1.25,
            proc_chance=0,
            proc_multiplier=1,
            end_of_round_damage=0,
        )

        result = simulate_combat(
            attacker,
            defender,
            SimulationConfig(rounds=expected["rounds"], seed=expected["seed"], trace_mode="events"),
        )

        self.assertEqual(expected["expected_total_damage"], result.total_damage)

        event_order = [event.event_type for event in result.events]
        self.assertEqual(expected["expected_event_order"], event_order)

        events_by_round = {}
        for event in result.events:
            events_by_round.setdefault(event.round_index, {})[event.event_type] = event

        for key, expected_value in expected["numeric_expectations"].items():
            round_idx, event_type, field = key.split(".")
            actual = events_by_round[int(round_idx)][event_type].values[field]
            self.assertAlmostEqual(expected_value, actual, places=6)

        # Smoke-check serialization helpers.
        json_payload = serialize_events_json(result.events)
        parsed = json.loads(json_payload)
        self.assertEqual(len(result.events), len(parsed))
        self.assertIn("officer_khan", format_events_human(result.events))


if __name__ == "__main__":
    unittest.main()
