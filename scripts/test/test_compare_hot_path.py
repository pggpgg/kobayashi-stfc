"""Checks for fixed-work parity and speedup units, independent of a simulator run."""

import importlib.util
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch


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


class DriverRobustnessTests(unittest.TestCase):
    def setUp(self):
        temp_parent = Path(__file__).resolve().parents[2] / "local-analysis"
        temp_parent.mkdir(exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="driver-tests-", dir=temp_parent)
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.executable = self.root / "executable.bin"
        self.executable.write_bytes(b"frozen executable")
        self.sources = {source: MODULE.hashlib.sha256(source.encode()).hexdigest()
                        for source in MODULE.BUILD_SOURCE_KEYS}
        self.manifest = {
            "schema_version": 1,
            "executable_sha256": MODULE.sha256(self.executable),
            "source_hashes": self.sources.copy(),
            "build": {
                "compiler": "rustc 1.93.1 test identity", "target_triple": "test-target",
                "profile": "release", "features": [], "rustflags": [],
                "cargo_profile": {"opt_level": 3, "lto": "fat", "codegen_units": 1,
                                  "panic": "abort", "strip": "symbols", "debug": False},
                "target_config": {"target_cpu": "default", "target_features": [],
                                  "linker": "default"},
            },
        }

    def read_manifest(self, manifest=None):
        path = self.root / "build.json"
        path.write_text(json.dumps(self.manifest if manifest is None else manifest), encoding="utf-8")
        return MODULE.read_build_manifest(path, self.executable, self.sources)

    def test_timeout_preserves_partial_output_and_failure_metadata(self):
        runner = MODULE.Runner(self.root, self.root, os.environ.copy(), process_timeout=0.25)
        arguments = ["-c", "import sys,time;print('partial',flush=True);"
                     "print('diagnostic',file=sys.stderr,flush=True);time.sleep(5)"]
        started = time.monotonic()
        with self.assertRaisesRegex(RuntimeError, "process timeout"):
            runner.call(Path(sys.executable), arguments, "timeout")
        self.assertLess(time.monotonic() - started, 4)
        self.assertIn("partial", (self.root / "0001-timeout.stdout.jsonl").read_text())
        self.assertIn("diagnostic", (self.root / "0001-timeout.stderr.log").read_text())
        record = json.loads((self.root / "0001-timeout.process.json").read_text())
        self.assertTrue(record["timed_out"])
        self.assertIsNone(record["exit_code"])
        self.assertEqual(record["process_timeout_seconds"], 0.25)
        self.assertEqual(record["executable_sha256"], MODULE.sha256(sys.executable))

    def test_timeout_with_no_output_and_launch_failure_also_record_evidence(self):
        runner = MODULE.Runner(self.root, self.root, {}, process_timeout=1)
        with patch.object(MODULE.subprocess, "run", side_effect=subprocess.TimeoutExpired(["x"], 1)):
            with self.assertRaisesRegex(RuntimeError, "process timeout"):
                runner.call(self.executable, [], "silent")
        self.assertEqual((self.root / "0001-silent.stdout.jsonl").read_text(), "")
        with patch.object(MODULE.subprocess, "run", side_effect=PermissionError("blocked")):
            with self.assertRaisesRegex(RuntimeError, "could not start"):
                runner.call(self.executable, [], "launch")
        record = json.loads((self.root / "0002-launch.process.json").read_text())
        self.assertFalse(record["timed_out"])
        self.assertEqual(record["launch_error"], "PermissionError")

    def test_nonzero_exit_preserves_output_without_becoming_a_sample(self):
        runner = MODULE.Runner(self.root, self.root, {})
        completed = subprocess.CompletedProcess(["x"], 7, '{"partial":true}\n', "failure")
        with patch.object(MODULE.subprocess, "run", return_value=completed):
            with self.assertRaisesRegex(RuntimeError, "failed \\(7\\)"):
                runner.call(self.executable, [], "failed")
        self.assertEqual((self.root / "0001-failed.stderr.log").read_text(), "failure")

    def test_invalid_timeout_is_rejected(self):
        for value in [0, -1, float("inf"), float("nan")]:
            with self.subTest(value=value), self.assertRaisesRegex(ValueError, "positive and finite"):
                MODULE.Runner(self.root, self.root, {}, process_timeout=value)

    def test_manifests_bind_executable_and_each_frozen_source(self):
        baseline = self.read_manifest()
        MODULE.validate_build_pair(baseline, self.read_manifest())
        self.executable.write_bytes(b"different executable")
        with self.assertRaisesRegex(ValueError, "executable SHA"):
            self.read_manifest()
        self.executable.write_bytes(b"frozen executable")
        for source in self.sources:
            changed = copy.deepcopy(self.manifest)
            changed["source_hashes"][source] = "0" * 64
            with self.subTest(source=source), self.assertRaisesRegex(ValueError, "source SHA"):
                self.read_manifest(changed)

    def test_build_manifest_retains_additional_engine_hashes_without_equating_revisions(self):
        changed = copy.deepcopy(self.manifest)
        changed["source_hashes"]["src/combat/engine.rs"] = "0" * 64
        changed["source_revision"] = "candidate"
        baseline = self.read_manifest()
        candidate = self.read_manifest(changed)
        MODULE.validate_build_pair(baseline, candidate)
        self.assertEqual(candidate["manifest"]["source_revision"], "candidate")

    def test_compiler_flags_features_and_target_changes_cannot_silently_compare(self):
        baseline = self.read_manifest()
        changes = [
            ("compiler", "different compiler"), ("target_triple", "different-target"),
            ("features", ["other-feature"]), ("rustflags", ["-C", "target-cpu=native"]),
            ("cargo_profile", {**self.manifest["build"]["cargo_profile"], "lto": "thin"}),
            ("target_config", {**self.manifest["build"]["target_config"], "target_cpu": "native"}),
        ]
        for key, value in changes:
            changed = copy.deepcopy(self.manifest)
            changed["build"][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "configuration differs"):
                MODULE.validate_build_pair(baseline, self.read_manifest(changed))

    def test_nonrelease_instrumented_or_incomplete_attestations_are_rejected(self):
        for key, value in [("profile", "dev"), ("features", ["bench-alloc-count"]),
                           ("rustflags", ""), ("cargo_profile", {}), ("target_config", {})]:
            changed = copy.deepcopy(self.manifest)
            changed["build"][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.read_manifest(changed)
        changed = copy.deepcopy(self.manifest)
        del changed["source_hashes"]["Cargo.lock"]
        with self.assertRaisesRegex(ValueError, "source_hashes"):
            self.read_manifest(changed)

    def test_old_frozen_summaries_remain_valid_but_new_attestations_cannot_be_dropped(self):
        frozen = dict.fromkeys(MODULE.FROZEN_PROTOCOL_KEYS, "same")
        current = {**frozen, "process_timeout_seconds": 300,
                   "kernel_policy": "automatic AVX2 as supported"}
        MODULE.validate_frozen_protocol(frozen, current)
        build = self.read_manifest()
        frozen["build_manifests"] = {"baseline": build}
        with self.assertRaisesRegex(ValueError, "requires its baseline build manifest"):
            MODULE.validate_frozen_protocol(frozen, current)
        current["build_manifests"] = {"baseline": build}
        MODULE.validate_frozen_protocol(frozen, current)


if __name__ == "__main__":
    unittest.main()
