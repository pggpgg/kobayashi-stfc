import json
import unittest
from pathlib import Path

from tools.officer_ingest.import_officers import validate


class ValidateMechanicsTests(unittest.TestCase):
    def test_edge_mechanics_severity_and_fidelity(self):
        fixture = Path("tools/officer_ingest/tests/fixtures/edge_mechanics.officers.json")
        officers = json.loads(fixture.read_text())

        diagnostics = validate(officers)
        by_officer = {}
        for diag in diagnostics:
            by_officer.setdefault(diag["officer_id"], []).append(diag)

        iso_messages = [d["message"] for d in by_officer["iso-officer"] if d["severity"] == "warning"]
        self.assertTrue(any("isolytic" in msg for msg in iso_messages))

        burn_entries = by_officer["burn-miner"]
        self.assertTrue(any(d["severity"] == "warning" and "partial" in d["message"] for d in burn_entries))
        self.assertTrue(any(d["severity"] == "info" and "non-combat" in d["message"] for d in burn_entries))

        bad_entries = by_officer["bad-data"]
        self.assertTrue(any(d["severity"] == "error" and "malformed" in d["message"] for d in bad_entries))

        fidelity = {o["id"]: o["simulation_fidelity"] for o in officers}
        self.assertEqual("partial", fidelity["iso-officer"]["fidelity"])
        self.assertTrue(fidelity["burn-miner"]["has_partial_mechanics"])
        self.assertEqual("error", fidelity["bad-data"]["diagnostic_severity"])


if __name__ == "__main__":
    unittest.main()
