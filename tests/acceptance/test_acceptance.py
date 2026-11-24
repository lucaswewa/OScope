import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import acceptance as a
import acceptance_workflows as w

IDENTITY = {"instrument": "ENG-001", "pc": "PC-001", "operator": "operator", "station": "bench"}


class Acceptance(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.profile = self.root / "profile.json"
        self.profile.write_bytes((ROOT / "configs/acceptance.example.json").read_bytes())

    def attempt(self, **kwargs):
        return a.Attempt(self.root / "runs", self.profile, IDENTITY, "engineering", **kwargs)

    def success(self, attempt):
        return attempt.execute([(n, lambda: {"fixture": True}) for n in a.ENGINEERING_STEPS[:-1]])

    def test_unqualified_profile_and_checklist(self):
        profile, sha = a.load_profile(self.profile)
        self.assertEqual(len(sha), 64)
        self.assertGreater(len(a.factory_gaps(profile)), 12)
        self.assertGreaterEqual(len(a.CHECKLIST), 10)
        self.assertTrue(any("operator signs" in text for text in a.CHECKLIST))

    def test_nonfinite_boolean_and_invalid_limits_rejected(self):
        profile = json.loads(self.profile.read_bytes())
        for value in (True, False, -1, 0, "1", float("nan"), float("inf"), 1e309, 10**400):
            with self.subTest(value=value):
                changed = copy.deepcopy(profile)
                changed["physical_limits"]["camera_min_fps"] = value
                self.profile.write_text(json.dumps(changed), encoding="utf-8")
                with self.assertRaises(ValueError):
                    a.load_profile(self.profile)

    def test_duplicate_unknown_and_wrong_profile_schema_rejected(self):
        for raw in ('{"schema":1,"schema":1}', '[]', '{"schema":true}', '{"extra":null}'):
            self.profile.write_text(raw, encoding="utf-8")
            with self.assertRaises((ValueError, TypeError)):
                a.load_profile(self.profile)

    def test_unknown_physical_fields_and_fractional_frame_count_rejected(self):
        base = json.loads(self.profile.read_bytes())
        for field, value in (("unknown", 1), ("camera_test_frames", 1.5)):
            changed = copy.deepcopy(base)
            changed["physical_limits"][field] = value
            self.profile.write_text(json.dumps(changed), encoding="utf-8")
            with self.assertRaises(ValueError):
                a.load_profile(self.profile)

    def test_factory_preflight_blocked_without_calling_docker(self):
        args = ["factory-preflight", "--profile", str(self.profile), "--output", str(self.root / "preflight")]
        for key, value in IDENTITY.items():
            args.extend(["--" + key, value])
        with patch("subprocess.run", side_effect=AssertionError("must not call external tools")), contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(a.main(args), 2)
        report = a.verify_report(next((self.root / "preflight").iterdir()))
        self.assertEqual(report["outcome"], "blocked")
        self.assertIs(report["shipment_authorized"], False)
        self.assertEqual(report["physical_qualification"], "not_run")

    def test_profile_claims_cannot_enable_unimplemented_factory_adapter(self):
        profile, _ = a.load_profile(self.profile)
        profile["qualifications"] = {k: "claimed-approved" for k in a.QUALIFICATIONS}
        profile["physical_limits"] = {k: 1 for k in a.LIMITS}
        for device in profile["devices"].values():
            device.update(serial="claimed-serial", firmware="claimed-version")
        self.assertEqual(len(a.factory_gaps(profile)), 3)

    def test_passing_fixture_report_is_explicitly_engineering(self):
        attempt = self.attempt()
        self.assertEqual(self.success(attempt), 0)
        report = a.verify_report(attempt.root)
        self.assertEqual(report["outcome"], "engineering_passed")
        self.assertIs(report["shipment_authorized"], False)
        self.assertEqual([s["status"] for s in report["steps"]], ["passed"] * len(a.ENGINEERING_STEPS))
        with self.assertRaises(ValueError):
            attempt.finish("engineering_passed")

    def test_failed_step_stops_remaining_and_does_not_export_exception_secret(self):
        attempt = self.attempt()
        later, cleanup = Mock(), Mock(return_value={"retained": True})
        def fail():
            raise RuntimeError("secret-password-do-not-export")
        self.assertEqual(attempt.execute([("first", fail), ("later", later)], cleanup), 1)
        later.assert_not_called()
        cleanup.assert_called_once()
        self.assertEqual([s["status"] for s in attempt.report["steps"]], ["failed", "not_run", "passed"])
        a.verify_report(attempt.root)
        for path in attempt.root.iterdir():
            self.assertNotIn(b"secret-password", path.read_bytes())

    def test_interruption_retained_and_cleanup_runs(self):
        attempt = self.attempt()
        cleanup = Mock(return_value={})
        interrupt = Mock(side_effect=KeyboardInterrupt())
        self.assertEqual(attempt.execute([("work", interrupt), ("later", Mock())], cleanup), 130)
        cleanup.assert_called_once()
        self.assertEqual(a.verify_report(attempt.root)["outcome"], "interrupted")

    def test_cleanup_failure_cannot_pass(self):
        attempt = self.attempt()
        result = attempt.execute([(n, lambda: {}) for n in a.ENGINEERING_STEPS[:-1]], Mock(side_effect=OSError()))
        self.assertEqual(result, 1)
        self.assertEqual(a.verify_report(attempt.root)["outcome"], "failed")

    def test_incomplete_pass_refused(self):
        attempt = self.attempt()
        with self.assertRaises(ValueError):
            attempt.finish("engineering_passed")

    def test_checksums_detect_corruption_and_reject_traversal(self):
        for target in ("report.json", "report.html", "events.jsonl", "profile.json"):
            attempt = self.attempt()
            self.success(attempt)
            (attempt.root / target).write_bytes(b"corrupt")
            with self.assertRaises(ValueError):
                a.verify_report(attempt.root)
        attempt = self.attempt()
        self.success(attempt)
        manifest = json.loads((attempt.root / "checksums.json").read_bytes())
        manifest["../outside.json"] = "0" * 64
        (attempt.root / "checksums.json").write_bytes(a.encoded(manifest))
        with self.assertRaises(ValueError):
            a.verify_report(attempt.root)

    def test_html_escapes_embedded_metadata(self):
        attempt = self.attempt()
        attempt.report["metadata"]["untrusted"] = '<script>alert("x")</script>'
        self.success(attempt)
        rendered = (attempt.root / "report.html").read_text(encoding="utf-8")
        self.assertNotIn("<script>", rendered)
        self.assertIn("&lt;script&gt;", rendered)

    def test_rework_is_new_attempt_with_unchanged_old_evidence(self):
        old = self.attempt()
        old.execute([("failure", Mock(side_effect=ValueError()))])
        before = (old.root / "report.json").read_bytes()
        new = self.attempt(previous=old.root / "report.json")
        self.success(new)
        self.assertNotEqual(new.root, old.root)
        self.assertEqual(new.report["previous"]["run_id"], old.report["run_id"])
        self.assertEqual((old.root / "report.json").read_bytes(), before)
        for key in ("instrument", "pc"):
            different = {**IDENTITY, key: "another"}
            with self.assertRaises(ValueError):
                a.Attempt(self.root / "runs", self.profile, different, "engineering", old.root / "report.json")
        changed = json.loads(self.profile.read_bytes())
        changed["revision"] += 1
        self.profile.write_bytes(a.encoded(changed))
        with self.assertRaises(ValueError):
            self.attempt(previous=old.root / "report.json")

    def test_private_application_state_not_in_manifest(self):
        attempt = self.attempt()
        (attempt.root / "private").mkdir()
        (attempt.root / "private/password").write_text("do-not-export")
        self.success(attempt)
        self.assertNotIn("private", (attempt.root / "checksums.json").read_text())
        self.assertNotIn("do-not-export", (attempt.root / "report.json").read_text())

    def test_gpu_skip_and_cpu_fallback_are_failures(self):
        engine = w.Engineering(self.attempt(), "fixture")
        for text in ("smoke: ok; CPU", "Cuda  available; smoke skipped", "no GPU in use"):
            engine.docker = Mock(return_value=Mock(stdout=text))
            with self.assertRaises(AssertionError):
                engine.cuda()

    def test_cleanup_refuses_foreign_container_label(self):
        engine = w.Engineering(self.attempt(), "fixture")
        engine.created = True
        engine.docker = Mock(return_value=Mock(returncode=0, stdout='[{"Config":{"Labels":{"oscope.acceptance":"foreign"}}}]'))
        with self.assertRaises(AssertionError):
            engine.cleanup()
        self.assertEqual(engine.docker.call_count, 1)

    def test_identity_and_evidence_filename_validation(self):
        for value in ("", "../instrument", "a\nb", "<script>", True):
            with self.assertRaises(ValueError):
                a.identifier(value)
        attempt = self.attempt()
        with self.assertRaises(ValueError):
            attempt.artifact("../outside.json", {})


if __name__ == "__main__":
    unittest.main()
