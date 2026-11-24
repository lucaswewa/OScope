"""Observe only a newly owned simulated runtime; never targets an existing server."""
import hashlib
import json
import os
from pathlib import Path
import socket
import struct
import threading
import time

from acceptance_workflows import Engineering
import pilot_metrics as m

ROOT = Path(__file__).resolve().parents[1]


class Observer:
    def __init__(self, report, image, seconds, interval):
        self.report, self.seconds, self.interval = report, seconds, interval
        self.engine = Engineering(report, image)
        self.engine.name = "oscope-r13-" + report.root.name
        self.stop = threading.Event()
        self.ready = threading.Event()
        self.stream = None
        self.reader = self.sampler = None
        self.errors, self.samples, self.workflows = [], [], []
        self.frames = 0

    def other_containers(self):
        return sorted(name for name in self.engine.docker("ps", "--format", "{{.Names}}").stdout.splitlines() if name != self.engine.name)

    def backend_pid(self):
        stack = [self.engine.info()["State"]["Pid"]]
        found = []
        while stack:
            pid = stack.pop()
            folder = Path("/proc") / str(pid)
            if (folder / "comm").read_text().strip() == "oscope-server":
                found.append(pid)
            stack.extend(int(p) for p in (folder / "task" / str(pid) / "children").read_text().split())
        m.check(len(found) == 1, "exactly one owned backend process required")
        return found[0]

    def process(self):
        folder = Path("/proc") / str(self.pid)
        text = (folder / "stat").read_text()
        fields = text[text.rfind(")") + 2:].split()
        m.check(fields[19] == self.process_start, "backend PID restarted/reused during measurement")
        status = dict(line.split(":", 1) for line in (folder / "status").read_text().splitlines() if ":" in line)
        return {"rss_bytes": int(status["VmRSS"].split()[0]) * 1024, "fds": len(list((folder / "fd").iterdir())),
                "cpu_seconds": (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")}

    def read_preview(self):
        e, previous = self.engine, None
        try:
            while not self.stop.is_set():
                opcode, data = e.runtime.frame(self.stream)
                if opcode == 9:
                    mask = os.urandom(4)
                    self.stream.sendall(bytes((0x8A, 0x80 | len(data))) + mask + bytes(v ^ mask[i % 4] for i, v in enumerate(data)))
                    continue
                if opcode == 1:
                    continue
                m.check(opcode == 2 and data[:7] == b"OSPV\x01\x01\x01" and len(data) >= 76, "bad preview frame")
                header, sequence = struct.unpack_from("<2I", data, 8)
                width, height, size = struct.unpack_from("<3I", data, 64)
                m.check(header >= 76 and 0 < width <= 160 and 0 < height <= 120 and len(data) == header + size,
                        "preview dimensions/framing differ")
                m.check(data[header:header + 2] == b"\xff\xd8" and data[-2:] == b"\xff\xd9", "bad preview JPEG")
                m.check(previous is None or sequence > previous, "preview sequence reset")
                previous = sequence
                self.frames += 1
                self.ready.set()
                e.runtime.send_text(self.stream, json.dumps({"ack": sequence + 1}))
        except Exception as error:
            if not self.stop.is_set():
                self.errors.append({"worker": "preview", "type": type(error).__name__})

    def sample(self):
        c = self.engine.client
        session = next(s for s in c.json("/api/v1/preview/sessions") if s["session"] == self.session_id)
        m.check(session["gpu"] is True, "preview silently used CPU")
        row = {"t": time.monotonic() - self.started, **self.process(), "preview_window_mean_ms": session["latency_ms"],
               "sent": session["sent"], "dropped": session["dropped"], "gpu": session["gpu"]}
        if self.stop.is_set():
            return
        self.samples.append(row)
        with (self.report.root / "samples.jsonl").open("ab") as stream:
            stream.write(json.dumps(row, allow_nan=False).encode() + b"\n")
            stream.flush()
            os.fsync(stream.fileno())

    def sampling(self):
        try:
            while not self.stop.is_set():
                self.sample()
                if self.stop.wait(self.interval):
                    break
        except Exception as error:
            self.errors.append({"worker": "sampler", "type": type(error).__name__})

    def wait_until(self, target):
        while time.monotonic() < target:
            m.check(not self.errors, "measurement worker failed")
            self.stop.wait(min(.25, target - time.monotonic()))

    def stop_workers(self):
        self.stop.set()
        if self.stream:
            try:
                self.stream.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            self.stream.close()
        for thread in (self.reader, self.sampler):
            if thread:
                thread.join(timeout=15)
                m.check(not thread.is_alive(), "measurement worker did not stop")

    def run(self):
        e, r = self.engine, self.report
        others = self.other_containers()
        e.bootstrap()
        r.report["image_id"] = e.image
        e.cuda()
        r.report["checks"]["cuda"] = True
        e.session()
        r.report["checks"]["session"] = True
        c = e.client
        c.action("stage/home", {})
        point = {"x": 20000.0, "y": 12500.0}
        c.action("stage/move_to", {**point, "z": c.action("simulation/in_focus", point)})
        c.json("/api/v1/camera/roi", method="PUT", value={"x": 556, "y": 360, "width": 256, "height": 192}, expected=201)
        c.json("/api/v1/preview/defaults", method="PUT", value={"width": 160, "height": 120, "quality": 70, "max_fps": 5}, expected=201)
        c.action("camera/start_streaming", {})
        before = {s["session"] for s in c.json("/api/v1/preview/sessions")}
        self.stream = c.websocket(path="/api/v1/preview/stream", first_message=True)
        self.stream.settimeout(30)
        e.runtime.send_text(self.stream, json.dumps({"width": 160, "height": 120, "max_fps": 5, "ack": 0}))
        self.reader = threading.Thread(target=self.read_preview, name="r13-preview", daemon=True)
        self.reader.start()
        m.check(self.ready.wait(30) and not self.errors, "required preview did not render")
        def rendered():
            sessions = [s for s in c.json("/api/v1/preview/sessions") if s["session"] not in before]
            return sessions if len(sessions) == 1 and sessions[0]["gpu"] is True else None
        sessions = e.runtime.until(rendered, "actual GPU session metadata", timeout=10)
        self.session_id = sessions[0]["session"]
        self.pid = self.backend_pid()
        text = (Path("/proc") / str(self.pid) / "stat").read_text()
        self.process_start = text[text.rfind(")") + 2:].split()[19]
        gpu = None
        try:
            gpu = e.docker("exec", e.name, "nvidia-smi", "--query-gpu=uuid,name,driver_version,memory.total", "--format=csv,noheader,nounits").stdout.strip()
        except RuntimeError:
            pass  # Kernel execution is required separately; unknown identity prevents comparison.
        metadata = r.report["metadata"]
        sources = {name: m.sha(ROOT / name) for name in ("scripts/pilot.py", "scripts/pilot_metrics.py", "scripts/pilot_runtime.py",
                   "scripts/acceptance_workflows.py", "scripts/check-runtime.py", "scripts/runtime_workflows.py")}
        r.report["context"].update(host_fingerprint=hashlib.sha256(Path("/etc/machine-id").read_bytes()).hexdigest(),
            gpu_identity=gpu, kernel=metadata["kernel"], docker_version=metadata["docker_server"],
            simulator_config_sha256=metadata["config_sha256"], workload_id=r.profile["workload_id"],
            workflow_bursts=r.profile["workflow_bursts"], seconds=self.seconds, interval=self.interval, runner_sources=sources,
            other_containers=others, process_pid=self.pid)
        self.started = time.monotonic()
        self.sampler = threading.Thread(target=self.sampling, name="r13-sampler", daemon=True)
        self.sampler.start()
        deadline = self.started + self.seconds
        for index in range(r.profile["workflow_bursts"]):
            self.wait_until(self.started + self.seconds * (index + 1) / (r.profile["workflow_bursts"] + 1))
            c.action("auth/acquire_control", {})
            used = sum(p.stat().st_size for p in e.private.rglob("*") if p.is_file())
            m.check(used < 256 * 1024**2, "bounded synthetic-state limit reached")
            c.action("camera/stop_streaming", {})  # R07 workflow starts its own acquisition interval.
            started = time.monotonic()
            evidence = e.runtime.runtime_workflows.check(c, e.runtime.frame, e.runtime.send_text, e.runtime.until, True)
            timing = {"burst": index, "started_seconds": started - self.started, "seconds": time.monotonic() - started, "evidence": evidence}
            self.workflows.append(timing)
            r.artifact("workflow_" + str(index) + ".json", timing)
            c.action("camera/start_streaming", {})
            print("Completed workflow burst " + str(index + 1), flush=True)
        m.check(time.monotonic() <= deadline + self.interval, "workflows exceeded measurement window")
        self.wait_until(deadline)
        self.stop_workers()
        r.report["worker_errors"] = self.errors
        m.check(not self.errors and self.frames > 0, "measurement worker failed")
        r.report["context"]["other_containers"] = sorted(set(others + self.other_containers()))
        metrics, references = m.summarize(self.samples, self.workflows)
        r.report.update(metrics=metrics, references=references, preview_frames=self.frames, sample_count=len(self.samples),
                        measurement_scope="quarter sensor preview with bounded workflow bursts; server latency window means",
                        not_measured=["20 MP benchmark budgets", "physical camera/stage", "GPU VRAM attribution", "operator active time", "full combined-load endurance"])
        r.report["checks"]["measurements"] = True
        c.action("camera/stop_streaming", {})
        e.scientific = self.workflows[-1]["evidence"]
        e.active = c.json("/api/v1/calibration/active")
        e.certificate = m.sha(e.private / "data/tls/certificate.pem")
        e.persistence()
        # Earlier dark sets are superseded by the final burst; verify each earlier
        # project's jobs/outputs while requiring the final active dark set.
        for workflow in self.workflows:
            evidence = {**workflow["evidence"], "dark": e.scientific["dark"]}
            e.runtime.runtime_workflows.verify(e.client, evidence)
        r.report["checks"]["persistence"] = True

    def cleanup(self):
        try:
            self.stop_workers()
        finally:
            self.report.report["worker_errors"] = self.errors
            self.engine.cleanup()
