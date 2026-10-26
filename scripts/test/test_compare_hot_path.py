"""Checks for fixed-work parity and speedup units, independent of a simulator run."""

import importlib.util
from pathlib import Path
import unittest


SPEC = importlib.util.spec_from_file_location(
    "compare_hot_path", Path(__file__).resolve().parents[1] / "compare_hot_path.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ComparisonTests(unittest.TestCase):
    def test_twice_the_throughput_is_half_the_time(self):
        block = [{"variant": "baseline", "elapsed_ns": 100},
                 {"variant": "candidate", "elapsed_ns": 50},
                 {"variant": "candidate", "elapsed_ns": 55},
                 {"variant": "baseline", "elapsed_ns": 110}]
        result = MODULE.paired_summary([block] * 8)
        self.assertEqual(result["median_paired_speedup"], 2)
        self.assertEqual(result["throughput_change_percent"], 100)
        self.assertEqual(result["time_reduction_percent"], 50)
        self.assertEqual(result["bootstrap_95pct_median_speedup_interval"], [2, 2])

    def test_a_regression_remains_visible(self):
        block = [{"variant": "baseline", "elapsed_ns": 50},
                 {"variant": "candidate", "elapsed_ns": 100}]
        result = MODULE.paired_summary([block] * 8)
        self.assertEqual(result["median_paired_speedup"], 0.5)
        self.assertEqual(result["throughput_change_percent"], -50)
        self.assertEqual(result["time_reduction_percent"], -100)

    def test_instrumented_reports_are_not_timing_evidence(self):
        with self.assertRaisesRegex(ValueError, "uninstrumented"):
            MODULE.checked_row({"mode": "instrumented_allocations_not_timing_data"}, "x", 64)

    def test_wrapper_and_prepared_reports_keep_their_declared_scope(self):
        for setup in (False, True):
            row = {"mode": "release_timing_uninstrumented", "case": "x", "combats": 64,
                   "actual_rounds": 6400, "elapsed_ns": 1000, "setup_timed": setup,
                   "trace_mode": "off", "workload": {"include_setup_in_timing": setup}}
            self.assertIs(MODULE.checked_row(row, "x", 64), row)
            row["setup_timed"] = not setup
            with self.assertRaisesRegex(ValueError, "Setup timing mode"):
                MODULE.checked_row(row, "x", 64)

    def test_same_winners_are_not_enough_for_parity(self):
        row = dict.fromkeys(("case", "workload_sha256", "combats", "actual_rounds",
                             "outbound_hits", "inbound_hits", "wins", "round_limit_outcomes",
                             "checksum", "probe"), 1)
        for key in ("checksum", "workload_sha256", "actual_rounds", "inbound_hits"):
            changed = {**row, key: 2}
            with self.assertRaisesRegex(ValueError, key):
                MODULE.same_work(row, changed)


if __name__ == "__main__":
    unittest.main()
