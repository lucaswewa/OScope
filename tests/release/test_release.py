import copy
import hashlib
import io
import json
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import release_bundle as b
import release_state as s
from fixtures import bundle, installation, Driver, sign


class Release(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.keys_tmp = tempfile.TemporaryDirectory()
        cls.keys = Path(cls.keys_tmp.name) / "keys"
        b.lab_keygen(cls.keys)

    @classmethod
    def tearDownClass(cls):
        cls.keys_tmp.cleanup()

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.install = self.root / "installation"
        installation(self.install, self.keys)
        self.trust = self.install / "trust/trust.json"

    def test_real_signature_and_archive_identity(self):
        directory = bundle(self.root, self.keys)
        manifest, sha = b.verify(directory, self.trust)
        self.assertEqual(sha, b.sha(directory / "manifest.json"))
        self.assertEqual(manifest["scope"], b.SCOPE)

    def test_tampered_signature_manifest_payload_and_extra_file_rejected(self):
        for name in ("manifest.sig", "manifest.json", "backend-image.tar.gz", "LICENSE", "extra.txt"):
            with self.subTest(name=name):
                directory = bundle(self.root, self.keys)
                with (directory / name).open("ab") as stream:
                    stream.write(b"tamper")
                with self.assertRaises(ValueError):
                    b.verify(directory, self.trust)

    def test_wrong_key_unknown_key_and_revocation_rejected(self):
        directory = bundle(self.root, self.keys)
        other = self.root / "other"
        b.lab_keygen(other)
        with self.assertRaises(ValueError):
            b.verify(directory, other / "trust.json")
        policy = s.read(self.trust)
        for field, value in (("id", "unknown"), ("revoked", True), ("sha256", "0" * 64)):
            changed = copy.deepcopy(policy)
            changed["keys"][0][field] = value
            b.atomic(self.trust, changed)
            with self.assertRaises(ValueError):
                b.verify(directory, self.trust)

    def test_bundle_cannot_supply_its_own_trust(self):
        directory = bundle(self.root, self.keys)
        shutil.copyfile(self.trust, directory / "trust.json")
        with self.assertRaises(ValueError):
            b.verify(directory, directory / "trust.json")

    def test_duplicate_fields_unsupported_scope_and_unsafe_path(self):
        directory = bundle(self.root, self.keys)
        original = s.read(directory / "manifest.json")
        for field, value in (("schema", True), ("scope", "production"), ("sequence", 0), ("sequence", True),
                             ("frontend", {}), ("baseline_ids", ["../host"])):
            changed = {**original, field: value}
            with self.assertRaises(ValueError):
                b.validate_manifest(changed)
        changed = copy.deepcopy(original)
        changed["backend"]["file"] = "../image.tar.gz"
        with self.assertRaises(ValueError):
            b.validate_manifest(changed)
        with self.assertRaises(ValueError):
            b.loads(b'{"schema":1,"schema":1}')
        with self.assertRaises(ValueError):
            b.loads(b'{"value":NaN}')

    def test_signed_archive_with_wrong_image_config_is_rejected(self):
        directory = bundle(self.root, self.keys)
        manifest = s.read(directory / "manifest.json")
        manifest["backend"]["image_id"] = "sha256:" + "0" * 64
        sign(directory, self.keys, manifest)
        with self.assertRaisesRegex(ValueError, "image ID"):
            b.verify(directory, self.trust)

    def test_oci_index_identity_binds_the_selected_config_digest(self):
        directory = bundle(self.root, self.keys)
        manifest = s.read(directory / "manifest.json")
        with tarfile.open(directory / "backend-image.tar.gz", "r:gz") as archive:
            config = archive.extractfile("config.json").read()
        config_digest = "sha256:" + hashlib.sha256(config).hexdigest()
        node = b.dumps({"schemaVersion": 2, "config": {"digest": config_digest, "size": len(config)}, "layers": []})
        node_digest = "sha256:" + hashlib.sha256(node).hexdigest()
        index = b.dumps({"schemaVersion": 2, "manifests": [{"digest": node_digest, "size": len(node),
                                                          "platform": {"os": "linux", "architecture": "amd64"}}]})
        index_digest = "sha256:" + hashlib.sha256(index).hexdigest()
        entries = {"blobs/sha256/" + config_digest[7:]: config, "blobs/sha256/" + node_digest[7:]: node,
                   "blobs/sha256/" + index_digest[7:]: index,
                   "index.json": b.dumps({"manifests": [{"digest": index_digest, "size": len(index)}]}),
                   "manifest.json": b.dumps([{"Config": "blobs/sha256/" + config_digest[7:], "Layers": []}])}
        with tarfile.open(directory / "backend-image.tar.gz", "w:gz") as archive:
            for name, raw in entries.items():
                info = tarfile.TarInfo(name)
                info.size = len(raw)
                archive.addfile(info, io.BytesIO(raw))
        manifest["backend"].update(image_id=index_digest, sha256=b.sha(directory / "backend-image.tar.gz"),
                                   size=(directory / "backend-image.tar.gz").stat().st_size)
        sign(directory, self.keys, manifest)
        b.verify(directory, self.trust)
        manifest["backend"]["config_digest"] = "sha256:" + "0" * 64
        sign(directory, self.keys, manifest)
        with self.assertRaisesRegex(ValueError, "config digest"):
            b.verify(directory, self.trust)

    def test_archive_traversal_and_special_entries_rejected(self):
        for name, kind in (("../escape", tarfile.REGTYPE), ("/escape", tarfile.REGTYPE), ("link", tarfile.SYMTYPE)):
            directory = bundle(self.root, self.keys)
            with tarfile.open(directory / "backend-image.tar.gz", "w:gz") as archive:
                info = tarfile.TarInfo(name)
                info.type = kind
                archive.addfile(info, io.BytesIO())
            manifest = s.read(directory / "manifest.json")
            manifest["backend"].update(sha256=b.sha(directory / "backend-image.tar.gz"), size=(directory / "backend-image.tar.gz").stat().st_size)
            sign(directory, self.keys, manifest)
            with self.assertRaises(ValueError):
                b.verify(directory, self.trust)

    def test_sequence_floor_enforced(self):
        directory = bundle(self.root, self.keys)
        policy = s.read(self.trust)
        policy["minimum_sequence"] = 2
        b.atomic(self.trust, policy)
        with self.assertRaises(ValueError):
            b.verify(directory, self.trust)

    def test_install_update_rollback_preserves_new_data_and_high_water(self):
        first, second = bundle(self.root, self.keys, 1), bundle(self.root, self.keys, 2)
        driver = Driver(self.install)
        identity = (self.install / "installation.json").read_bytes()
        for directory in (first, second):
            self.assertEqual(s.change(self.install, directory, driver)["outcome"], "passed")
        (self.install / "data/new.txt").write_text("acquisition after update")
        self.assertEqual(s.change(self.install, None, driver, rollback=True)["outcome"], "passed")
        report = s.inventory(self.install)
        self.assertEqual(report["active"]["sequence"], 1)
        self.assertEqual(report["high_water"], 2)
        self.assertEqual((self.install / "data/new.txt").read_text(), "acquisition after update")
        self.assertEqual((self.install / "installation.json").read_bytes(), identity)
        self.assertIs(report["shipment_authorized"], False)
        with self.assertRaisesRegex(ValueError, "high-water"):
            s.change(self.install, second, driver)

    def test_incompatible_schema_or_baseline_refused_before_stopping(self):
        for mutation in ("schema", "baseline"):
            directory = bundle(self.root, self.keys, schemas={"data": 2, "config": 2} if mutation == "schema" else None)
            if mutation == "baseline":
                manifest = s.read(directory / "manifest.json")
                manifest["baseline_ids"] = ["another"]
                sign(directory, self.keys, manifest)
            driver = Driver(self.install)
            with self.assertRaises(ValueError):
                s.change(self.install, directory, driver)
            self.assertNotIn("stop", driver.calls)
            self.assertFalse((self.install / "pending.json").exists())

    def test_busy_runtime_not_stopped(self):
        driver = Driver(self.install, fail="idle")
        with self.assertRaises(RuntimeError):
            s.change(self.install, bundle(self.root, self.keys), driver)
        self.assertNotIn("stop", driver.calls)
        self.assertFalse((self.install / "maintenance.json").exists())

    def test_failed_validation_retains_stopped_maintenance_and_report(self):
        driver = Driver(self.install, fail="validate")
        receipt = s.change(self.install, bundle(self.root, self.keys), driver)
        self.assertEqual(receipt["outcome"], "failed")
        self.assertFalse(driver.running)
        self.assertTrue((self.install / "pending.json").exists())
        self.assertEqual(s.inventory(self.install)["state"], "maintenance")
        self.assertNotIn("fixture secret", json.dumps(receipt))

    def test_interruption_requires_recover_before_compatible_rollback(self):
        first, second = bundle(self.root, self.keys, 1), bundle(self.root, self.keys, 2)
        driver = Driver(self.install)
        s.change(self.install, first, driver)
        class Interrupted(Driver):
            def switch(self, *args):
                raise KeyboardInterrupt()
        receipt = s.change(self.install, second, Interrupted(self.install))
        self.assertEqual(receipt["outcome"], "interrupted")
        with self.assertRaises(ValueError):
            s.change(self.install, None, driver, rollback=True)
        s.recover(self.install, driver)
        self.assertEqual(s.change(self.install, None, driver, rollback=True)["outcome"], "passed")
        self.assertEqual(s.inventory(self.install)["active"]["sequence"], 1)

    def test_no_automatic_initial_install_rollback(self):
        driver = Driver(self.install, fail="validate")
        s.change(self.install, bundle(self.root, self.keys), driver)
        driver.fail = None
        s.recover(self.install, driver)
        with self.assertRaisesRegex(ValueError, "no previous"):
            s.change(self.install, None, driver, rollback=True)

    def test_history_receipt_identity_and_active_state_corruption_detected(self):
        directory = bundle(self.root, self.keys)
        s.change(self.install, directory, Driver(self.install))
        active = s.read(self.install / "state.json")
        active["high_water"] += 1
        b.atomic(self.install / "state.json", active)
        with self.assertRaises(ValueError):
            s.inventory(self.install)
        identity = s.read(self.install / "installation.json")
        identity["pc_serial"] = "changed"
        b.atomic(self.install / "installation.json", identity)
        with self.assertRaises(ValueError):
            s.instance(self.install)
        with (self.install / "history.jsonl").open("ab") as stream:
            stream.write(b'{"previous":"bad"}\n')
        with self.assertRaises(ValueError):
            s.history(self.install)

    def test_changed_trust_policy_and_r10_roots_refused(self):
        original = s.read(self.trust)
        original["minimum_sequence"] += 1
        b.atomic(self.trust, original)
        with self.assertRaises(ValueError):
            s.instance(self.install)
        b.atomic(self.install / "installation.json", {"installation_id": "R10"})
        with self.assertRaises(ValueError):
            s.instance(self.install)

    def test_missing_signature_tool_is_a_failure(self):
        directory = bundle(self.root, self.keys)
        with patch("release_bundle.subprocess.run", side_effect=FileNotFoundError("missing")):
            with self.assertRaises(FileNotFoundError):
                b.verify(directory, self.trust)

    def test_stationary_homing_required_is_safe_but_motion_and_other_faults_refused(self):
        from release_runtime import stationary
        for status, moving, expected in (({"state": "idle"}, False, True),
                                        ({"state": "alarm", "reason": "homing required"}, False, True),
                                        ({"state": "alarm", "reason": "limit"}, False, False),
                                        ({"state": "moving"}, True, False), ({"state": "idle"}, True, False)):
            client = Mock()
            client.json.side_effect = lambda path: moving if path.endswith("/moving") else status
            self.assertIs(stationary(client), expected)

    def test_altered_operation_receipt_detected(self):
        receipt = s.change(self.install, bundle(self.root, self.keys), Driver(self.install))
        path = self.install / "operations" / receipt["operation_id"] / "receipt.json"
        path.write_text("altered", encoding="utf-8")
        with self.assertRaises(ValueError):
            s.inventory(self.install)


if __name__ == "__main__":
    unittest.main()
