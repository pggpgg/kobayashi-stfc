#!/usr/bin/env python3
"""Fixed-work, paired comparison of uninstrumented hot_path_bench executables.

Build both executables from the same frozen harness. Runs are sequential. Keep
the output directory outside tracked source: it contains raw process output.
"""

import argparse
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import random
import re
import statistics
import subprocess
import sys
import time


def sha256(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def data_tree(root):
    digest = hashlib.sha256()
    count = 0
    for path in sorted(root.rglob("*"), key=lambda item: item.relative_to(root).as_posix()):
        if path.is_symlink():
            raise ValueError("Benchmark catalogs must not contain symlinks")
        if path.is_file():
            digest.update(path.relative_to(root).as_posix().encode())
            digest.update(b"\0")
            digest.update(bytes.fromhex(sha256(path)))
            count += 1
    return {"sha256": digest.hexdigest(), "file_count": count}


def affinity(mask):
    if mask is None:
        return
    if mask <= 0:
        raise ValueError("Affinity mask must be positive")
    if os.name == "nt":
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.GetCurrentProcess.restype = ctypes.c_void_p
        kernel.SetProcessAffinityMask.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        if not kernel.SetProcessAffinityMask(kernel.GetCurrentProcess(), mask):
            raise ctypes.WinError(ctypes.get_last_error())
    else:
        os.sched_setaffinity(0, {i for i in range(mask.bit_length()) if mask & (1 << i)})


def checked_row(row, expected_case, iterations):
    if row.get("mode") != "release_timing_uninstrumented":
        raise ValueError("Only uninstrumented timing reports are comparable")
    if row["case"] != expected_case or row["combats"] != iterations:
        raise ValueError("Benchmark returned a different workload count")
    if not 0 < row["elapsed_ns"] or row["actual_rounds"] <= 0:
        raise ValueError("Benchmark timing or executed-round count is invalid")
    if row["setup_timed"] != row["workload"]["include_setup_in_timing"] or row["trace_mode"] != "off":
        raise ValueError("Setup timing mode differs from the declared workload")
    return row


def same_work(a, b):
    for key in ("case", "workload_sha256", "combats", "actual_rounds", "outbound_hits",
                "inbound_hits", "wins", "round_limit_outcomes", "checksum", "probe"):
        if a[key] != b[key]:
            raise ValueError(f"Baseline/candidate parity mismatch in {key}")


BUILD_SOURCE_KEYS = {
    "src/bin/hot_path_bench.rs": "harness_sha256",
    "src/bin/hot_path_bench/allocations.rs": "allocator_helper_sha256",
    "Cargo.lock": "cargo_lock_sha256",
}
FROZEN_PROTOCOL_KEYS = (
    "baseline_sha256", "harness_sha256", "allocator_helper_sha256",
    "cargo_lock_sha256", "data_tree", "affinity_mask", "rayon_threads", "simd",
)


def read_build_manifest(manifest_path, executable, source_hashes):
    """Check an external build-time attestation against the immutable executable.

    The caller must capture this manifest at build time. Runtime source hashes
    alone cannot recover compiler flags or prove what source an executable used.
    Root metadata is retained for provenance; only the build configuration and
    required source/executable hashes participate in configuration validation.
    """
    manifest_bytes = Path(manifest_path).read_bytes()
    manifest = json.loads(manifest_bytes)
    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        raise ValueError("Unsupported build manifest schema_version")
    executable_hash = manifest.get("executable_sha256")
    sources = manifest.get("source_hashes")
    if not isinstance(sources, dict) or not set(BUILD_SOURCE_KEYS).issubset(sources):
        raise ValueError("Build manifest requires the harness/helper/lock source_hashes")
    for digest in [executable_hash, *sources.values()]:
        if not isinstance(digest, str) or not re.fullmatch(r"[a-f0-9]{64}", digest):
            raise ValueError("Build manifest requires lowercase SHA-256 hashes")
    if executable_hash != sha256(executable):
        raise ValueError("Build manifest executable SHA-256 mismatch")
    for source, expected_hash in source_hashes.items():
        if sources[source] != expected_hash:
            raise ValueError(f"Build manifest source SHA-256 mismatch: {source}")
    build = manifest.get("build")
    required = {"compiler", "target_triple", "profile", "features", "rustflags",
                "cargo_profile", "target_config"}
    if not isinstance(build, dict) or not required.issubset(build):
        raise ValueError("Build manifest is missing compiler/profile/features/flags/target configuration")
    for field in ("compiler", "target_triple"):
        if not isinstance(build[field], str) or not build[field].strip():
            raise ValueError(f"Build manifest requires nonempty {field}")
    if build["profile"] != "release":
        raise ValueError("Timing build manifest must use the release profile")
    for field in ("features", "rustflags"):
        if not isinstance(build[field], list) or not all(isinstance(x, str) for x in build[field]):
            raise ValueError(f"Build manifest {field} must be an explicit string array")
    if len(set(build["features"])) != len(build["features"]):
        raise ValueError("Build manifest features must be unique")
    if any(feature.rsplit("/", 1)[-1] == "bench-alloc-count" for feature in build["features"]):
        raise ValueError("Allocation-instrumented build manifests cannot supply timing data")
    for field, keys in (
        ("cargo_profile", {"opt_level", "lto", "codegen_units", "panic", "strip", "debug"}),
        ("target_config", {"target_cpu", "target_features", "linker"}),
    ):
        if not isinstance(build[field], dict) or not keys.issubset(build[field]):
            raise ValueError(f"Build manifest requires explicit {field} settings")
    return {"sha256": hashlib.sha256(manifest_bytes).hexdigest(), "manifest": manifest}


def validate_build_pair(baseline, candidate):
    if baseline["manifest"]["build"] != candidate["manifest"]["build"]:
        raise ValueError("Baseline/candidate compiler/profile/features/flags/target configuration differs")


def validate_frozen_protocol(frozen, current):
    for key in FROZEN_PROTOCOL_KEYS:
        if frozen[key] != current[key]:
            raise ValueError(f"Frozen measurement protocol changed: {key}")
    # Legacy baseline summaries predate optional external build manifests.
    if frozen.get("build_manifests"):
        recorded = frozen["build_manifests"]["baseline"]["manifest"]
        supplied = current.get("build_manifests", {}).get("baseline", {}).get("manifest")
        if supplied is None:
            raise ValueError("Frozen measurement protocol requires its baseline build manifest")
        for key in ("executable_sha256", "source_hashes", "build"):
            if recorded[key] != supplied[key]:
                raise ValueError(f"Frozen baseline build manifest changed: {key}")


def paired_summary(blocks):
    """Pair each block's two baseline and two candidate fixed-work samples."""
    ratios = []
    for block in blocks:
        baseline = statistics.fmean(row["elapsed_ns"] for row in block if row["variant"] == "baseline")
        candidate = statistics.fmean(row["elapsed_ns"] for row in block if row["variant"] == "candidate")
        ratios.append(baseline / candidate)
    median = statistics.median(ratios)
    # Resample whole paired blocks, never individual baseline/candidate runs.
    rng = random.Random(0x4B4F4241)
    bootstrap = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(10_000))
    interval = [bootstrap[249], bootstrap[9749]]
    return {
        "paired_blocks": len(blocks), "paired_speedup_ratios": ratios,
        "median_paired_speedup": median, "bootstrap_95pct_median_speedup_interval": interval,
        "throughput_change_percent": (median - 1) * 100,
        "time_reduction_percent": (1 - 1 / median) * 100,
        "uncertainty_method": "deterministic percentile bootstrap of whole paired ABBA/BAAB blocks; 10000 resamples",
    }


