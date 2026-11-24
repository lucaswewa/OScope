#!/usr/bin/env python3
"""R11 evidence runner. Engineering results never authorize shipment."""
import argparse
from datetime import datetime, timezone
import hashlib
import html
import json
import math
from pathlib import Path
import re
import sys
import time
import traceback
import uuid

ROOT = Path(__file__).resolve().parents[1]
VERSION = "r11.1"
QUALIFICATIONS = ("hardware_report", "host_baseline", "release_manifest", "fixture")
LIMITS = ("camera_min_fps", "camera_test_frames", "stage_repeatability_max_um",
          "stop_distance_max_um", "pixel_error_max_fraction", "focus_error_max_um",
          "reference_image_error_max", "endurance_min_seconds")
ENGINEERING_STEPS = ("bootstrap", "cuda", "session", "calibration_focus", "workflows", "persistence", "cleanup")
CHECKLIST = (
    "Scan microscope and PC serials; verify release and qualified host evidence.",
    "Match camera/stage serials, firmware, SDK and GPU to the qualified BOM.",
    "Prepare approved fixture, collision clearance, travel limits and emergency stop; operator signs readiness.",
    "Execute required CUDA kernels; reject skips and CPU fallback.",
    "Acquire each qualified camera mode; measure rate, timing, corruption and reconnect recovery.",
    "Home with fixture-specific limits; measure repeatability, limits and emergency stop.",
    "Fit each objective and its certified target; run geometry, dark, flat, colour and stage calibration.",
    "Measure autofocus against the certified target and profile limits.",
    "Capture, scan, stitch, reopen and compare with the approved reference image.",
    "Run combined-load endurance and controlled recovery; verify calibration and data after restart.",
    "Review immutable attempt reports; failures require rework and a new full attempt.",
)


def now():
    return datetime.now(timezone.utc).isoformat()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def encoded(value):
    return (json.dumps(value, indent=2, allow_nan=False, ensure_ascii=False) + "\n").encode("utf-8")


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.:-]{0,95}", value):
        raise ValueError("identity must be 1–96 letters, digits or . _ : -")
    return value


