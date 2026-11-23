"""Synthetic inventory/ELF fixtures; these never qualify physical hardware."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("hardware_linux", ROOT / "scripts/hardware-linux.py")
inventory = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(inventory)


def profile():
    return {"version": 1, "camera": {"vid": "1234", "pid": "abcd", "serial": "camera-fixture"},
            "stage": {"vid": "5678", "pid": "efab", "serial": "stage-fixture"},
            "sdk": {"library": "", "sha256": ""}}


def device(role):
    return {**profile()[role], "node": {"character_device": True, "apparent_read_write_access": True}}


class HardwareInventory(unittest.TestCase):
    def test_template_is_explicitly_incomplete(self):
        config, digest = inventory.load_profile(ROOT / "configs/hardware.example.toml")
        result = inventory.check_prerequisites(config, [], inventory.sdk_inventory(config["sdk"]),
                                               "Linux", "x86_64", 1000)
        self.assertFalse(result["ready"])
        self.assertEqual(len(digest), 64)
        self.assertTrue(any("camera" in message for message in result["issues"]))
        self.assertTrue(any("stage" in message for message in result["issues"]))

    def test_profile_rejects_typos_invalid_identity_and_same_device_for_both_roles(self):
        original = (ROOT / "configs/hardware.example.toml").read_text()
        cases = [original.replace("version = 1", "version = true"),
                 original.replace('vid = ""', 'vid = "12345"', 1),
                 original.replace('[camera]', '[camrea]'),
                 original.replace('library = ""', 'library = "relative.so"'),
                 original.replace('sha256 = ""', 'sha256 = "not-a-digest"'),
                 original.replace('serial = ""', 'serial = " padded "', 1),
                 original.replace('vid = ""', 'vid = "1234"').replace('pid = ""', 'pid = "abcd"')
                 .replace('serial = ""', 'serial = "same"')]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "profile.toml"
            for text in cases:
                path.write_text(text)
                with self.subTest(text=text), self.assertRaises(ValueError):
                    inventory.load_profile(path)

    def test_matches_stable_identity_after_bus_number_changes(self):
        devices = [device("camera"), device("stage")]
        result = inventory.check_prerequisites(profile(), devices, {"issues": []}, "Linux", "x86_64", 1000)
        self.assertTrue(result["ready"])  # Synthetic prerequisite check, not qualification.
        devices[0].update(bus=99, device=88)
        self.assertTrue(inventory.check_prerequisites(profile(), devices, {"issues": []},
                                                      "Linux", "x86_64", 1000)["ready"])
        devices[0]["serial"] = "another-camera"
        self.assertFalse(inventory.check_prerequisites(profile(), devices, {"issues": []},
                                                       "Linux", "x86_64", 1000)["ready"])

    def test_missing_ambiguous_and_inaccessible_devices_fail(self):
        for devices in ([], [device("camera"), device("camera"), device("stage")],
                        [{**device("camera"), "node": {"character_device": True,
                                                      "apparent_read_write_access": False}}, device("stage")]):
            with self.subTest(devices=devices):
                self.assertFalse(inventory.check_prerequisites(profile(), devices, {"issues": []},
                                                               "Linux", "x86_64", 1000)["ready"])

    def test_root_wrong_platform_and_wrong_architecture_do_not_qualify_permissions(self):
        for system, machine, uid in [("Linux", "x86_64", 0), ("Windows", "AMD64", None),
                                     ("Linux", "aarch64", 1000)]:
            with self.subTest(system=system, machine=machine, uid=uid):
                result = inventory.check_prerequisites(profile(), [device("camera"), device("stage")],
                                                       {"issues": []}, system, machine, uid)
                self.assertFalse(result["ready"])

    def test_sysfs_fixture_reads_identity_and_handles_disconnect(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            entry = root / "usb/1-4"
            entry.mkdir(parents=True)
            for name, value in {"idVendor": "1234", "idProduct": "abcd", "serial": "camera-fixture",
                                "busnum": "1", "devnum": "7", "speed": "5000"}.items():
                (entry / name).write_text(value)
            devices, errors = inventory.usb_inventory(root / "usb", root / "dev")
            self.assertFalse(errors)
            self.assertEqual(devices[0]["serial"], "camera-fixture")
            self.assertEqual(devices[0]["node"]["path"], str(root / "dev/001/007"))
            self.assertFalse(devices[0]["node"]["character_device"])
            (entry / "devnum").unlink()
            devices, errors = inventory.usb_inventory(root / "usb", root / "dev")
            self.assertFalse(devices)
            self.assertTrue(errors)

    def test_regular_file_cannot_stand_in_for_a_usb_device(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "fake-node"
            path.write_bytes(b"not hardware")
            self.assertFalse(inventory.node_status(path)["character_device"])

    def test_static_sdk_check_rejects_dll_wrong_architecture_and_digest_mismatch(self):
        header = bytearray(64)
        header[:7] = b"\x7fELF\x02\x01\x01"
        struct.pack_into("<HH", header, 16, 3, 62)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sdk-fixture.so"
            path.write_bytes(header)
            settings = {"library": str(path), "sha256": hashlib.sha256(header).hexdigest()}
            result = inventory.sdk_inventory(settings)
            self.assertFalse(result["issues"])
            self.assertFalse(result["loaded"])
            self.assertTrue(result["elf64_x86_64_shared"])
            changed = copy.deepcopy(settings)
            changed["sha256"] = "0" * 64
            self.assertTrue(inventory.sdk_inventory(changed)["issues"])
            changed["sha256"] = ""
            self.assertTrue(inventory.sdk_inventory(changed)["issues"])
            struct.pack_into("<H", header, 18, 183)  # AArch64 is not x86_64.
            for data in (header, b"MZ" + b"\0" * 62, b"\x7fELF"):
                path.write_bytes(data)
                self.assertFalse(inventory.sdk_inventory(settings)["elf64_x86_64_shared"])

    def test_report_is_append_only_and_never_claims_physical_qualification(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / "report.json"
            run = subprocess.run([sys.executable, str(ROOT / "scripts/hardware-linux.py"),
                                  "--profile", str(ROOT / "configs/hardware.example.toml"),
                                  "--report", str(target), "--require-ready"], capture_output=True, text=True)
            self.assertEqual(run.returncode, 2, run.stderr)
            report = json.loads(target.read_text(encoding="utf-8"))
            self.assertEqual(report["qualification"]["status"], "not_run")
            self.assertEqual(report["scope"], "inventory_only")
            self.assertFalse(report["preflight"]["ready"])
            self.assertFalse(report["sdk"]["loaded"])
            before = target.read_bytes()
            with self.assertRaises(FileExistsError):
                inventory.write_report({"replacement": True}, str(target))
            self.assertEqual(target.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