class Runner:
    def __init__(self, output, cwd, environment, process_timeout=300):
        if not math.isfinite(process_timeout) or process_timeout <= 0:
            raise ValueError("Process timeout must be positive and finite")
        self.output = output
        self.cwd = cwd
        self.environment = environment
        self.sequence = 0
        self.process_timeout = process_timeout

    def call(self, executable, arguments, label):
        self.sequence += 1
        stem = self.output / f"{self.sequence:04d}-{label}"
        started = time.monotonic()
        executable_hash = sha256(executable)
        timed_out = False
        launch_error = None
        exit_code = None
        try:
            result = subprocess.run(
                [str(executable), *arguments], cwd=self.cwd, env=self.environment,
                capture_output=True, text=True, timeout=self.process_timeout,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
            stdout, stderr, exit_code = result.stdout, result.stderr, result.returncode
        except subprocess.TimeoutExpired as error:
            # subprocess.run kills and waits for its own child before raising.
            # Partial captured output may be bytes even when text=True.
            stdout, stderr = error.stdout or "", error.stderr or ""
            timed_out = True
        except OSError as error:
            stdout, stderr = "", str(error)
            launch_error = type(error).__name__
        if isinstance(stdout, bytes):
            stdout = stdout.decode("utf-8", errors="replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode("utf-8", errors="replace")
        stem.with_suffix(".stdout.jsonl").write_text(stdout, encoding="utf-8")
        stem.with_suffix(".stderr.log").write_text(stderr, encoding="utf-8")
        stem.with_suffix(".process.json").write_text(json.dumps({
            "exit_code": exit_code, "timed_out": timed_out, "launch_error": launch_error,
            "process_timeout_seconds": self.process_timeout,
            "wall_seconds_including_setup": time.monotonic() - started,
            "arguments": arguments, "executable_sha256": executable_hash,
        }, indent=2) + "\n", encoding="utf-8")
        if timed_out:
            raise RuntimeError(f"Benchmark exceeded {self.process_timeout}s process timeout; see {stem.name} logs")
        if launch_error:
            raise RuntimeError(f"Benchmark could not start ({launch_error}); see {stem.name} logs")
        if exit_code:
            raise RuntimeError(f"Benchmark failed ({exit_code}); see {stem.name} logs")
        return [json.loads(line) for line in stdout.splitlines() if line.strip()]

    def timing(self, executable, name, iterations, label):
        rows = self.call(executable, ["--case", name, "--iterations", str(iterations)], label)
        if len(rows) != 1:
            raise ValueError("Expected exactly one fixed-work timing report")
        return checked_row(rows[0], name, iterations)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--runtime", default=Path.cwd(), type=Path)
    parser.add_argument("--case", action="append", dest="cases")
    parser.add_argument("--blocks", default=8, type=int)
    parser.add_argument("--seconds", default=0.75, type=float)
    parser.add_argument("--affinity", type=lambda value: int(value, 0))
    parser.add_argument("--threads", default=1, type=int)
    parser.add_argument("--process-timeout", default=300, type=float,
                        help="Maximum wall-clock seconds per owned benchmark child, including probes (default 300)")
    parser.add_argument("--baseline-build-manifest", type=Path)
    parser.add_argument("--candidate-build-manifest", type=Path)
    parser.add_argument("--counts-from", type=Path,
                        help="Use frozen calibrated counts from an earlier baseline summary")
    args = parser.parse_args()
    if args.blocks < 2 or not 0 < args.seconds <= 60 or args.threads < 1:
        parser.error("Need blocks >= 2, seconds in (0,60], and threads >= 1")
    if not math.isfinite(args.process_timeout) or args.process_timeout <= 0:
        parser.error("Need a positive finite --process-timeout")
    if args.candidate_build_manifest and not args.candidate:
        parser.error("--candidate-build-manifest requires --candidate")
    if args.candidate and bool(args.baseline_build_manifest) != bool(args.candidate_build_manifest):
        parser.error("Paired build validation requires both build manifests")
    baseline = args.baseline.resolve(strict=True)
    candidate = args.candidate.resolve(strict=True) if args.candidate else None
    args.output.mkdir(parents=False, exist_ok=False)
    affinity(args.affinity)
    env = {key: value for key, value in os.environ.items() if not key.startswith("KOBAYASHI_")}
    env.update({"KOBAYASHI_HOME": str(args.runtime.resolve()),
                "KOBAYASHI_RAYON_THREADS": str(args.threads),
                "KOBAYASHI_EXPERIMENTAL_SIMD_DAMAGE_KERNEL": "0"})
    run = Runner(args.output, args.runtime.resolve(), env, args.process_timeout)
    listing = run.call(baseline, ["--list"], "baseline-list")[0]
    cases = args.cases or listing["cases"]
    if len(set(cases)) != len(cases) or not set(cases).issubset(listing["cases"]):
        raise ValueError("Duplicate or unknown case")
    if candidate and listing != run.call(candidate, ["--list"], "candidate-list")[0]:
        raise ValueError("Executables expose different benchmark matrices")
    context = {"schema_version": 1, "baseline_sha256": sha256(baseline),
               "candidate_sha256": sha256(candidate) if candidate else None,
               "harness_sha256": sha256(args.runtime / "src/bin/hot_path_bench.rs"),
               "allocator_helper_sha256": sha256(args.runtime / "src/bin/hot_path_bench/allocations.rs"),
               "cargo_lock_sha256": sha256(args.runtime / "Cargo.lock"),
               "data_tree": data_tree(args.runtime / "data"),
               "affinity_mask": args.affinity, "rayon_threads": args.threads,
               # Retain the original protocol's field as the requested flag value.
               # The baseline engine ignores that flag and automatically selects
               # its AVX2 kernel in TraceOff mode. Do not claim scalar measurements.
               "simd": False, "simd_field_scope": "requested environment flag only",
               "kernel_policy": "unchanged automatic AVX2 when supported and tracing is off; legacy engine gate ignores the environment request",
               "cases": cases, "blocks": args.blocks,
               "target_seconds": args.seconds, "process_timeout_seconds": args.process_timeout,
               "build_validation": "not_supplied", "results": {}}
    if args.baseline_build_manifest:
        source_hashes = {source: context[key] for source, key in BUILD_SOURCE_KEYS.items()}
        baseline_build = read_build_manifest(args.baseline_build_manifest, baseline, source_hashes)
        manifests = {"baseline": baseline_build}
        if candidate:
            manifests["candidate"] = read_build_manifest(args.candidate_build_manifest, candidate, source_hashes)
            validate_build_pair(baseline_build, manifests["candidate"])
        context["build_manifests"] = manifests
        context["build_validation"] = "external_build_time_attestations_bound_to_executable_and_sources"
    frozen = json.loads(args.counts_from.read_text()) if args.counts_from else None
    if frozen:
        validate_frozen_protocol(frozen, context)
    for name in cases:
        # Full-result hashes, including ordered traced events, are outside timing.
        baseline_parity = run.call(baseline, ["--case", name, "--parity"], f"{name}-baseline-parity")
        if candidate:
            candidate_parity = run.call(candidate, ["--case", name, "--parity"], f"{name}-candidate-parity")
            if baseline_parity != candidate_parity:
                raise ValueError(f"Full-result parity failed for {name}")
        parity_sha = hashlib.sha256(json.dumps(baseline_parity, sort_keys=True).encode()).hexdigest()
        pilot = run.timing(baseline, name, 64, f"{name}-pilot")
        if frozen:
            record = frozen["results"][name]
            iterations = record["iterations"]
            if pilot["workload_sha256"] != record["workload_sha256"] or parity_sha != record["parity_sha256"]:
                raise ValueError("Frozen workload or full-result parity panel changed")
        else:
            estimate = args.seconds * 1e9 / (pilot["elapsed_ns"] / 64)
            iterations = max(64, min(1_000_000_000, math.ceil(estimate / 64) * 64))
        reference = run.timing(baseline, name, iterations, f"{name}-baseline-warmup")
        if candidate:
            same_work(reference, run.timing(candidate, name, iterations, f"{name}-candidate-warmup"))
        result = {"iterations": iterations, "workload_sha256": pilot["workload_sha256"],
                  "parity_sha256": parity_sha, "parity_rows": len(baseline_parity),
                  "probe": pilot["probe"], "samples": [], "blocks": []}
        for block_index in range(args.blocks):
            order = (["baseline", "candidate", "candidate", "baseline"] if block_index % 2 == 0
                     else ["candidate", "baseline", "baseline", "candidate"]) if candidate else ["baseline"]
            block = []
            for slot, variant in enumerate(order):
                row = run.timing(baseline if variant == "baseline" else candidate, name, iterations,
                                 f"{name}-{block_index:02d}-{slot}-{variant}")
                same_work(reference, row)
                sample = {**row, "variant": variant, "block": block_index, "slot": slot}
                block.append(sample)
                result["samples"].append(sample)
            result["blocks"].append(block)
        for variant in (["baseline", "candidate"] if candidate else ["baseline"]):
            times = [row["elapsed_ns"] for row in result["samples"] if row["variant"] == variant]
            median = statistics.median(times)
            result[variant] = {"median_elapsed_ns": median,
                               "median_combats_per_second": iterations * 1e9 / median,
                               "median_actual_rounds_per_second": reference["actual_rounds"] * 1e9 / median,
                               "sample_count": len(times)}
        if candidate:
            result["comparison"] = paired_summary(result["blocks"])
        context["results"][name] = result
        (args.output / "summary.json").write_text(json.dumps(context, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"case": name, "baseline": result["baseline"],
                          "comparison": result.get("comparison")}), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
