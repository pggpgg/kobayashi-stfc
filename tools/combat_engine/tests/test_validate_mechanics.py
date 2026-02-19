from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3]))

from tools.combat_engine.validation import validate_mechanics


def test_edge_mechanics_severity_and_fidelity() -> None:
    fixture = Path("tools/combat_engine/tests/fixtures/edge_mechanics.officers.json")
    officers = json.loads(fixture.read_text())

    diagnostics = validate_mechanics(officers)
    by_officer: dict[str, list[dict[str, str | None]]] = {}
    for diag in diagnostics:
        by_officer.setdefault(diag["officer_id"], []).append(diag)

    iso_messages = [d["message"] for d in by_officer["iso-officer"] if d["severity"] == "warning"]
    assert any("isolytic" in message for message in iso_messages)

    burn_entries = by_officer["burn-miner"]
    assert any(d["severity"] == "warning" and "partial" in d["message"] for d in burn_entries)
    assert any(d["severity"] == "info" and "non-combat" in d["message"] for d in burn_entries)

    bad_entries = by_officer["bad-data"]
    assert any(d["severity"] == "error" and "malformed" in d["message"] for d in bad_entries)

    fidelity = {officer["id"]: officer["simulation_fidelity"] for officer in officers}
    assert fidelity["iso-officer"]["fidelity"] == "partial"
    assert fidelity["burn-miner"]["has_partial_mechanics"] is True
    assert fidelity["bad-data"]["diagnostic_severity"] == "error"
