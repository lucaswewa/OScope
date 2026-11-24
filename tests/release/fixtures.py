"""Tiny signed archives and a file-backed driver for portable transaction tests."""
import hashlib
import io
import json
from pathlib import Path
import shutil
import tarfile
import uuid

import release_bundle as b
import release_state as s


def bundle(root, trust_dir, sequence=1, schemas=None):
    directory = root / str(uuid.uuid4())
    directory.mkdir()
    config = {"os": "linux", "architecture": "amd64", "config": {"User": "1000:1000", "Labels": {"org.opencontainers.image.revision": "a" * 40}},
              "fixture_sequence": sequence}
    raw = b.dumps(config)
    image = hashlib.sha256(raw).hexdigest()
    with tarfile.open(directory / "backend-image.tar.gz", "w:gz") as archive:
        for name, data in (("config.json", raw), ("manifest.json", b.dumps([{"Config": "config.json", "Layers": []}]))):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
    (directory / "LICENSE").write_text("fixture license\n")
    def artifact(name):
        return {"file": name, "sha256": b.sha(directory / name), "size": (directory / name).stat().st_size}
    manifest = {"schema": 1, "scope": b.SCOPE, "release_id": "fixture-" + str(sequence), "sequence": sequence,
                "key_id": "lab", "baseline_ids": ["lab-only"], "schemas": schemas or {"data": 1, "config": 2},
                "backend": {**artifact("backend-image.tar.gz"), "image_id": "sha256:" + image, "config_digest": "sha256:" + image, "revision": "a" * 40,
                            "platform": "linux/amd64"}, "license": artifact("LICENSE"), "frontend": None}
    sign(directory, trust_dir, manifest)
    return directory


def sign(directory, trust_dir, manifest):
    (directory / "manifest.json").write_bytes(b.dumps(manifest))
    b.openssl("pkeyutl", "-sign", "-rawin", "-inkey", trust_dir / "lab-private.pem", "-in", directory / "manifest.json", "-out", directory / "manifest.sig")


def installation(root, keys):
    root.mkdir()
    (root / "trust").mkdir()
    for name in ("trust.json", "lab-public.pem"):
        shutil.copyfile(keys / name, root / "trust" / name)
    b.atomic(root / "installation.json", {"schema": 1, "scope": b.SCOPE, "installation_id": str(uuid.uuid4()),
             "instrument_serial": "ENG-001", "pc_serial": "PC-001", "baseline_id": "lab-only", "port": 18210,
             "linux_versions": {"fixture": "not_host_qualification"}, "trust_sha256": b.sha(root / "trust/trust.json")})
    (root / "data").mkdir()
    (root / "data/keep.txt").write_text("existing acquisition")
    s.record(root, {"type": "initialized", "time": s.now(), "identity_sha256": b.sha(root / "installation.json")})


class Driver:
    def __init__(self, root, fail=None):
        self.root, self.fail, self.running = root, fail, False
        self.calls = []

    def hit(self, name):
        self.calls.append(name)
        if name == self.fail:
            raise RuntimeError("fixture secret must not enter receipt")

    def prepare_recovery(self, *args):
        self.hit("prepare_recovery")

    def prepare(self, *args):
        self.hit("prepare")

    def idle(self):
        self.hit("idle")

    def stop(self):
        self.hit("stop")
        self.running = False

    def snapshot(self, operation):
        self.hit("snapshot")
        return {"fixture": True, "automatic_restore": False}

    def switch(self, *args):
        self.hit("switch")
        self.running = True

    def validate(self, *args):
        self.hit("validate")
        return {"fixture": True, "hardware": "not_run"}
