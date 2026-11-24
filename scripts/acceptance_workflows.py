"""Linux-only, required-CUDA engineering adapter; no physical-device adapter."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import re
import secrets
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


class Engineering:
    def __init__(self, attempt, image):
        self.attempt = attempt
        self.image = image
        self.name = "oscope-r11-" + attempt.root.name
        self.label = "oscope.acceptance=" + attempt.root.name
        self.private = attempt.root / "private"
        self.password = secrets.token_urlsafe(32)
        self.created = False
        self.client = None

    def docker(self, *args, password=None, check=True):
        result = subprocess.run(["/usr/bin/docker", *args], input=password, text=True,
                                capture_output=True, timeout=180)
        if check and result.returncode:
            raise RuntimeError("Docker operation failed; private state retained for diagnosis")
        return result

    def info(self):
        return json.loads(self.docker("inspect", self.name).stdout)[0]

    def actions(self):
        return [(name, getattr(self, name)) for name in
                ("bootstrap", "cuda", "session", "calibration_focus", "workflows", "persistence")]

    def bootstrap(self):
        if platform.system() != "Linux" or platform.machine() not in ("x86_64", "amd64") or os.geteuid() != 0:
            raise ValueError("Run engineering adapter as root on Linux x86_64")
        image = json.loads(self.docker("image", "inspect", self.image).stdout)[0]
        image_id = image["Id"]
        revision = image["Config"].get("Labels", {}).get("org.opencontainers.image.revision", "")
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", image_id) or not re.fullmatch(r"[0-9a-f]{40}", revision):
            raise ValueError("immutable runtime identity and OCI source revision required")
        if (image["Os"], image["Architecture"], image["Config"]["User"]) != ("linux", "amd64", "1000:1000"):
            raise ValueError("runtime must be Linux amd64 with UID 1000:1000")
        self.image = image_id  # Resolve once; a moved tag cannot change the tested image.
        self.revision = revision
        self.private.mkdir(mode=0o700)
        # The container needs to traverse its data parent. Protect the run directory
        # from other host users; bind mounts expose only data and scratch to UID 1000.
        os.chmod(self.attempt.root, 0o700)
        for name in ("data", "scratch"):
            path = self.private / name
            path.mkdir(mode=0o700)
            os.chown(path, 1000, 1000)
        config_dir = self.private / "data/config"
        config_dir.mkdir(mode=0o700)
        config = (ROOT / "configs/runtime.toml").read_text(encoding="utf-8")
        config = config.replace('sensor = "quarter"', 'sensor = "quarter"\nfocus = 1000.0\nrate = 10.0\nrotation = -20.0\n'
                                'scale = 0.98\nbacklash = 3.0\nvignetting = 0.2\n'
                                'response = [[0.85, 0.2, 0.05], [0.1, 0.8, 0.1], [0.02, 0.18, 0.6]]')
        parsed = tomllib.loads(config)
        if any(parsed["devices"].get(k) != "sim" for k in ("camera", "stage", "illumination", "objective")):
            raise ValueError("all engineering devices must be simulated")
        config_path = config_dir / "oscope.toml"
        config_path.write_text(config, encoding="utf-8", newline="\n")
        for path in (config_dir, config_path):
            os.chown(path, 1000, 1000)
        self.mounts = ["--mount", f"type=bind,src={self.private / 'data'},dst=/var/lib/oscope/data",
                       "--mount", f"type=bind,src={self.private / 'scratch'},dst=/var/lib/oscope/scratch"]
        isolation = ["--read-only", "--cap-drop", "ALL", "--security-opt", "no-new-privileges", "--ipc", "private"]
        self.docker("run", "--pull", "never", "--rm", "-i", "--network", "none", *isolation,
                    "-e", "NVIDIA_VISIBLE_DEVICES=void", *self.mounts, self.image,
                    "account", "add", "operator", password=self.password + "\n")
        # Set before run, since an interrupted CLI may still have created the container.
        self.created = True
        self.docker("run", "--pull", "never", "-d", "--name", self.name, "--label", self.label,
                    "--init", "--stop-timeout", "30", "-p", "127.0.0.1::8210", *isolation,
                    "--gpus", "all", "-e", "NVIDIA_VISIBLE_DEVICES=all",
                    "--tmpfs", "/tmp:rw,nosuid,nodev,size=256m,mode=1777", *self.mounts, self.image)
        spec = importlib.util.spec_from_file_location("acceptance_runtime_client", ROOT / "scripts/check-runtime.py")
        self.runtime = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.runtime)
        self.runtime.until(self.healthy, "isolated engineering runtime health", timeout=120)
        info = self.info()
        host = info["HostConfig"]
        assert info["Image"] == self.image and info["Config"]["User"] == "1000:1000"
        assert host["ReadonlyRootfs"] and host["CapDrop"] == ["ALL"] and not host["Privileged"]
        assert host["IpcMode"] != "host" and host["DeviceRequests"] and not host.get("Devices")
        assert "no-new-privileges" in host["SecurityOpt"]
        ports = info["NetworkSettings"]["Ports"]["8210/tcp"]
        assert len(ports) == 1 and ports[0]["HostIp"] == "127.0.0.1"
        self.port = int(ports[0]["HostPort"])
        self.attempt.report["metadata"].update(
            image_id=self.image, image_revision=revision, kernel=platform.release(), machine=platform.machine(),
            docker_server=self.docker("version", "--format", "{{.Server.Version}}").stdout.strip(),
            container=self.name, config_sha256=hashlib.sha256(config_path.read_bytes()).hexdigest(),
            device_mode="simulated_only", frontend_version="not_tested", windows_host="not_qualified",
            sdk="simulator; no vendor SDK", driver="see CUDA diagnostic artifact",
            sources={name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in
                     ("scripts/acceptance.py", "scripts/acceptance_workflows.py", "scripts/check-runtime.py", "scripts/runtime_workflows.py")})
        return {"immutable_image": self.image, "simulated_devices": True, "isolated": True}

    def healthy(self):
        state = self.info()["State"]
        assert state["Running"], "runtime exited before health"
        return state.get("Health", {}).get("Status") == "healthy"

    def cuda(self):
        diagnostic = self.docker("exec", self.name, "/usr/local/bin/oscope-cuda-check", "--require-cuda", "--smoke")
        assert "Cuda  available" in diagnostic.stdout and "smoke: ok" in diagnostic.stdout
        return self.attempt.artifact("cuda.json", {"required": True, "kernel_execution": "passed", "stdout": diagnostic.stdout})

    def connect(self):
        self.client = self.runtime.Client(self.port, self.private / "data/tls/certificate.pem")
        self.client.login("operator", self.password)
        assert self.client.json("/ready") == {"status": "ready"}
        self.client.action("auth/acquire_control", {})

    def session(self):
        self.connect()
        c = self.client
        assert c.json("/health") == {"status": "ok"}
        c.request("/ready", authenticated=False, expected=401)
        assert c.json("/api/v1/system/git_commit") == self.revision[:12]
        # Verify live configuration before any stage action, not just the template.
        devices = tomllib.loads((self.private / "data/config/oscope.toml").read_text(encoding="utf-8"))["devices"]
        assert all(devices[k] == "sim" for k in ("camera", "stage", "illumination", "objective"))
        assert c.json("/api/v1/settings/simulator")["rate"] == 10.0
        capabilities = {kind: c.json("/api/v1/" + kind + "/capabilities") for kind in ("camera", "stage")}
        return self.attempt.artifact("capabilities.json", capabilities)

    def job(self, action, value):
        return self.runtime.runtime_workflows.completed_job(self.client, self.client.action(action, value), self.runtime.until)[1]

    def calibration_focus(self):
        c = self.client
        # HeTissue::new(1, Slide::COVERSLIP).islands()[0], the focus/calibration
        # regression's point (tissue.rs + noise.rs); never a physical coordinate.
        point = {"x": 35350.49621825543, "y": 14060.14759534321}
        c.action("stage/home", {})
        truth = c.action("simulation/in_focus", point)
        c.action("stage/move_to", {**point, "z": truth + 12})
        c.json("/api/v1/camera/roi", method="PUT", value={"x": 556, "y": 360, "width": 256, "height": 192}, expected=201)
        c.action("camera/start_streaming", {})
        before = c.json("/api/v1/autofocus/measures")["brenner"]
        self.job("autofocus/run", {})
        focus = c.json("/api/v1/autofocus/last")
        error = abs(c.json("/api/v1/stage/position")["position"]["z"] - truth)
        bound = focus["depth_of_field_um"] / 4
        self.attempt.artifact("focus.json", {"focus": focus, "error_um": error, "limit_um": bound,
                             "limit_source": "crates/oscope-api/tests/focus.rs: depth_of_field_um / 4"})
        assert focus["found"] and focus["measure"] == "brenner" and error < bound
        assert c.json("/api/v1/autofocus/measures")["brenner"] > before
        c.json("/api/v1/camera/exposure", method="PUT", value=0.1, expected=201)
        self.job("exposure/auto", {})
        exposure = c.json("/api/v1/exposure/last")
        assert exposure["converged"] and c.json("/api/v1/camera/exposure") == exposure["exposure_ms"]
        self.job("exposure/white_balance", {})
        white = c.json("/api/v1/exposure/last_white_balance")
        assert white["gains"][1] == 1.0
        # Geometry uses a larger field, as in the calibration regression.
        c.action("camera/stop_streaming", {})
        caps = c.json("/api/v1/camera/capabilities")
        c.json("/api/v1/camera/roi", method="PUT", value={"x": 0, "y": 0, "width": caps["sensor_width"],
               "height": caps["sensor_height"]}, expected=201)
        c.action("stage/move_to", {**point, "z": 1002.0})
        c.action("camera/start_streaming", {})
        c.action("calibration/wizard_start", {})
        for index in range(6):
            if index == 3:
                c.action("stage/move_to", {"x": 10000.0, "y": 12500.0, "z": 1002.0})
            elif index == 4:
                c.action("simulation/load", {"name": "colour target"})
                c.action("stage/move_to", {"x": 47000.0, "y": 12500.0, "z": 1000.05})
            elif index == 5:
                c.action("stage/move_to", {**point, "z": 1002.0})
                c.action("simulation/load", {"name": "H&E tissue"})
            c.action("calibration/wizard_confirm", {})
            def settled():
                state = c.json("/api/v1/calibration/wizard")["steps"][index]["state"]
                return state if state["state"] != "running" else None
            state = self.runtime.until(settled, "calibration wizard step " + str(index), timeout=180)
            self.attempt.artifact("wizard_" + str(index) + ".json", c.json("/api/v1/calibration/wizard"))
            assert state["state"] == "done"
        wizard = c.json("/api/v1/calibration/wizard")
        assert wizard["finished"] and not wizard["cancelled"]
        active = c.json("/api/v1/calibration/active")
        self.attempt.artifact("calibration.json", active)
        geometry = active["geometry"]["values"]
        pixel_error = abs(geometry["pixel_um"] / (9.6 / (10 * 0.98)) - 1)
        assert pixel_error < 0.002 and abs(geometry["rotation_deg"] + 20) < 0.1
        assert all(abs(active["stage"]["values"]["backlash_um"][a] - 3.0) < 0.2 for a in ("x", "y"))
        assert active["flat"]["values"]["dark"] and active["flat"]["values"]["uneven"] > 0.05
        assert active["colour"]["values"]["delta_e_mean"] < 0.5
        c.action("camera/stop_streaming", {})
        return self.attempt.artifact("calibration_focus.json", {
            "focus": focus, "focus_error_um": error, "focus_limit_um": bound,
            "focus_limit_source": "crates/oscope-api/tests/focus.rs: depth_of_field_um / 4",
            "exposure": exposure, "white_balance": white, "wizard": wizard, "calibration": active,
            "pixel_error_fraction": pixel_error, "limits_scope": "simulated regression only"})

    def workflows(self):
        r = self.runtime
        self.scientific = r.runtime_workflows.check(self.client, r.frame, r.send_text, r.until, True)
        self.active = self.client.json("/api/v1/calibration/active")
        self.certificate = hashlib.sha256((self.private / "data/tls/certificate.pem").read_bytes()).hexdigest()
        return self.attempt.artifact("workflows.json", self.scientific)

    def persistence(self):
        self.docker("stop", self.name)
        assert self.info()["State"]["ExitCode"] == 0
        self.docker("start", self.name)
        self.runtime.until(self.healthy, "restart health", timeout=120)
        ports = self.info()["NetworkSettings"]["Ports"]["8210/tcp"]
        assert len(ports) == 1 and ports[0]["HostIp"] == "127.0.0.1"
        self.port = int(ports[0]["HostPort"])
        self.connect()
        self.runtime.runtime_workflows.verify(self.client, self.scientific)
        assert self.client.json("/api/v1/calibration/active") == self.active
        assert hashlib.sha256((self.private / "data/tls/certificate.pem").read_bytes()).hexdigest() == self.certificate
        return {"project_jobs_output_hashes": "unchanged", "all_active_calibration": "unchanged", "tls_certificate": "unchanged"}

    def cleanup(self):
        if self.created:
            result = self.docker("inspect", self.name, check=False)
            if result.returncode:
                # A failed run may not have created a container; verify its absence.
                names = self.docker("ps", "-a", "--format", "{{.Names}}").stdout.splitlines()
                assert self.name not in names
            else:
                info = json.loads(result.stdout)[0]
                assert info["Config"].get("Labels", {}).get("oscope.acceptance") == self.attempt.root.name
                self.docker("stop", self.name)
                self.docker("rm", self.name)
        self.password = None
        self.client = None
        return {"owned_container": "removed_or_never_created", "private_data": "retained", "other_deployments": "untouched"}
