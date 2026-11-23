#!/usr/bin/env python3
"""Exercise an immutable runtime image with isolated, disposable simulated data.

Run on a Linux Docker host, not inside the development container. No credentials
are accepted on the command line or written into the report. Never touches the
deployment's Compose project or named volumes.
"""
import argparse
import base64
import hashlib
import http.client
import json
import os
from pathlib import Path
import secrets
import socket
import ssl
import struct
import subprocess
import tempfile
import time
import uuid
import zlib


def docker(*args, input=None, check=True, binary=False):
    result = subprocess.run(
        ["docker", *args], input=input, text=not binary, capture_output=True, timeout=180
    )
    if check and result.returncode:
        # Never include stdin, which can contain a generated account password.
        raise RuntimeError(f"docker {args[0]} failed: {result.stderr.strip()}")
    return result


def inspect(name):
    return json.loads(docker("inspect", name).stdout)[0]


def until(check, label, timeout=90):
    end = time.monotonic() + timeout
    while True:
        result = check()
        if result:
            return result
        if time.monotonic() >= end:
            raise AssertionError(f"timed out: {label}")
        time.sleep(0.2)


class Client:
    def __init__(self, port, certificate):
        self.port = port
        self.context = ssl.create_default_context(cafile=str(certificate))
        self.token = None

    def request(self, path, method="GET", value=None, raw=None, headers=None,
                expected=200, authenticated=True):
        headers = dict(headers or {})
        if authenticated and self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        if value is not None:
            raw = json.dumps(value).encode()
            headers["Content-Type"] = "application/json"
        connection = http.client.HTTPSConnection(
            "127.0.0.1", self.port, context=self.context, timeout=10
        )
        try:
            connection.request(method, path, raw, headers)
            response = connection.getresponse()
            data = response.read()
            assert response.status == expected, f"{method} {path}: HTTP {response.status}, expected {expected}"
            return data
        finally:
            connection.close()

    def json(self, path, **kwargs):
        return json.loads(self.request(path, **kwargs))

    def action(self, path, value):
        started = self.json(f"/api/v1/{path}", method="POST", value=value, expected=201)
        href = started["href"]

        def finished():
            now = self.json(href)
            return now if now["status"] not in ("pending", "running") else None

        result = until(finished, path)
        assert result["status"] == "completed", f"{path}: {result['status']}"
        return result.get("output")

    def login(self, name, password):
        self.token = self.json("/api/v1/auth/login", method="POST",
                               value={"name": name, "password": password})["token"]

    def websocket(self, authenticated=True, origin=None):
        stream = self.context.wrap_socket(
            socket.create_connection(("127.0.0.1", self.port), timeout=10),
            server_hostname="localhost",
        )
        key = base64.b64encode(os.urandom(16)).decode()
        headers = [f"GET /api/v1/media/ping HTTP/1.1",
                   f"Host: 127.0.0.1:{self.port}", "Upgrade: websocket",
                   "Connection: Upgrade", "Sec-WebSocket-Version: 13",
                   f"Sec-WebSocket-Key: {key}"]
        if authenticated:
            headers.append(f"Authorization: Bearer {self.token}")
        if origin:
            headers.append(f"Origin: {origin}")
        stream.sendall(("\r\n".join(headers) + "\r\n\r\n").encode())
        # Read exactly the upgrade headers, without consuming the first frame.
        answer = bytearray()
        while not answer.endswith(b"\r\n\r\n"):
            answer.extend(exact(stream, 1))
            assert len(answer) < 16384, "oversized upgrade headers"
        if not authenticated or origin:
            expected = b"403" if origin else b"401"
            assert answer.split(b"\r\n", 1)[0].split()[1] == expected
            stream.close()
            return None
        assert answer.startswith(b"HTTP/1.1 101"), "WebSocket upgrade refused"
        accept = base64.b64encode(hashlib.sha1(
            (key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()
        ).digest())
        fields = dict(line.split(b":", 1) for line in bytes(answer).split(b"\r\n")[1:] if b":" in line)
        fields = {key.lower(): value.strip() for key, value in fields.items()}
        assert fields.get(b"sec-websocket-accept") == accept, "invalid upgrade accept"
        return stream


def exact(stream, length):
    data = bytearray()
    while len(data) < length:
        chunk = stream.recv(length - len(data))
        assert chunk, "unexpected WebSocket EOF"
        data.extend(chunk)
    return bytes(data)


def frame(stream):
    first, second = exact(stream, 2)
    assert not second & 0x80, "server must not mask frames"
    size = second & 127
    if size == 126:
        size = struct.unpack("!H", exact(stream, 2))[0]
    elif size == 127:
        size = struct.unpack("!Q", exact(stream, 8))[0]
    assert size <= 65536, "unexpectedly large frame"
    return first & 15, exact(stream, size)


def echo(stream):
    message = b"R06 authenticated TLS WebSocket"
    mask = os.urandom(4)
    stream.sendall(bytes((0x81, 0x80 | len(message))) + mask
                   + bytes(byte ^ mask[i % 4] for i, byte in enumerate(message)))
    assert frame(stream) == (1, message), "WebSocket echo mismatch"


def closed(stream):
    try:
        opcode, payload = frame(stream)
        assert opcode == 8 and struct.unpack("!H", payload[:2])[0] == 1001, "expected Going Away"
    finally:
        stream.close()


def png():
    def chunk(kind, data):
        return struct.pack("!I", len(data)) + kind + data + struct.pack("!I", zlib.crc32(kind + data))
    pixels = b"".join(b"\0" + bytes(range(8)) for _ in range(6))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack("!2I5B", 8, 6, 8, 0, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))


