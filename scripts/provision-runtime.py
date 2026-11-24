#!/usr/bin/env python3
"""Engineering host runtime bootstrap. Does not qualify physical devices or a PC."""
import argparse
from contextlib import contextmanager
from datetime import datetime, timezone
import http.client
import json
import os
from pathlib import Path
import platform
import re
import ssl
import subprocess
import sys
import time
import tomllib
import uuid


def validate_identity(value):
    fields = {"installation_id", "instrument_serial", "pc_serial", "baseline_id", "image_id", "port", "compute", "linux_versions"}
    if not isinstance(value, dict) or set(value) != fields:
        raise ValueError("identity fields differ from the provisioning schema")
    if not isinstance(value["installation_id"], str) or str(uuid.UUID(value["installation_id"])) != value["installation_id"]:
        raise ValueError("installation_id must be a canonical UUID")
    for name in ("instrument_serial", "pc_serial", "baseline_id"):
        if not isinstance(value[name], str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}", value[name]):
            raise ValueError(f"invalid {name}")
    if not isinstance(value["image_id"], str) or not re.fullmatch(r"sha256:[0-9a-f]{64}", value["image_id"]):
        raise ValueError("use an immutable Docker image ID")
    if type(value["port"]) is not int or not 1024 <= value["port"] <= 65535:
        raise ValueError("port must be an integer from 1024 to 65535")
    if value["compute"] not in ("cuda", "cpu_fallback"):
        raise ValueError("compute must be cuda or the explicit engineering cpu_fallback")
    pins = value["linux_versions"]
    if not isinstance(pins, dict) or set(pins) != {"ubuntu_release", "kernel_release", "docker_version", "compose_version", "nvidia_ctk_version"} or any(not isinstance(v, str) or not v for v in pins.values()):
        raise ValueError("complete Linux version pins are required")
    return value


def linux_versions():
    def output(arguments):
        return subprocess.check_output(arguments, text=True, timeout=30).strip()
    release = Path("/etc/os-release").read_text(encoding="utf-8")
    return {"ubuntu_release": re.search(r'(?m)^VERSION_ID="?([^"\n]+)', release)[1],
            "kernel_release": platform.release(),
            "docker_version": output(["/usr/bin/docker", "version", "--format", "{{.Server.Version}}"]),
            "compose_version": output(["/usr/bin/docker", "compose", "version", "--short"]),
            "nvidia_ctk_version": re.search(r"\d+\.\d+\.\d+", output(["/usr/bin/nvidia-ctk", "--version"]))[0]}


def verify_versions(identity):
    if linux_versions() != identity["linux_versions"]:
        raise ValueError("installed Linux/kernel/Docker/Compose/toolkit differs from the pinned candidate")


