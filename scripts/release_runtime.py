"""Linux engineering runtime adapter, using R10 isolation and R07 trusted TLS."""
import importlib.util
import json
import os
from pathlib import Path
import platform
import sqlite3
import tarfile
import tomllib

import release_bundle as b

ROOT = Path(__file__).resolve().parents[1]


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / filename)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def stationary(client):
    status = client.json("/api/v1/stage/status")
    expected = status == {"state": "idle"} or status == {"state": "alarm", "reason": "homing required"}
    return expected and client.json("/api/v1/stage/moving") is False


class Runtime:
    def __init__(self, root, password=None):
        self.root, self.password = root, password
        self.p = module("release_provision_runtime", "provision-runtime.py")
        self.r = module("release_tls_client", "check-runtime.py")
        self.p.container_name = lambda identity: "oscope-r12-" + identity["installation_id"].replace("-", "")
        self.identity = None
        self.allowed = set()
        self.stable = None

    def mapped(self, image):
        return {k: self.identity[k] for k in ("installation_id", "instrument_serial", "pc_serial", "baseline_id", "port", "linux_versions")} | {
            "image_id": image, "compute": "cuda"}

    def prepare_recovery(self, identity, current, pending):
        self.identity = identity
        self.is_initial = current["active"] is None
        for value in (current["active"], current["previous"], pending["target"], pending["before"]["active"]):
            if value:
                self.allowed.add(value["image_id"])

    def inspect(self):
        name = self.p.container_name(self.identity)
        result = self.p.docker("container", "inspect", name, check=False)
        if result.returncode:
            names = self.p.docker("ps", "-a", "--format", "{{.Names}}").stdout.splitlines()
            b.check(name not in names, "cannot inspect owned runtime")
            return None
        info = b.loads(result.stdout)[0]
        b.check(info["Image"] in self.allowed, "runtime has an unexpected image")
        self.p.validate_container(info, self.mapped(info["Image"]), self.root)
        return info

    def schemas(self):
        path = self.root / "data/config/oscope.toml"
        if path.exists():
            config = tomllib.loads(path.read_text(encoding="utf-8"))
            b.check(config["version"] == 2, "configuration schema differs")
            self.p.engineering_config(self.root)
        database = self.root / "data/index.sqlite"
        if database.exists():
            with sqlite3.connect(database.as_uri() + "?mode=ro", uri=True) as connection:
                b.check(connection.execute("SELECT version FROM store_schema").fetchall() == [(1,)], "data schema differs")

    def prepare(self, identity, current, target, cached):
        b.check(platform.system() == "Linux" and platform.machine() == "x86_64" and os.geteuid() == 0,
                "Linux x86_64 root adapter required")
        self.identity = identity
        self.is_initial = current["active"] is None
        self.allowed.update(v["image_id"] for v in (current["active"], current["previous"], target) if v)
        b.check(not any((self.root / name).is_symlink() for name in ("data", "scratch")), "state directory symlink refused")
        self.p.verify_versions(self.mapped(target["image_id"]))
        self.schemas()
        # verify/cache happened before this import; no pull/build/network package fetch.
        self.p.docker("image", "load", "--input", str(cached / "backend-image.tar.gz"))
        image = b.loads(self.p.docker("image", "inspect", target["image_id"]).stdout)[0]
        b.check((image["Os"], image["Architecture"], image["Config"]["User"]) == ("linux", "amd64", "1000:1000"), "runtime identity differs")
        b.check(image["Config"]["Labels"]["org.opencontainers.image.revision"] == target["revision"], "runtime revision differs")
        if current["active"] is None:
            for name in ("data", "scratch"):
                path = self.root / name
                path.mkdir(exist_ok=True, mode=0o700)
                os.chown(path, 1000, 1000)
            self.p.account(self.mapped(target["image_id"]), self.root, self.password or "")
            self.p.engineering_config(self.root)
        self.inspect()  # Refuse a foreign/unexpected container before maintenance.

    def connect(self):
        c = self.r.Client(self.identity["port"], self.root / "data/tls/certificate.pem")
        c.token = (self.root / "data/run/local.token").read_text().strip()
        b.check(c.json("/ready") == {"status": "ready"}, "trusted authenticated readiness failed")
        return c

    def idle(self):
        info = self.inspect()
        if info is None:
            return
        b.check(info["State"]["Running"], "current runtime is stopped; diagnose it before updating")
        c = self.connect()
        c.action("auth/acquire_control", {})
        try:
            b.check(c.json("/api/v1/camera/streaming") is False, "stop streaming before update")
            b.check(stationary(c), "stage must be stationary without an unexpected fault")
            b.check(c.json("/api/v1/jobs/queue") == [], "finish/cancel all queued work before update")
        except Exception:
            c.action("auth/release_control", {})
            raise
        # Keep the lease until shutdown, so another account cannot start acquisition.

    def stop(self):
        info = self.inspect()
        if info and info["State"]["Running"]:
            self.p.docker("stop", "--time", "30", self.p.container_name(self.identity))

    def metadata(self):
        data = self.root / "data"
        paths = []
        for name in ("config", "calibration", "settings", "tls", "accounts.json"):
            path = data / name
            b.check(not path.is_symlink(), "metadata symlink refused")
            if path.is_dir():
                paths.extend(p for p in path.rglob("*") if p.is_file())
            elif path.is_file():
                paths.append(path)
        b.check(not any(p.is_symlink() for p in paths), "metadata symlink refused")
        return {p.relative_to(data).as_posix(): b.sha(p) for p in sorted(paths)}

    def snapshot(self, operation):
        info = self.inspect()
        b.check(info is None or (not info["State"]["Running"] and info["State"]["ExitCode"] == 0), "clean shutdown required for metadata backup")
        self.schemas()
        self.stable = self.metadata()
        files = {self.root / "data" / name for name in self.stable}
        files.update(p for p in (self.root / "data").rglob("*.sqlite*") if p.is_file())
        b.check(not any(p.is_symlink() for p in files) and sum(p.stat().st_size for p in files) <= 256 * 1024**2,
                "metadata backup limit/path violation")
        manifest = {p.relative_to(self.root / "data").as_posix(): b.sha(p) for p in sorted(files)}
        backup = operation / "private-metadata.tar.gz"
        with tarfile.open(backup, "x:gz") as archive:
            for path in sorted(files):
                archive.add(path, arcname=path.relative_to(self.root / "data").as_posix(), recursive=False)
        backup.chmod(0o600)
        b.atomic(operation / "backup-files.json", manifest)
        return {"archive_sha256": b.sha(backup), "metadata_files": len(manifest), "automatic_restore": False}

    def switch(self, target, cached):
        info = self.inspect()
        if info:
            b.check(not info["State"]["Running"], "runtime still running")
            self.p.docker("rm", self.p.container_name(self.identity))
        self.p.ensure_container(self.mapped(target["image_id"]), self.root)

    def validate(self, target):
        self.p.health(self.mapped(target["image_id"]), self.root)
        c = self.connect()
        b.check(c.json("/api/v1/system/git_commit") == target["revision"][:12], "API/image revision differs")
        self.p.engineering_config(self.root)
        diagnostic = self.p.docker("exec", self.p.container_name(self.identity), "/usr/local/bin/oscope-cuda-check", "--require-cuda", "--smoke")
        b.check("Cuda  available" in diagnostic.stdout and "smoke: ok" in diagnostic.stdout, "required CUDA kernel did not execute")
        self.schemas()
        actual = self.metadata()
        preserved = all(actual.get(k) == v for k, v in self.stable.items()) if self.is_initial else self.stable == actual
        b.check(preserved, "credentials/config/calibration changed unexpectedly")
        b.check(c.json("/api/v1/camera/streaming") is False and c.json("/api/v1/jobs/queue") == [] and stationary(c),
                "startup resumed acquisition or stage is not stationary")
        return {"cuda": "actual_kernels_executed", "tls": "trusted_and_authenticated", "image_revision": "verified",
                "metadata": "unchanged", "motion_and_acquisition": "not_resumed", "devices": "simulated"}
