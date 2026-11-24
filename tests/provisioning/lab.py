#!/usr/bin/env python3
"""Explicit required-CUDA simulated deployment lab; no Windows host changes."""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]


def run(args, password=None, check=True):
    result = subprocess.run(args, input=password, capture_output=True, text=True, timeout=180)
    if check and result.returncode:
        raise RuntimeError(f"lab command failed (exit {result.returncode}): {result.stderr[-2000:]}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="oscope-backend:r07")
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    if sys.platform != "linux" or os.geteuid() != 0:
        parser.error("run in WSL as root for the Docker socket and UID-1000 state setup")
    if args.report.exists():
        parser.error("report already exists; use a new run path")
    image = json.loads(run(["docker", "image", "inspect", args.image]).stdout)[0]["Id"]
    spec = importlib.util.spec_from_file_location("provision_runtime", ROOT / "scripts/provision-runtime.py")
    runtime = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runtime)
    observed_versions = runtime.linux_versions()  # Lab observation only, not a production baseline.
    run_id = str(uuid.uuid4())
    work = ROOT / "artifacts/r10" / ("lab-" + run_id)
    work.mkdir(parents=True, mode=0o700)
    checks, owned, supervisor = [], [], None
    command = [sys.executable, str(ROOT / "scripts/provision-runtime.py")]
    try:
        for index in range(2):
            installation = str(uuid.uuid4())
            root = work / ("runtime-" + str(index))
            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0))
                port = listener.getsockname()[1]
            identity = {"installation_id": installation, "instrument_serial": f"R10-SIM-{index}",
                        "pc_serial": "engineering-lab", "baseline_id": "r10-lab-only", "image_id": image,
                        "port": port, "compute": "cuda", "linux_versions": observed_versions}
            name = "oscope-" + uuid.UUID(installation).hex
            path = work / f"identity-{index}.json"
            path.write_text(json.dumps(identity), encoding="utf-8")
            owned.append((name, installation))
            password = secrets.token_urlsafe(32)
            init = [*command, "init", "--root", str(root), "--identity", str(path), "--engineering"]
            result = json.loads(run(init, password + "\n").stdout)
            assert result["state"] == "ready" and result["qualification"] == "pending" and result["compute"] == "cuda"
            cert = (root / "data/tls/certificate.pem").read_bytes()
            (root / "data/preserve.fixture").write_bytes(b"preserve synthetic instrument data")
            run(init, password + "\n")
            assert (root / "data/tls/certificate.pem").read_bytes() == cert
            assert (root / "data/preserve.fixture").read_bytes() == b"preserve synthetic instrument data"
            checks.append({"installation_id": installation, "port": port, "certificate_sha256": hashlib.sha256(cert).hexdigest(),
                           "checks": ["actual required CUDA diagnostic", "trusted TLS and bootstrap login", "repeat preserves identity/TLS/data"]})
            if index == 0:
                wrong = run(init, secrets.token_urlsafe(32) + "\n", check=False)
                assert wrong.returncode == 2 and "not rotating" in wrong.stderr
                run(init, password + "\n")  # Original credential still works.
                changed = {**identity, "pc_serial": "another-pc"}
                path.write_text(json.dumps(changed), encoding="utf-8")
                assert run(init, password + "\n", check=False).returncode == 2
                path.write_text(json.dumps(identity), encoding="utf-8")
                supervisor = subprocess.Popen([*command, "supervise", "--root", str(root), "--engineering", "--interval", "1"],
                                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                time.sleep(3)
                assert supervisor.poll() is None
                run([*command, "pause", "--root", str(root), "--engineering"])
                time.sleep(2)
                assert supervisor.poll() is None
                info = json.loads(run(["docker", "container", "inspect", name]).stdout)[0]
                assert info["State"]["Status"] == "exited"
                assert json.loads((root / "status.json").read_text())["state"] == "maintenance"
                run([*command, "resume", "--root", str(root), "--engineering"])
                assert (root / "data/tls/certificate.pem").read_bytes() == cert
                supervisor.terminate()
                supervisor.communicate(timeout=10)
                supervisor = None
                run([*command, "supervise", "--root", str(root), "--engineering", "--cycles", "2", "--interval", "1"])
                checks[0]["checks"] += ["wrong credential not overwritten", "foreign identity rejected", "foreground supervision",
                                        "maintenance stays stopped", "explicit resume", "bounded supervisor run"]
        assert checks[0]["installation_id"] != checks[1]["installation_id"]
        assert checks[0]["certificate_sha256"] != checks[1]["certificate_sha256"]
        report = {"schema_version": 1, "run_id": run_id, "completed_at_utc": datetime.now(timezone.utc).isoformat(),
                  "status": "passed", "scope": "simulated_linux_bootstrap_lab", "image_id": image,
                  "checks": checks, "windows_import_task_cold_boot": "not_run", "physical_hardware": "not_run",
                  "production_qualification": "pending", "data_root": str(work)}
        report["runtime_sha256"] = hashlib.sha256((ROOT / "scripts/provision-runtime.py").read_bytes()).hexdigest()
        args.report.parent.mkdir(parents=True, exist_ok=True)
        with args.report.open("x", encoding="utf-8") as stream:
            json.dump(report, stream, indent=2)
            stream.write("\n")
        print(json.dumps(report))
    finally:
        if supervisor is not None:
            supervisor.terminate()
            supervisor.communicate(timeout=10)
        for name, installation in owned:
            result = run(["docker", "container", "inspect", name], check=False)
            if result.returncode == 0:
                info = json.loads(result.stdout)[0]
                if info["Config"].get("Labels", {}).get("oscope.installation") == installation:
                    run(["docker", "stop", "--time", "30", name], check=False)
                    run(["docker", "rm", name])
        # Keep all synthetic state/TLS/data and reports for review. No volumes/data are deleted.


if __name__ == "__main__":
    main()