def atomic_json(path, value):
    temporary = path.with_name(path.name + "." + uuid.uuid4().hex + ".tmp")
    try:
        with temporary.open("x", encoding="utf-8") as stream:
            os.chmod(temporary, 0o600)
            json.dump(value, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


def validate_root(path):
    reserved = {Path(p) for p in ("/", "/etc", "/var", "/var/lib", "/home", "/root", "/usr", "/opt", "/tmp", "/run")}
    if any(c in str(path) for c in (",", "\r", "\n")) or not path.is_absolute() or path in reserved or path == Path(path.anchor) or path.resolve() != path:
        raise ValueError("use a dedicated absolute Linux directory without symlinks or parent traversal")
    if any(parent.is_symlink() for parent in (path, *path.parents)):
        raise ValueError("provisioning root must not traverse a symlink")
    return path


@contextmanager
def locked(root):
    import fcntl  # Linux-only execution; validators are also tested on Windows.
    validate_root(root)
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (root / ".provision.lock").open("a") as stream:
        deadline = time.monotonic() + 15
        while True:
            try:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                if time.monotonic() >= deadline:
                    raise RuntimeError("installation is busy; retry after the other owned operation completes")
                time.sleep(0.1)
        yield


def reserve(root, identity):
    path = root / "identity.json"
    if path.exists():
        if json.loads(path.read_text(encoding="utf-8")) != identity:
            raise ValueError("installation identity changed; refusing to replace existing instrument state")
    else:
        if any(p.name != ".provision.lock" for p in root.iterdir()):
            raise ValueError("unowned nonempty installation directory")
        atomic_json(path, identity)
    for name in ("data", "scratch"):
        directory = root / name
        if directory.is_symlink():
            raise ValueError("state directories must not be symlinks")
        directory.mkdir(exist_ok=True, mode=0o700)
        os.chown(directory, 1000, 1000)
    os.chmod(root, 0o700)


def docker(*args, password=None, check=True):
    result = subprocess.run(["/usr/bin/docker", *args], input=password, text=True, capture_output=True, timeout=90)
    if check and result.returncode:
        # Do not include child output: it may originate from an authentication command.
        raise RuntimeError(f"Docker operation {args[0]} failed (exit {result.returncode}); inspect this installation")
    return result


def container_name(identity):
    return "oscope-" + uuid.UUID(identity["installation_id"]).hex


def validate_container(info, identity, root):
    host, config = info["HostConfig"], info["Config"]
    ports = host.get("PortBindings", {}).get("8210/tcp")
    mounts = {m["Destination"]: m for m in info["Mounts"]}
    expected = {"/var/lib/oscope/data": root / "data", "/var/lib/oscope/scratch": root / "scratch"}
    if (info["Image"] != identity["image_id"] or
        config.get("Labels", {}).get("oscope.installation") != identity["installation_id"] or
        config.get("User") != "1000:1000" or not host["ReadonlyRootfs"] or
        host["IpcMode"] != "private" or host["Privileged"] or host.get("CapDrop") != ["ALL"] or
        "no-new-privileges" not in host.get("SecurityOpt", []) or
        host.get("RestartPolicy", {}).get("Name") != "unless-stopped" or
        ports != [{"HostIp": "127.0.0.1", "HostPort": str(identity["port"])}] or
        bool(host.get("DeviceRequests")) != (identity["compute"] == "cuda") or host.get("Devices") or
        set(mounts) != set(expected) or any(mounts[d]["Source"] != str(p) for d, p in expected.items())):
        raise ValueError("existing container differs from owned image/identity/isolation/mount/port/compute settings")


def inspect_owned(identity, root):
    result = docker("container", "inspect", container_name(identity), check=False)
    if result.returncode:
        # Distinguish a missing container from a failed Docker daemon/query.
        names = docker("ps", "-a", "--format", "{{.Names}}").stdout.splitlines()
        if container_name(identity) in names:
            raise RuntimeError("cannot inspect the owned container")
        return None
    info = json.loads(result.stdout)[0]
    validate_container(info, identity, root)
    return info


def ensure_container(identity, root):
    image = json.loads(docker("image", "inspect", identity["image_id"]).stdout)[0]
    if image["Os"] != "linux" or image["Architecture"] != "amd64" or image["Config"]["User"] != "1000:1000":
        raise ValueError("expected the Linux amd64 non-root OScope runtime image")
    info = inspect_owned(identity, root)
    if info is None:
        args = ["create", "--pull", "never", "--name", container_name(identity), "--init", "--read-only", "--ipc", "private",
                "--cap-drop", "ALL", "--security-opt", "no-new-privileges", "--restart", "unless-stopped",
                "--stop-timeout", "30", "--shm-size", "1g", "--label", "oscope.installation=" + identity["installation_id"],
                "--publish", f"127.0.0.1:{identity['port']}:8210", "--tmpfs", "/tmp:rw,nosuid,nodev,size=256m,mode=1777"]
        for name in ("data", "scratch"):
            args += ["--mount", f"type=bind,src={root / name},dst=/var/lib/oscope/{name}"]
        args += ["-e", "NVIDIA_VISIBLE_DEVICES=" + ("all" if identity["compute"] == "cuda" else "void")]
        if identity["compute"] == "cuda":
            args += ["--gpus", "all"]
        docker(*args, identity["image_id"])
        info = inspect_owned(identity, root)
    if info["State"]["Status"] not in ("running", "restarting"):
        docker("start", container_name(identity))


def engineering_config(root):
    path = root / "data/config/oscope.toml"
    config = tomllib.loads(path.read_text(encoding="utf-8"))
    devices = config.get("devices", {})
    if not isinstance(devices, dict) or any(devices.get(name) != "sim" for name in ("camera", "stage", "illumination", "objective")):
        raise ValueError("R10 engineering bootstrap requires simulated devices; physical drivers remain unqualified")


def account(identity, root, password):
    if not 16 <= len(password) <= 256 or "\n" in password or "\r" in password:
        raise ValueError("bootstrap credential must be a single line of 16–256 characters")
    args = ["run", "--pull", "never", "--rm", "-i", "--network", "none", "--read-only", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges", "-e", "NVIDIA_VISIBLE_DEVICES=void",
            "--mount", f"type=bind,src={root / 'data'},dst=/var/lib/oscope/data", identity["image_id"], "account"]
    names = docker(*args, "list").stdout.splitlines()
    if "operator" not in names:
        docker(*args, "add", "operator", password=password + "\n")


def health(identity, root, password=None, timeout=60):
    deadline = time.monotonic() + timeout
    last = "TLS/health unavailable"
    while time.monotonic() < deadline:
        try:
            context = ssl.create_default_context(cafile=str(root / "data/tls/certificate.pem"))
            connection = http.client.HTTPSConnection("127.0.0.1", identity["port"], context=context, timeout=5)
            try:
                connection.request("GET", "/health")
                response = connection.getresponse()
                assert response.status == 200 and json.loads(response.read()) == {"status": "ok"}
                if password is not None:
                    connection.request("POST", "/api/v1/auth/login", json.dumps({"name": "operator", "password": password}),
                                       {"Content-Type": "application/json"})
                    response = connection.getresponse()
                    if response.status != 200:
                        response.read()
                        raise ValueError("existing bootstrap account does not match the protected credential; not rotating it")
                    response.read()  # Discard token; never include authentication data in reports.
            finally:
                connection.close()
            return
        except (OSError, ssl.SSLError, http.client.HTTPException, AssertionError) as error:
            last = type(error).__name__
            time.sleep(0.5)
    raise RuntimeError(f"bounded trusted-TLS readiness failed: {last}")


def diagnostic(identity):
    args = ["run", "--pull", "never", "--rm", "--network", "none", "--read-only", "--cap-drop", "ALL",
            "--tmpfs", "/tmp:rw,nosuid,nodev,size=256m", "--entrypoint", "/usr/local/bin/oscope-cuda-check"]
    if identity["compute"] == "cuda":
        args += ["--gpus", "all"]
    else:
        args += ["-e", "NVIDIA_VISIBLE_DEVICES=void"]
    args += [identity["image_id"], *( ["--require-cuda"] if identity["compute"] == "cuda" else []), "--smoke"]
    result = docker(*args)
    if "smoke: ok" not in result.stdout or (identity["compute"] == "cuda" and "Cuda  available" not in result.stdout):
        raise RuntimeError("required compute diagnostic did not execute successfully")


def status(identity, root, state):
    report = {"schema_version": 1, "installation_id": identity["installation_id"], "state": state,
              "image_id": identity["image_id"], "compute": identity["compute"],
              "updated_at_utc": datetime.now(timezone.utc).isoformat(), "qualification": "pending",
              "devices": "simulated", "container": container_name(identity)}
    atomic_json(root / "status.json", report)
    return report


def initialize(root, identity, password):
    verify_versions(identity)
    image = json.loads(docker("image", "inspect", identity["image_id"]).stdout)[0]
    if image["Os"] != "linux" or image["Architecture"] != "amd64" or image["Config"]["User"] != "1000:1000":
        raise ValueError("expected the pinned Linux amd64 non-root image")
    reserve(root, identity)
    if (root / "maintenance").exists():
        raise ValueError("installation is in maintenance; use explicit resume")
    account(identity, root, password)
    engineering_config(root)
    diagnostic(identity)
    ensure_container(identity, root)
    health(identity, root, password)
    atomic_json(root / "initialized.json", {"installation_id": identity["installation_id"]})
    return status(identity, root, "ready")


def supervise(root, identity, cycles=0, interval=5):
    if not (root / "initialized.json").is_file():
        raise ValueError("initialize this installation before unattended startup")
    if json.loads((root / "initialized.json").read_text(encoding="utf-8")).get("installation_id") != identity["installation_id"]:
        raise ValueError("initialization receipt belongs to another installation")
    first = True
    count = 0
    while cycles == 0 or count < cycles:
        with locked(root):
            if json.loads((root / "identity.json").read_text(encoding="utf-8")) != identity:
                raise ValueError("installation identity changed during supervision")
            if (root / "maintenance").exists():
                report = status(identity, root, "maintenance")
            else:
                engineering_config(root)
                if first:
                    verify_versions(identity)
                    diagnostic(identity)
                    ensure_container(identity, root)
                    first = False
                info = inspect_owned(identity, root)
                if info is None or info["State"]["Status"] not in ("running", "restarting"):
                    raise RuntimeError("backend stopped; Docker owns process restart; inspect rather than racing it")
                health(identity, root)
                report = status(identity, root, "ready")
        count += 1
        if cycles == 0 or count < cycles:
            time.sleep(interval)  # Foreground WSL process keeps the Windows-side handle open.
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("init", "supervise", "pause", "resume", "status"))
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--identity", type=Path, help="Public installation identity JSON for init")
    parser.add_argument("--engineering", action="store_true", required=True, help="R10 is not a production-qualified installer")
    parser.add_argument("--cycles", type=int, default=0)
    parser.add_argument("--interval", type=int, default=5)
    args = parser.parse_args()
    if sys.platform != "linux" or os.geteuid() != 0 or args.cycles < 0 or not 1 <= args.interval <= 60:
        parser.error("execute on Linux as root; use bounded positive interval/cycles")
    try:
        if args.operation != "init" and not (args.root / "identity.json").is_file():
            raise ValueError("owned installation identity is missing")
        if args.operation in ("supervise", "resume"):
            validate_root(args.root)
            identity = validate_identity(json.loads((args.root / "identity.json").read_text(encoding="utf-8")))
            if args.operation == "resume":
                with locked(args.root):
                    if not (args.root / "initialized.json").is_file():
                        raise ValueError("initialize before resume")
                    (args.root / "maintenance").unlink(missing_ok=True)
            print(json.dumps(supervise(args.root, identity, args.cycles if args.operation == "supervise" else 1, args.interval)))
            return
        with locked(args.root):
            path = args.identity if args.operation == "init" else args.root / "identity.json"
            if path is None:
                raise ValueError("init requires --identity")
            identity = validate_identity(json.loads(path.read_text(encoding="utf-8")))
            if args.operation == "init":
                password = sys.stdin.readline().removesuffix("\n").removesuffix("\r")
                report = initialize(args.root, identity, password)
            elif args.operation == "pause":
                (args.root / "maintenance").touch(mode=0o600)
                if inspect_owned(identity, args.root):
                    docker("stop", "--time", "30", container_name(identity))
                report = status(identity, args.root, "maintenance")
            else:
                report = json.loads((args.root / "status.json").read_text(encoding="utf-8"))
            print(json.dumps(report))
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        parser.exit(2, f"provisioning: {error}\n")


if __name__ == "__main__":
    main()
