#!/usr/bin/env python3
"""Explicit offline/CUDA update lab. Never selects an existing instrument."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import uuid

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import release_bundle as b
import release_state as s
import release as cli
from release_runtime import Runtime


def crash_worker(root, directory):
    class Crash(Runtime):
        def stop(self):
            super().stop()
            os._exit(91)  # Hard process exit after real shutdown; no Python finally/receipt.
    with s.locked(root):
        s.change(root, directory, Crash(root))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="oscope-backend:r07")
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/r12")
    parser.add_argument("--reuse-bundles", type=Path, help="Existing lab UUID directory; verifies both signed bundles before reuse")
    parser.add_argument("--crash-root", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--crash-bundle", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.crash_root:
        crash_worker(args.crash_root, args.crash_bundle)
        return
    b.check(sys.platform == "linux" and os.geteuid() == 0, "run lab on Linux as root")
    work = args.output.resolve() / str(uuid.uuid4())
    work.mkdir(parents=True, exist_ok=False, mode=0o700)
    print("Lab directory: " + str(work), flush=True)
    source = args.reuse_bundles.resolve() if args.reuse_bundles else work
    root, keys = work / "installation", source / "keys"
    candidate_tag = "oscope-r12-lab:" + work.name
    report = {"schema": 1, "run_id": work.name, "scope": b.SCOPE, "status": "in_progress", "checks": [],
              "physical_hardware": "not_run", "windows_lifecycle": "not_run", "schema_migration": "not_run",
              "product_qualification": "pending", "candidate": "metadata-only; identical application code"}
    driver = None
    try:
        if not args.reuse_bundles:
            image = b.loads(b.docker("image", "inspect", args.image))[0]
            b.lab_keygen(keys)
            build = work / "empty-build"
            build.mkdir()
            (build / "Dockerfile").write_text("FROM " + args.image + "\nLABEL org.oscope.engineering_candidate=" + work.name + "\n")
            b.docker("build", "--network", "none", "--pull=false", "-t", candidate_tag, str(build))
            second_image = b.loads(b.docker("image", "inspect", candidate_tag))[0]["Id"]
            b.check(second_image != image["Id"], "lab must use distinct immutable image IDs")
            report["images"] = [image["Id"], second_image]
            for index, selected in enumerate((image["Id"], second_image), 1):
                b.pack(source / ("bundle-" + str(index)), selected, "r12-lab-" + str(index), index, "r12-lab-only",
                       keys / "lab-private.pem", "lab", keys / "trust.json")
        else:
            manifests = [b.verify(source / ("bundle-" + str(i)), keys / "trust.json")[0] for i in (1, 2)]
            report["images"] = [m["backend"]["image_id"] for m in manifests]
            b.check(report["images"][0] != report["images"][1], "distinct immutable images required")
        report["bundle_source"] = str(source)
        report["checks"].append("real Ed25519 signatures and compressed Docker save archives")
        # Remove only the lab's unused metadata-only image so install imports it offline.
        if not args.reuse_bundles:
            b.docker("image", "rm", candidate_tag)
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        cli.initialize(root, keys / "trust.json", "R12-SIM-001", "R12-LAB-PC", "r12-lab-only", port)
        password = secrets.token_urlsafe(32)
        driver = Runtime(root, password)
        with s.locked(root):
            receipt = s.change(root, source / "bundle-1", driver)
        b.check(receipt["outcome"] == "passed", "initial install failed; inspect private receipt")
        initial_identity = (root / "installation.json").read_bytes()
        certificate = b.sha(root / "data/tls/certificate.pem")
        c = driver.connect()
        c.login("operator", password)
        c.action("auth/acquire_control", {})
        r = driver.r
        scientific = r.runtime_workflows.check(c, r.frame, r.send_text, r.until, True)
        calibration = c.json("/api/v1/calibration/active")
        c.action("camera/start_streaming", {})
        c.action("auth/release_control", {})
        try:
            with s.locked(root):
                s.change(root, source / "bundle-2", Runtime(root))
            raise AssertionError("busy update was not refused")
        except ValueError as error:
            b.check("streaming" in str(error), "unexpected idle refusal")
        b.check(not (root / "pending.json").exists(), "idle refusal must leave active runtime untouched")
        c.action("auth/acquire_control", {})
        c.action("camera/stop_streaming", {})
        c.action("auth/release_control", {})
        report["checks"].append("streaming update refused before shutdown")
        child = subprocess.run([sys.executable, str(Path(__file__).resolve()), "--crash-root", str(root),
                                "--crash-bundle", str(source / "bundle-2")], timeout=300, capture_output=True)
        b.check(child.returncode == 91 and (root / "pending.json").exists(), "hard-exit injection did not reach stopped transaction")
        b.check(not RuntimeForRecovery(root).inspect()["State"]["Running"], "interrupted runtime must be stopped")
        with s.locked(root):
            s.recover(root, Runtime(root))
            receipt = s.change(root, None, Runtime(root), rollback=True)
        b.check(receipt["outcome"] == "passed", "recovery rollback failed")
        report["checks"].append("hard process exit after shutdown; durable maintenance; explicit compatible rollback")
        with s.locked(root):
            receipt = s.change(root, source / "bundle-2", Runtime(root))
        b.check(receipt["outcome"] == "passed", "update failed")
        driver = RuntimeForRecovery(root)
        c = driver.connect()
        c.login("operator", password)
        r.runtime_workflows.verify(c, scientific)
        b.check(c.json("/api/v1/calibration/active") == calibration, "calibration lost")
        # Add a new acquisition after the update, then prove rollback retains it.
        c.action("auth/acquire_control", {})
        c.action("camera/start_streaming", {})
        project = scientific["project_path"].split("/")[-1]
        href, capture = r.runtime_workflows.completed_job(c, c.action("capture/single", {
            "project": project, "name": "after update", "rendition": "png"}), r.until)
        new_acquisition = r.runtime_workflows.record_id(capture)
        c.action("camera/stop_streaming", {})
        c.action("auth/release_control", {})
        # Hash all retained project payloads, not just the initial report outputs.
        payloads = {p.relative_to(root / "data").as_posix(): b.sha(p) for p in (root / "data").rglob("*")
                    if p.is_file() and p.suffix.lower() in (".png", ".jpg", ".tif", ".pdf")}
        b.check(bool(payloads), "no acquired payloads to verify")
        with s.locked(root):
            receipt = s.change(root, None, Runtime(root), rollback=True)
        b.check(receipt["outcome"] == "passed", "rollback failed")
        driver = RuntimeForRecovery(root)
        c = driver.connect()
        c.login("operator", password)
        r.runtime_workflows.verify(c, scientific)
        b.check(any(a["id"] == new_acquisition for a in c.json(scientific["project_path"])["acquisitions"]), "new acquisition disappeared")
        b.check(c.json(href)["state"] == "completed", "new job disappeared")
        for name, expected in payloads.items():
            b.check(b.sha(root / "data" / name) == expected, "rollback changed a project payload")
        b.check((root / "installation.json").read_bytes() == initial_identity and b.sha(root / "data/tls/certificate.pem") == certificate,
                "identity/TLS changed")
        report["checks"].append("distinct-image update and rollback preserve credentials, identity, calibration, jobs and old/new acquisitions")
        try:
            with s.locked(root):
                s.change(root, source / "bundle-2", Runtime(root))
            raise AssertionError("sequence replay accepted")
        except ValueError as error:
            b.check("high-water" in str(error), "unexpected replay refusal")
        report["checks"].append("normal replay refused after rollback; high-water mark retained")
        inventory = s.inventory(root)
        (work / "inventory.json").write_bytes(b.dumps(inventory))
        report.update(status="passed", inventory=inventory, project_payload_hashes=payloads,
                      source_sha256={name: b.sha(ROOT / "scripts" / name) for name in
                                     ("release.py", "release_bundle.py", "release_state.py", "release_runtime.py")})
    except (Exception, KeyboardInterrupt) as error:
        report.update(status="failed", error_type=type(error).__name__)
        # Local developer trace contains no secret-valued command arguments.
        import traceback
        traceback.print_exc()
    finally:
        if (root / "installation.json").exists():
            try:
                driver = RuntimeForRecovery(root)
                info = driver.inspect()
                if info:
                    driver.stop()
                    driver.p.docker("rm", driver.p.container_name(driver.identity))
                report["cleanup"] = "owned container removed; images, keys and synthetic state retained"
            except Exception as error:
                report.update(status="failed", cleanup_error_type=type(error).__name__)
        report["finished"] = s.now()
        (work / "lab-report.json").write_bytes(b.dumps(report))
        print("Outcome: " + report["status"], flush=True)
    if report["status"] != "passed":
        raise SystemExit(1)


def RuntimeForRecovery(root):
    driver = Runtime(root)
    identity, current = s.instance(root), s.state(root)
    pending = s.read(root / "pending.json") if (root / "pending.json").exists() else {"target": current["active"], "before": current}
    driver.prepare_recovery(identity, current, pending)
    return driver


if __name__ == "__main__":
    main()