def run(args, report):
    assert os.name == "posix", "run this check on the Linux Docker host"
    name = "oscope-r06-" + uuid.uuid4().hex[:12]
    image = json.loads(docker("image", "inspect", args.image).stdout)[0]
    report.update(image=image["Id"], revision=image["Config"]["Labels"]["org.opencontainers.image.revision"])
    checks = report["checks"]
    with tempfile.TemporaryDirectory(prefix="oscope-r06-") as temporary:
        root = Path(temporary)
        root.chmod(0o755)
        for path in (root / "data", root / "scratch"):
            path.mkdir()
            if os.geteuid() == 0:
                os.chown(path, 1000, 1000)
            else:
                assert os.getuid() == 1000, "bind-mount test requires UID 1000 or root"
        mounts = ["--mount", f"type=bind,src={root / 'data'},dst=/var/lib/oscope/data",
                  "--mount", f"type=bind,src={root / 'scratch'},dst=/var/lib/oscope/scratch"]
        isolation = ["--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges",
                     "-e", "NVIDIA_VISIBLE_DEVICES=void",
                     "--tmpfs", "/tmp:rw,nosuid,nodev,size=256m,mode=1777"]
        password = secrets.token_urlsafe(32)
        try:
            version = docker("run", "--rm", "--read-only", args.image, "--version")
            assert version.stdout.startswith("oscope-server ")
            refusal = docker("run", "--rm", *isolation, *mounts, args.image, check=False)
            assert refusal.returncode != 0 and "needs an account first" in refusal.stderr
            checks.append("network startup refuses missing account")
            docker("run", "--rm", "-i", *isolation, *mounts, args.image,
                   "account", "add", "r06-operator", input=password + "\n")
            docker("run", "-d", "--name", name, "--init", "--restart", "unless-stopped",
                   "--stop-timeout", "30", "-p", "127.0.0.1::8210", *isolation, *mounts, args.image)

            def healthy():
                state = inspect(name)["State"]
                if state.get("Restarting"):
                    return False
                assert state["Running"], "runtime exited before readiness"
                return state.get("Health", {}).get("Status") == "healthy"

            until(healthy, "authenticated TLS health probe")
            info = inspect(name)
            host = info["HostConfig"]
            assert info["Config"]["User"] == "1000:1000" and host["ReadonlyRootfs"]
            assert host["CapDrop"] == ["ALL"] and "no-new-privileges" in host["SecurityOpt"]
            assert host["IpcMode"] != "host" and not host["Privileged"]
            ports = info["NetworkSettings"]["Ports"]["8210/tcp"]
            assert len(ports) == 1 and ports[0]["HostIp"] == "127.0.0.1"
            assert not host.get("DeviceRequests"), "CPU fallback check must have no injected GPU"
            client = Client(int(ports[0]["HostPort"]), root / "data/tls/certificate.pem")
            checks.append("CPU fallback; non-root read-only image; private IPC; localhost port")
            cpu = docker("exec", name, "/usr/local/bin/oscope-cuda-check", "--smoke")
            assert "no GPU in use, so OScope computes on the CPU" in cpu.stdout
            required = docker("exec", name, "/usr/local/bin/oscope-cuda-check", "--require-cuda", "--smoke", check=False)
            assert required.returncode != 0 and "CUDA required but unavailable" in required.stderr
            checks.append("CPU backend selected; required-device diagnostic rejects missing CUDA")
            assert client.json("/health") == {"status": "ok"}
            client.request("/ready", expected=401)
            client.request("/api/v1/system/memory", expected=401)
            client.websocket(authenticated=False)
            client.request("/api/v1/auth/login", method="POST",
                           value={"name": "r06-operator", "password": "wrong"}, expected=401)
            client.login("r06-operator", password)
            assert client.json("/ready") == {"status": "ready"}
            client.request("/api/v1/projects/create", method="POST", value={"name": "refused"}, expected=403)
            client.request("/api/v1/media/ping", headers={"Origin": "https://untrusted.invalid"}, expected=403)
            client.websocket(origin="https://untrusted.invalid")
            client.action("auth/acquire_control", {})
            checks.append("trusted TLS; login; authentication; control lease; origin enforcement")
            system = client.json("/api/v1/system/")
            assert system["title"]
            revision = report["revision"]
            assert client.json("/api/v1/system/git_commit") == revision[:12], "API/image revision mismatch"
            descriptions = client.json("/api/v1/directory/things")
            titles = {td["title"].lower() for td in descriptions}
            for deferred in ("preview", "capture", "autofocus", "exposure", "scan", "calibration", "workspace"):
                assert deferred not in titles
                client.request(f"/api/v1/{deferred}/", expected=404)
            for action in ("reboot", "shutdown"):
                client.request(f"/api/v1/power/{action}", method="POST", value={"confirm": True}, expected=404)
            checks.append("R07 workflows and host-power actions absent")
            project = client.action("projects/create", {"name": "R06 container persistence"})["id"]
            renamed = client.action("projects/rename", {"id": project, "name": "R06 renamed"})
            assert renamed["name"] == "R06 renamed"
            upload = client.json("/api/v1/upload/check.png", method="POST", raw=png(),
                                 headers={"Content-Type": "image/png"}, expected=201)
            submitted = client.action("jobs/submit", {"kind": "projects.import",
                                      "params": {"project": project, "upload": upload["id"]}})
            job_path = submitted["href"]

            def job_done():
                job = client.json(job_path)
                return job if job["state"] in ("completed", "failed", "cancelled") else None

            job = until(job_done, "PNG import")
            assert job["state"] == "completed", "PNG import job failed"
            project_path = f"/api/v1/projects/project/{project}"
            contents = client.json(project_path)
            acquisition = contents["acquisitions"][0]
            assert (acquisition["image"]["width"], acquisition["image"]["height"]) == (8, 6)
            file_path = f"/api/v1/data/{project}/project.json"
            whole = client.request(file_path)
            assert client.request(file_path, headers={"Range": "bytes=0-15"}, expected=206) == whole[:16]
            client.request(file_path, authenticated=False, expected=401)
            client.request(f"/api/v1/data/{project}/%2e%2e/accounts.json", expected=400)
            checks.append("project CRUD; upload/import job; stored media authentication and byte ranges")
            simulator = client.json("/api/v1/settings/simulator")
            simulator["rate"] = 1.25
            client.request("/api/v1/settings/simulator", method="PUT", value=simulator, expected=201)
            assert client.json("/api/v1/settings/restart_needed") is True
            stream = client.websocket()
            echo(stream)
            certificate = (root / "data/tls/certificate.pem").read_bytes()
            token = (root / "data/run/local.token").read_bytes()
            assert (root / "data/run/local.token").stat().st_mode & 0o777 == 0o600
            docker("stop", name)
            closed(stream)
            assert inspect(name)["State"]["ExitCode"] == 0
            docker("start", name)
            until(healthy, "ready after SIGTERM and start")
            client.port = int(inspect(name)["NetworkSettings"]["Ports"]["8210/tcp"][0]["HostPort"])
            client.request("/ready", expected=401)  # Old sessions cannot be reused.
            client.login("r06-operator", password)
            assert client.json(project_path) == contents and client.json(job_path)["state"] == "completed"
            assert client.json("/api/v1/settings/simulator")["rate"] == 1.25
            assert client.json("/api/v1/settings/restart_needed") is False
            assert (root / "data/tls/certificate.pem").read_bytes() == certificate
            assert (root / "data/run/local.token").read_bytes() != token
            checks.append("SIGTERM exit 0; WS close 1001; account/CA/project/job/config persistence; session/token rotation")
            client.action("auth/acquire_control", {})
            stream = client.websocket()
            echo(stream)
            before = inspect(name)["RestartCount"]
            client.request("/api/v1/power/restart", method="POST", value={"confirm": True}, expected=201)
            closed(stream)
            until(lambda: inspect(name)["RestartCount"] > before, "Docker supervisor restart")
            until(healthy, "ready after API restart")
            client.port = int(inspect(name)["NetworkSettings"]["Ports"]["8210/tcp"][0]["HostPort"])
            client.request("/ready", expected=401)
            client.login("r06-operator", password)
            assert client.json(project_path) == contents and client.json(job_path)["state"] == "completed"
            checks.append("API restart supervised by Docker; WS close 1001; durable data retained")
            environment = "/var/lib/oscope/scratch/r06-sdk-env"
            sdk = docker("exec", "-e", f"UV_PROJECT_ENVIRONMENT={environment}",
                         "-e", "PYTHONDONTWRITEBYTECODE=1", name,
                         "uv", "run", "--project", "/opt/oscope/sdk", "--locked",
                         "--no-managed-python", "--python", "/usr/bin/python3",
                         "pytest", "-q", "-p", "no:cacheprovider", "/opt/oscope/sdk/tests")
            assert "13 passed" in sdk.stdout, "SDK tests did not all execute"
            docker("exec", name, f"{environment}/bin/python", "-c",
                   "import sys,oscope_ext;assert sys.version_info[:2]==(3,12)")
            message = json.dumps({"ping": {}}).encode()
            packet = struct.pack("<I", len(message)) + message
            worker = docker("exec", "-i", name, f"{environment}/bin/python", "-m",
                            "oscope_ext.worker", "oscope_ext", input=packet, binary=True)
            reply = worker.stdout
            assert struct.unpack("<I", reply[:4])[0] == len(reply) - 4
            assert json.loads(reply[4:]) == {"pong": {}}, "runtime worker protocol failed"
            docker("exec", name, "bash", "-c", "! command -v rustc && ! command -v cargo && ! command -v node")
            linked = docker("exec", name, "ldd", "/usr/local/bin/oscope-server").stdout
            assert "not found" not in linked
            docker("exec", name, "/usr/local/bin/oscope-healthcheck")
            checks.append("Python 3.12; 13 locked SDK tests; real worker ping; linked libraries; no Rust/Node toolchains")
            client.action("auth/acquire_control", {})
            client.action("projects/delete", {"id": project})
            client.request(project_path, expected=404)
        finally:
            # Only this check's unique container and temporary bind mounts.
            docker("rm", "-f", name, check=False)
    report["status"] = "passed"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default="oscope-backend:r06")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report = {"phase": "R06", "status": "failed", "checks": []}
    try:
        run(args, report)
    finally:
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(json.dumps(report, indent=2) + "\n")
        print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
