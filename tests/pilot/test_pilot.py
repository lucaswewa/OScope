import contextlib
import copy
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch
import uuid

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import pilot
import pilot_metrics as m

MIB = 1024**2


def series(minutes, noise, level):
    # The pinned P53 fixture's seeded LCG and ten-second sample interval.
    state, result = 1, []
    for k in range(int(minutes * 6)):
        state = (state * 6364136223846793005 + 1442695040888963407) & ((1 << 64) - 1)
        unit, t = (state >> 11) / (1 << 53), k * 10.0
        result.append((t, level(t) + (2 * unit - 1) * noise))
    return result


def compare_run(profile_hash, image="sha256:" + "a" * 64):
    return {"run_id": str(uuid.uuid4()), "outcome": "observed", "checks": {key: True for key in pilot.CHECKS},
            "context": {key: (profile_hash if key == "profile_sha256" else "fixture") for key in m.CONTEXT} | {"other_containers": []},
            "image_id": image, "references": {key: {"status": "passed"} for key in ("memory", "fds", "preview_drop")},
            "metrics": {key: 1.0 for key in m.METRICS}}


class Pilot(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.profile = self.root / "profile.json"
        self.profile.write_bytes((ROOT / "configs/pilot.example.json").read_bytes())
        self.p, self.fingerprint = m.profile(self.profile)

    def groups(self):
        return ([compare_run(self.fingerprint) for _ in range(3)],
                [compare_run(self.fingerprint, "sha256:" + "b" * 64) for _ in range(3)])

    def test_legacy_flat_warmup_sawtooth_spikes_are_not_leaks(self):
        flat = series(240, 20 * MIB, lambda _: 400 * MIB)
        warming = series(240, 5 * MIB, lambda t: 300 * MIB + 100 * MIB * min(t / 1200, 1))
        sawtooth = series(240, 2 * MIB, lambda t: 400 * MIB + 50 * MIB * ((t % 900) / 900))
        spiky = flat[:-5] + [(t, value + 1024 * MIB) for t, value in flat[-5:]]
        for values in (flat, warming, sawtooth, spiky):
            self.assertEqual(m.leak_reference(values, 32 * MIB, .05)["status"], "passed")

    def test_legacy_slow_growth_detected(self):
        memory = series(240, 20 * MIB, lambda t: 400 * MIB + 15 * MIB * t / 3600)
        handles = series(240, 3, lambda t: 300 + t / 60)
        self.assertEqual(m.leak_reference(memory, 32 * MIB, .05)["status"], "failed")
        self.assertEqual(m.leak_reference(handles, 32, .10)["status"], "failed")
        self.assertEqual(m.slope([(k, 3 * k + 1) for k in range(10)]), 3)

    def test_both_legacy_floor_and_fraction_are_required(self):
        large = series(240, 2 * MIB, lambda t: 2048 * MIB + 60 * MIB * t / 14400)
        few = series(240, 0, lambda t: 100 + 20 * t / 14400)
        self.assertEqual(m.leak_reference(large, 32 * MIB, .05)["status"], "passed")
        self.assertEqual(m.leak_reference(few, 32, .1)["status"], "passed")

    def test_short_or_sparse_runs_are_insufficient_not_a_pass(self):
        for values in ([], series(3, 0, lambda t: 100 + t), series(6, 0, lambda t: 100 + t)):
            self.assertEqual(m.leak_reference(values, 32, .1)["status"], "insufficient_evidence")
        self.assertEqual(m.leak_reference([(300 + i * 10, 100) for i in range(10)], 32, .1)["status"], "passed")

    def test_nonfinite_bool_duplicate_clock_and_reset_rejected(self):
        for values in ([(0, float("nan"))], [(True, 1)], [(1, 1), (1, 2)], [(2, 1), (1, 2)]):
            with self.assertRaises(ValueError):
                m.leak_reference(values, 32, .1)
        for value in (True, float("inf"), 10**400, -1):
            with self.assertRaises(ValueError):
                m.number(value)

    def test_profile_rejects_invalid_limits_cohorts_and_unknown_workload(self):
        for key, value in (("schema", True), ("workload_id", "physical-unimplemented"), ("minimum_comparison_runs", 1),
                           ("workflow_bursts", 100), ("required_endurance_seconds", False), ("allowed_regression_percent", float("nan"))):
            self.profile.write_bytes(m.encoded({**self.p, key: value}) if not isinstance(value, float) else json.dumps({**self.p, key: value}).encode())
            with self.assertRaises(ValueError):
                m.profile(self.profile)
        changed = copy.deepcopy(self.p)
        changed["cohort"] = [{"instrument": "same", "pc": "PC1", "variant": "A"}, {"instrument": "same", "pc": "PC2", "variant": "A"}]
        self.profile.write_bytes(m.encoded(changed))
        with self.assertRaises(ValueError):
            m.profile(self.profile)
        with self.assertRaises(ValueError):
            m.loads(b'{"schema":1,"schema":1}')

    def test_preflight_writes_blocked_report_without_subprocesses(self):
        args = ["preflight", "--profile", str(self.profile), "--output", str(self.root / "preflight")]
        with patch("subprocess.run", side_effect=AssertionError("no device calls")), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(pilot.main(args), 2)
        report = pilot.verify(next((self.root / "preflight").iterdir()))
        self.assertEqual(report["outcome"], "blocked")
        self.assertFalse(report["production_qualified"])

    def test_claimed_qualifications_do_not_unlock_physical_adapter(self):
        p = copy.deepcopy(self.p)
        p.update(qualifications={key: "claimed" for key in m.QUALIFICATIONS}, cohort=[{"instrument": "I", "pc": "P", "variant": "V"}],
                 required_units_per_variant=1, required_endurance_seconds=600, allowed_regression_percent=0, budgets={key: 1 for key in m.METRICS})
        self.assertEqual(len(m.gaps(p)), 1)

    def test_comparison_requires_independent_runs_and_checks(self):
        baseline, candidate = self.groups()
        result = m.compare(baseline[:1], candidate, self.p, self.fingerprint)
        self.assertEqual(result["status"], "not_comparable")
        with self.assertRaises(ValueError):
            m.compare(baseline + baseline, candidate, self.p, self.fingerprint)
        candidate[0]["checks"]["cuda"] = False
        self.assertEqual(m.compare(baseline, candidate, self.p, self.fingerprint)["status"], "not_comparable")

    def test_short_soak_missing_metric_and_contention_reject_comparison(self):
        for kind in ("short", "missing", "contention"):
            baseline, candidate = self.groups()
            if kind == "short":
                candidate[0]["references"]["memory"]["status"] = "insufficient_evidence"
            elif kind == "missing":
                candidate[0]["metrics"][m.METRICS[0]] = None
            else:
                candidate[0]["context"]["other_containers"] = ["review-deployment"]
            self.assertEqual(m.compare(baseline, candidate, self.p, self.fingerprint)["status"], "not_comparable")

    def test_context_mismatch_and_mixed_image_groups_reject_comparison(self):
        for key in m.CONTEXT:
            baseline, candidate = self.groups()
            candidate[0]["context"][key] = "changed"
            self.assertEqual(m.compare(baseline, candidate, self.p, self.fingerprint)["status"], "not_comparable")
        baseline, candidate = self.groups()
        baseline[0]["image_id"] = "sha256:" + "c" * 64
        self.assertEqual(m.compare(baseline, candidate, self.p, self.fingerprint)["status"], "not_comparable")

    def test_unconfigured_budgets_never_approve_an_optimization(self):
        baseline, candidate = self.groups()
        result = m.compare(baseline, candidate, self.p, self.fingerprint)
        self.assertEqual(result["status"], "budgets_missing")
        self.assertFalse(result["optimization_approved"])

    def test_regression_or_absolute_budget_miss_detected(self):
        baseline, candidate = self.groups()
        p = {**self.p, "allowed_regression_percent": 5, "budgets": {key: 2 for key in m.METRICS}}
        candidate[0]["metrics"][m.METRICS[0]] = 1.2
        candidate[1]["metrics"][m.METRICS[0]] = 1.2
        self.assertEqual(m.compare(baseline, candidate, p, self.fingerprint)["status"], "regression_detected")
        p["budgets"][m.METRICS[0]] = .5
        self.assertEqual(m.compare(baseline, candidate, p, self.fingerprint)["status"], "regression_detected")

    def test_same_image_control_is_explicit_and_never_approved(self):
        baseline, candidate = self.groups()
        for run in candidate:
            run["image_id"] = baseline[0]["image_id"]
        p = {**self.p, "allowed_regression_percent": 0, "budgets": {key: 2 for key in m.METRICS}}
        result = m.compare(baseline, candidate, p, self.fingerprint)
        self.assertEqual(result["status"], "comparison_available")
        self.assertTrue(result["same_image_control"])
        self.assertFalse(result["optimization_approved"])

    def observed_report(self):
        report = pilot.Report(self.root / "reports", self.profile, "engineering")
        samples = [{"t": i * 2, "rss_bytes": 1000, "fds": 10, "cpu_seconds": i, "preview_window_mean_ms": 2,
                    "sent": i + 1, "dropped": 0, "gpu": True} for i in range(20)]
        (report.root / "samples.jsonl").write_text("\n".join(json.dumps(s) for s in samples) + "\n", encoding="utf-8")
        workflows = [{"seconds": 1} for _ in range(3)]
        for i, value in enumerate(workflows):
            report.artifact("workflow_" + str(i) + ".json", value)
        report.artifact("cuda.json", {"required": True, "kernel_execution": "passed", "stdout": "Cuda  available; smoke: ok"})
        metrics, references = m.summarize(samples, workflows)
        report.report.update(metrics=metrics, references=references, checks={key: True for key in pilot.CHECKS})
        report.report["context"].update(workflow_bursts=3, seconds=45, interval=2)
        return report

    def test_report_integrity_and_recomputed_measurements(self):
        report = self.observed_report()
        report.finish("observed")
        self.assertEqual(pilot.verify(report.root)["outcome"], "observed")
        (report.root / "samples.jsonl").write_text("corrupt", encoding="utf-8")
        with self.assertRaises(ValueError):
            pilot.verify(report.root)
        report = self.observed_report()
        report.report["metrics"][m.METRICS[0]] += 1
        report.finish("observed")
        with self.assertRaises(ValueError):
            pilot.verify(report.root)

    def test_missing_cuda_incomplete_checks_and_forged_summary_cannot_pass(self):
        report = self.observed_report()
        report.report["checks"]["cuda"] = False
        with self.assertRaises(ValueError):
            report.finish("observed")
        report.report["checks"]["cuda"] = True
        report.report["references"]["memory"]["status"] = "passed"
        report.finish("observed")
        with self.assertRaises(ValueError):
            pilot.verify(report.root)

    def test_cpu_preview_and_counter_reset_fail(self):
        base = {"t": 0, "rss_bytes": 1000, "fds": 10, "cpu_seconds": 0, "preview_window_mean_ms": 1, "sent": 1, "dropped": 0, "gpu": False}
        with self.assertRaises(ValueError):
            m.summarize([base], [{"seconds": 1}])
        base["gpu"] = True
        with self.assertRaises(ValueError):
            m.summarize([base, {**base, "t": 1, "sent": 0}], [{"seconds": 1}])

    def test_html_escaping_private_state_exclusion_and_path_guard(self):
        report = pilot.Report(self.root / "reports", self.profile, "preflight")
        report.report["metadata"]["text"] = "<script>bad()</script>"
        (report.root / "private").mkdir()
        (report.root / "private/secret").write_text("never-export")
        with self.assertRaises(ValueError):
            report.artifact("../escape.json", {})
        report.finish("blocked")
        self.assertNotIn("<script>", (report.root / "report.html").read_text(encoding="utf-8"))
        self.assertNotIn("private", (report.root / "checksums.json").read_text(encoding="utf-8"))
        self.assertNotIn("never-export", (report.root / "report.json").read_text(encoding="utf-8"))

    def test_worker_shutdown_failure_still_cleans_owned_runtime(self):
        from pilot_runtime import Observer
        observer = object.__new__(Observer)
        observer.stop_workers = Mock(side_effect=RuntimeError("worker shutdown timed out"))
        observer.engine = Mock()
        observer.report = Mock(report={})
        observer.errors = [{"worker": "sampler", "type": "TimeoutError"}]
        with self.assertRaises(RuntimeError):
            observer.cleanup()
        observer.engine.cleanup.assert_called_once()
        self.assertEqual(observer.report.report["worker_errors"], observer.errors)


if __name__ == "__main__":
    unittest.main()
