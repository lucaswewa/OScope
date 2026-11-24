import copy
import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import uuid

ROOT = Path(__file__).resolve().parents[2]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


runtime = module("provision_runtime", "provision-runtime.py")
template = module("provision_template", "provision-template.py")


def identity():
    return {"installation_id": str(uuid.uuid4()), "instrument_serial": "fixture-001", "pc_serial": "pc-001",
            "baseline_id": "candidate-fixture", "image_id": "sha256:" + "a" * 64, "port": 8220, "compute": "cuda",
            "linux_versions": {name: "fixture" for name in ("ubuntu_release", "kernel_release", "docker_version", "compose_version", "nvidia_ctk_version")}}


class Provisioning(unittest.TestCase):
    def test_root_rejects_system_paths_and_docker_mount_separators(self):
        for path in (Path("relative"), Path("/"), Path("/etc"), Path("/tmp/unsafe,src=elsewhere"), Path("/tmp/line\nbreak")):
            with self.subTest(path=str(path)), self.assertRaises(ValueError):
                runtime.validate_root(path)

    def test_identity_rejects_mutable_images_bad_ports_and_unexpected_fields(self):
        for field, value in [("image_id", "oscope:latest"), ("port", True), ("port", 80),
                             ("installation_id", "another-host"), ("instrument_serial", "../other"),
                             ("compute", "auto"), ("image_id", None)]:
            changed = identity()
            changed[field] = value
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                runtime.validate_identity(changed)
        changed = identity()
        changed["password"] = "must-not-be-in-public-identity"
        with self.assertRaises(ValueError):
            runtime.validate_identity(changed)

    def test_repeated_reservation_preserves_identity_and_existing_data(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(runtime.os, "chown", create=True):
            root = Path(tmp)
            value = identity()
            runtime.reserve(root, value)
            (root / "data/record.fixture").write_bytes(b"preserve acquisition data")
            (root / "data/certificate.fixture").write_bytes(b"preserve TLS identity")
            runtime.reserve(root, value)
            before = (root / "identity.json").read_bytes()
            changed = {**value, "pc_serial": "another-pc"}
            with self.assertRaises(ValueError):
                runtime.reserve(root, changed)
            self.assertEqual((root / "identity.json").read_bytes(), before)
            self.assertEqual((root / "data/record.fixture").read_bytes(), b"preserve acquisition data")
            self.assertEqual((root / "data/certificate.fixture").read_bytes(), b"preserve TLS identity")

    def test_foreign_directory_is_not_adopted(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "foreign.json").write_text("existing installation")
            with self.assertRaises(ValueError):
                runtime.reserve(root, identity())
            self.assertFalse((root / "identity.json").exists())

    def test_engineering_bootstrap_rejects_physical_device_configuration(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path = root / "data/config/oscope.toml"
            path.parent.mkdir(parents=True)
            path.write_text('[devices]\ncamera="real"\nstage="sim"\nillumination="sim"\nobjective="sim"\n')
            with self.assertRaises(ValueError):
                runtime.engineering_config(root)
            path.write_text(path.read_text().replace('camera="real"', 'camera="sim"'))
            runtime.engineering_config(root)

    def test_container_ownership_checks_all_important_bindings(self):
        value = identity()
        root = Path("/qualified-fixture")
        info = {"Image": value["image_id"], "Config": {"User": "1000:1000", "Labels": {"oscope.installation": value["installation_id"]}},
                "HostConfig": {"ReadonlyRootfs": True, "IpcMode": "private", "Privileged": False, "CapDrop": ["ALL"],
                               "SecurityOpt": ["no-new-privileges"], "RestartPolicy": {"Name": "unless-stopped"},
                               "PortBindings": {"8210/tcp": [{"HostIp": "127.0.0.1", "HostPort": "8220"}]},
                               "DeviceRequests": [{"Driver": "nvidia"}], "Devices": []},
                "Mounts": [{"Destination": f"/var/lib/oscope/{name}", "Source": str(root / name)} for name in ("data", "scratch")]}
        runtime.validate_container(info, value, root)
        for section, key, replacement in [("HostConfig", "ReadonlyRootfs", False), ("HostConfig", "IpcMode", "host"),
                                          ("HostConfig", "Privileged", True), ("HostConfig", "DeviceRequests", []),
                                          ("HostConfig", "Devices", [{"PathOnHost": "/dev/ttyUSB0"}]),
                                          ("Config", "Labels", {"oscope.installation": "another"})]:
            changed = copy.deepcopy(info)
            changed[section][key] = replacement
            with self.subTest(key=key), self.assertRaises(ValueError):
                runtime.validate_container(changed, value, root)
        changed = copy.deepcopy(info)
        changed["Mounts"][0]["Source"] = "/another-instrument/data"
        with self.assertRaises(ValueError):
            runtime.validate_container(changed, value, root)

    def test_maintenance_does_not_restart_container_or_clear_its_marker(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            value = identity()
            runtime.atomic_json(root / "identity.json", value)
            runtime.atomic_json(root / "initialized.json", {"installation_id": value["installation_id"]})
            (root / "maintenance").touch()
            # No Docker call is allowed while the maintenance marker exists.
            from contextlib import nullcontext
            with patch.object(runtime, "locked", return_value=nullcontext()), patch.object(runtime, "docker", side_effect=AssertionError("unexpected Docker call")):
                result = runtime.supervise(root, value, cycles=1)
            self.assertEqual(result["state"], "maintenance")
            self.assertTrue((root / "maintenance").exists())

    def test_new_hosts_have_distinct_installation_identity(self):
        self.assertNotEqual(identity()["installation_id"], identity()["installation_id"])

    def test_linux_version_drift_blocks_startup(self):
        value = identity()
        with patch.object(runtime, "linux_versions", return_value={**value["linux_versions"], "kernel_release": "changed"}):
            with self.assertRaises(ValueError):
                runtime.verify_versions(value)


def write_template(path, changes=None, extra=None):
    files = {"etc/machine-id": b"", "etc/shadow": b"root:!:0:0:99999:7:::\n",
             "etc/wsl.conf": b"[boot]\nsystemd=true\n[interop]\nappendWindowsPath=false\n", "etc/os-release": b'ID=ubuntu\nVERSION_ID="24.04"\n',
             "etc/oscope-host-baseline.json": b'{"schema_version":1,"baseline_id":"candidate-fixture"}'}
    files.update(changes or {})
    with tarfile.open(path, "w") as archive:
        for name, data in files.items():
            item = tarfile.TarInfo(name)
            item.size = len(data)
            archive.addfile(item, io.BytesIO(data))
        if extra:
            archive.addfile(extra)


class TemplateGuard(unittest.TestCase):
    def test_clean_fixture_passes_only_the_guard_not_host_qualification(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "rootfs.tar"
            write_template(path)
            result = template.audit(path, "candidate-fixture")
            self.assertTrue(result["ready"], result["issues"])
            self.assertEqual(result["qualification"], "pending")
            self.assertEqual(result["scope"], "template_guard_only")

    def test_configured_instrument_template_is_rejected_without_extraction(self):
        cases = [{"etc/machine-id": b"a-previous-machine-id"}, {"etc/shadow": b"operator:$6$existing-password-hash::::::\n"},
                 {"var/lib/oscope/data/accounts.db": b"account state"}, {"root/.ssh/id_ed25519": b"key"},
                 {"var/lib/docker/volumes/state/file": b"customer data"}, {"root/.bash_history": b"history"},
                 {"etc/wsl.conf": b"[boot]\nsystemd=false\n"}, {"etc/oscope-host-baseline.json": b'{}'}]
        with tempfile.TemporaryDirectory() as tmp:
            for index, changed in enumerate(cases):
                path = Path(tmp) / f"rootfs-{index}.tar"
                write_template(path, changed)
                with self.subTest(changed=list(changed)):
                    self.assertFalse(template.audit(path, "candidate-fixture")["ready"])

    def test_unsafe_paths_and_state_symlinks_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            for index, name in enumerate(("../outside", "/absolute", "var/lib/oscope-host", "root/.ssh")):
                item = tarfile.TarInfo(name)
                item.type = tarfile.SYMTYPE
                item.linkname = "/somewhere"
                path = Path(tmp) / f"rootfs-{index}.tar"
                write_template(path, extra=item)
                self.assertFalse(template.audit(path, "candidate-fixture")["ready"])


if __name__ == "__main__":
    unittest.main()