def load_profile(path):
    raw = path.read_bytes()
    # Reject duplicate keys as well as non-finite JSON constants.
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate profile key")
            result[key] = value
        return result
    profile = json.loads(raw, object_pairs_hook=pairs,
                         parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite profile value")))
    if set(profile) != {"schema", "profile_id", "revision", "qualifications", "devices", "objectives", "physical_limits"}:
        raise ValueError("unexpected profile fields")
    if type(profile["schema"]) is not int or profile["schema"] != 1:
        raise ValueError("unsupported profile schema")
    identifier(profile["profile_id"])
    if type(profile["revision"]) is not int or profile["revision"] < 1:
        raise ValueError("invalid profile revision")
    if not isinstance(profile["qualifications"], dict) or set(profile["qualifications"]) != set(QUALIFICATIONS):
        raise ValueError("qualification reference fields differ")
    for value in profile["qualifications"].values():
        if value is not None:
            identifier(value)
    if not isinstance(profile["devices"], dict) or set(profile["devices"]) != {"camera", "stage"}:
        raise ValueError("camera and stage identities required")
    for device in profile["devices"].values():
        if not isinstance(device, dict) or set(device) != {"model", "serial", "firmware"}:
            raise ValueError("invalid device fields")
        for value in device.values():
            if value is not None and (not isinstance(value, str) or not value.strip() or len(value) > 120):
                raise ValueError("invalid device identity")
    objectives = profile["objectives"]
    if not isinstance(objectives, list) or not objectives or len(set(map(str, objectives))) != len(objectives):
        raise ValueError("unique objectives required")
    for value in objectives:
        identifier(value)
    if not isinstance(profile["physical_limits"], dict) or set(profile["physical_limits"]) != set(LIMITS):
        raise ValueError("physical limit fields differ")
    for key, value in profile["physical_limits"].items():
        if value is None:
            continue
        try:
            valid = type(value) in (int, float) and math.isfinite(value) and value > 0
        except OverflowError:
            valid = False
        if not valid:
            raise ValueError("limits must be finite positive numbers or null")
        if key == "camera_test_frames" and type(value) is not int:
            raise ValueError("frame count must be an integer")
    return profile, digest(raw)


def factory_gaps(profile):
    gaps = ["Real-device acceptance adapter is not implemented (R09 physical qualification pending).",
            "Clean-PC and cold-boot qualification is pending (R10).",
            "Authenticated release verification is not implemented (R12)."]
    gaps += ["Missing qualification: " + k for k, v in profile["qualifications"].items() if v is None]
    gaps += ["Missing physical limit: " + k for k, v in profile["physical_limits"].items() if v is None]
    gaps += ["Missing device identity: " + kind + "." + k
             for kind, device in profile["devices"].items() for k, v in device.items() if v is None]
    return gaps


class Attempt:
    def __init__(self, output, profile_path, identity, mode, previous=None):
        profile, profile_hash = load_profile(profile_path)
        identity = {key: identifier(identity[key]) for key in ("instrument", "pc", "operator", "station")}
        rework = None
        if previous:
            if previous.name != "report.json":
                raise ValueError("previous must name report.json")
            old = verify_report(previous.parent)
            if any(old["identity"][k] != identity[k] for k in ("instrument", "pc")) or old["profile"]["sha256"] != profile_hash:
                raise ValueError("rework must match instrument, PC and exact profile")
            rework = {"run_id": old["run_id"], "report_sha256": digest(previous.read_bytes())}
        self.root = output.resolve() / str(uuid.uuid4())
        self.root.mkdir(parents=True, exist_ok=False)
        self.profile = profile
        self.finished = False
        self.report = {"schema": 1, "runner_version": VERSION, "run_id": self.root.name,
                       "mode": mode, "identity": identity,
                       "profile": {"id": profile["profile_id"], "revision": profile["revision"], "sha256": profile_hash},
                       "previous": rework, "started": now(), "finished": None,
                       "outcome": "in_progress", "shipment_authorized": False,
                       "physical_qualification": "not_run", "host_qualification": "not_run",
                       "release_qualification": "not_run", "steps": [], "metadata": {}}
        self.event("attempt_started", {"identity": identity, "mode": mode})
        self.artifact("profile.json", profile)

    def event(self, kind, value):
        with (self.root / "events.jsonl").open("ab") as stream:
            stream.write(encoded({"time": now(), "event": kind, "value": value}).replace(b"\n", b" ") + b"\n")
            stream.flush()
            import os
            os.fsync(stream.fileno())

    def artifact(self, name, value):
        if not re.fullmatch(r"[a-z0-9_-]+\.json", name):
            raise ValueError("unsafe evidence filename")
        path = self.root / name
        with path.open("xb") as stream:
            stream.write(encoded(value))
        return {"path": name, "sha256": digest(path.read_bytes())}

    def execute(self, actions, cleanup=lambda: {}):
        failed = False
        interrupted = False
        try:
            for name, function in actions:
                step = {"name": name, "status": "not_run"}
                self.report["steps"].append(step)
                if failed:
                    continue
                started = time.monotonic()
                step.update(status="running", started=now())
                self.event("step_started", {"name": name})
                try:
                    step["evidence"] = function() or {}
                    step["status"] = "passed"
                except (Exception, KeyboardInterrupt) as error:
                    failed = True
                    interrupted = isinstance(error, KeyboardInterrupt)
                    step.update(status="interrupted" if interrupted else "failed", error_type=type(error).__name__)
                    step["error_location"] = [{"file": Path(frame.filename).name, "line": frame.lineno}
                                              for frame in traceback.extract_tb(error.__traceback__)]
                    # Exception text and subprocess output may contain credentials; never export them.
                finally:
                    step.update(finished=now(), duration_seconds=round(time.monotonic() - started, 3))
                    self.event("step_finished", step)
        finally:
            cleanup_started = time.monotonic()
            step = {"name": "cleanup", "started": now()}
            try:
                step.update(evidence=cleanup() or {}, status="passed")
            except (Exception, KeyboardInterrupt) as error:
                failed = True
                step.update(status="failed", error_type=type(error).__name__)
            step["finished"] = now()
            step["duration_seconds"] = round(time.monotonic() - cleanup_started, 3)
            self.report["steps"].append(step)
            self.event("step_finished", step)
            self.finish("interrupted" if interrupted else "failed" if failed else "engineering_passed")
        return 130 if interrupted else 1 if failed else 0

    def finish(self, outcome):
        if self.finished:
            raise ValueError("attempt already finalized")
        if outcome not in ("blocked", "failed", "interrupted", "engineering_passed"):
            raise ValueError("unsupported outcome")
        if outcome == "engineering_passed" and (
                self.report["mode"] != "engineering" or
                [s["name"] for s in self.report["steps"]] != list(ENGINEERING_STEPS) or
                any(s["status"] != "passed" for s in self.report["steps"])):
            raise ValueError("engineering pass requires every engineering step")
        self.report.update(outcome=outcome, finished=now())
        self.event("attempt_finished", {"outcome": outcome, "shipment_authorized": False})
        self.artifact("report.json", self.report)
        title = "ENGINEERING ONLY — SHIPMENT NOT AUTHORIZED"
        body = html.escape(json.dumps(self.report, indent=2, ensure_ascii=False))
        with (self.root / "report.html").open("x", encoding="utf-8", newline="\n") as stream:
            stream.write('<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width">'
                         '<title>OScope acceptance evidence</title><style>body{font:16px system-ui;margin:2rem;max-width:80rem}'
                         'pre{white-space:pre-wrap;overflow-wrap:anywhere}h1{color:#923000}</style>'
                         '<h1>' + title + '</h1><p>Outcome: ' + html.escape(outcome) + '</p><pre>' + body + '</pre></html>')
        files = {p.name: digest(p.read_bytes()) for p in sorted(self.root.iterdir()) if p.is_file()}
        self.artifact("checksums.json", files)
        self.finished = True


def verify_report(root):
    manifest = json.loads((root / "checksums.json").read_bytes())
    if not isinstance(manifest, dict):
        raise ValueError("invalid evidence manifest")
    if not {"report.json", "report.html", "events.jsonl", "profile.json"} <= set(manifest):
        raise ValueError("incomplete evidence manifest")
    for name, expected in manifest.items():
        if not re.fullmatch(r"[a-z0-9_-]+\.(json|jsonl|html)", name):
            raise ValueError("unsafe manifest path")
        path = root / name
        if path.is_symlink() or digest(path.read_bytes()) != expected:
            raise ValueError("evidence checksum mismatch")
    report = json.loads((root / "report.json").read_bytes())
    if report["schema"] != 1 or report["shipment_authorized"] is not False or report["finished"] is None:
        raise ValueError("invalid report")
    if report["outcome"] not in ("blocked", "failed", "interrupted", "engineering_passed"):
        raise ValueError("invalid outcome")
    uuid.UUID(report["run_id"])
    if report["outcome"] == "engineering_passed":
        if report["mode"] != "engineering" or [s["name"] for s in report["steps"]] != list(ENGINEERING_STEPS):
            raise ValueError("incomplete engineering attempt")
        if any(s["status"] != "passed" for s in report["steps"]):
            raise ValueError("engineering attempt contains non-passing steps")
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("plan", "factory-preflight", "run-engineering"):
        sub = commands.add_parser(name)
        sub.add_argument("--profile", type=Path, required=True)
        if name != "plan":
            for field in ("instrument", "pc", "operator", "station"):
                sub.add_argument("--" + field, required=True)
            sub.add_argument("--output", type=Path, required=True)
            sub.add_argument("--previous", type=Path)
        if name == "run-engineering":
            sub.add_argument("--image", required=True, help="Locally available runtime image; never pulls")
    sub = commands.add_parser("verify")
    sub.add_argument("run", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "verify":
            report = verify_report(args.run)
            print(json.dumps({"run_id": report["run_id"], "outcome": report["outcome"], "checksums": "verified",
                              "shipment_authorized": False}))
            return 0
        profile, _ = load_profile(args.profile)
        if args.command == "plan":
            print(json.dumps({"profile": profile["profile_id"], "checklist": CHECKLIST,
                              "factory_blockers": factory_gaps(profile)}, indent=2))
            return 0
        identity = {key: getattr(args, key) for key in ("instrument", "pc", "operator", "station")}
        attempt = Attempt(args.output, args.profile, identity,
                          "engineering" if args.command == "run-engineering" else "factory_preflight", args.previous)
        print("Run directory: " + str(attempt.root), flush=True)
        if args.command == "factory-preflight":
            attempt.report["steps"] = [{"name": "factory_preflight", "status": "blocked", "gaps": factory_gaps(profile)}]
            attempt.finish("blocked")
            return 2
        from acceptance_workflows import Engineering
        engine = Engineering(attempt, args.image)
        result = attempt.execute(engine.actions(), engine.cleanup)
        print("Outcome: " + attempt.report["outcome"], flush=True)
        return result
    except (ValueError, OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        print("Acceptance input/evidence error: " + type(error).__name__, file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
