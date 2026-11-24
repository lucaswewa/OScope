#!/usr/bin/env python3
"""R13 pilot preparation. Engineering observations never approve production."""
import argparse
from datetime import datetime, timezone
import html
import json
from pathlib import Path
import re
import sys
import traceback
import uuid

import pilot_metrics as m

CHECKS = ("cuda", "session", "measurements", "persistence", "cleanup")


def now():
    return datetime.now(timezone.utc).isoformat()


class Report:
    def __init__(self, output, profile, mode):
        self.profile, self.profile_sha = m.profile(profile)
        self.root = output.resolve() / str(uuid.uuid4())
        self.root.mkdir(parents=True, exist_ok=False)
        (self.root / "profile.json").write_bytes(profile.read_bytes())
        self.report = {"schema": 1, "run_id": self.root.name, "mode": mode, "outcome": "in_progress", "started": now(),
                       "finished": None, "metadata": {}, "context": {"profile_sha256": self.profile_sha}, "image_id": None,
                       "checks": {key: False for key in CHECKS}, "metrics": {}, "references": {},
                       "production_qualified": False, "optimization_approved": False, "shipment_authorized": False}

    def artifact(self, name, value):
        m.check(re.fullmatch(r"[a-z0-9_-]+\.json", name), "unsafe evidence filename")
        with (self.root / name).open("xb") as stream:
            stream.write(m.encoded(value))
        return {"path": name, "sha256": m.sha(self.root / name)}

    def finish(self, outcome):
        m.check(outcome in ("observed", "blocked", "failed", "interrupted"), "unsupported outcome")
        if outcome == "observed":
            m.check(all(self.report["checks"].values()), "all functional checks must complete")
        self.report.update(outcome=outcome, finished=now())
        self.artifact("report.json", self.report)
        body = html.escape(json.dumps(self.report, indent=2))
        with (self.root / "report.html").open("x", encoding="utf-8", newline="\n") as stream:
            stream.write('<!doctype html><html lang="en"><meta charset="utf-8"><title>R13 engineering observations</title>'
                         '<style>body{font:16px system-ui;margin:2rem}pre{white-space:pre-wrap;overflow-wrap:anywhere}</style>'
                         '<h1>Engineering observations — production not qualified</h1><pre>' + body + '</pre></html>')
        self.artifact("checksums.json", {p.name: m.sha(p) for p in sorted(self.root.iterdir()) if p.is_file()})


def verify(root):
    files = m.loads((root / "checksums.json").read_bytes())
    m.check(isinstance(files, dict) and {"report.json", "report.html", "profile.json"} <= set(files), "incomplete report")
    for name, expected in files.items():
        m.check(isinstance(name, str) and re.fullmatch(r"[a-z0-9_-]+\.(json|jsonl|html)", name), "unsafe evidence path")
        path = root / name
        m.check(not path.is_symlink() and path.stat().st_size <= 16 * 1024**2 and m.sha(path) == expected, "evidence checksum differs")
    value = m.loads((root / "report.json").read_bytes())
    m.check(value["schema"] == 1 and str(uuid.UUID(value["run_id"])) == value["run_id"] and value["finished"] is not None, "invalid report identity")
    m.check(all(value[k] is False for k in ("production_qualified", "optimization_approved", "shipment_authorized")), "engineering report cannot claim approval")
    m.check(set(value["checks"]) == set(CHECKS) and all(type(v) is bool for v in value["checks"].values()), "invalid functional checks")
    m.check(value["context"]["profile_sha256"] == m.sha(root / "profile.json"), "profile hash differs")
    m.check(value["outcome"] in ("observed", "blocked", "failed", "interrupted"), "invalid outcome")
    if value["outcome"] == "observed":
        m.check(value["mode"] == "engineering" and all(value["checks"].values()), "incomplete engineering observations")
        m.check({"samples.jsonl", "cuda.json"} <= set(files), "resource/kernel evidence missing")
        cuda = m.loads((root / "cuda.json").read_bytes())
        m.check(cuda["required"] is True and cuda["kernel_execution"] == "passed" and
                "Cuda  available" in cuda["stdout"] and "smoke: ok" in cuda["stdout"], "required CUDA evidence missing")
        bursts = value["context"]["workflow_bursts"]
        m.check(type(bursts) is int and 1 <= bursts <= 10 and
                {name for name in files if name.startswith("workflow_")} == {"workflow_" + str(i) + ".json" for i in range(bursts)}, "workflow evidence missing")
        samples = [m.loads(line) for line in (root / "samples.jsonl").read_bytes().splitlines()]
        m.check(2 <= len(samples) <= 4096 and all(s["t"] <= value["context"]["seconds"] + value["context"]["interval"] for s in samples), "sample coverage invalid")
        workflows = [m.loads((root / ("workflow_" + str(i) + ".json")).read_bytes()) for i in range(bursts)]
        metrics, references = m.summarize(samples, workflows)
        m.check(metrics == value["metrics"] and references == value["references"], "summary differs from retained raw measurements")
        for key in m.METRICS:
            m.number(value["metrics"][key])
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("plan", "preflight", "run-engineering"):
        sub = commands.add_parser(name)
        sub.add_argument("--profile", type=Path, required=True)
        if name != "plan":
            sub.add_argument("--output", type=Path, required=True)
        if name == "run-engineering":
            sub.add_argument("--image", required=True)
            sub.add_argument("--seconds", type=int, default=45)
            sub.add_argument("--interval", type=int, default=2)
    sub = commands.add_parser("verify")
    sub.add_argument("run", type=Path)
    sub = commands.add_parser("compare")
    sub.add_argument("--profile", type=Path, required=True)
    sub.add_argument("--baseline", action="append", type=Path, required=True)
    sub.add_argument("--candidate", action="append", type=Path, required=True)
    sub.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "verify":
            r = verify(args.run)
            print(json.dumps({"run_id": r["run_id"], "outcome": r["outcome"], "checksums": "verified", "production_qualified": False}))
            return 0
        p, fingerprint = m.profile(args.profile)
        if args.command == "plan":
            print(json.dumps({"pilot": p["pilot_id"], "gaps": m.gaps(p), "workload": p["workload_id"],
                              "note": "Short observations are not endurance, 20 MP performance or production qualification."}, indent=2))
            return 0
        if args.command == "compare":
            result = m.compare([verify(path) for path in args.baseline], [verify(path) for path in args.candidate], p, fingerprint)
            result["evidence"] = {"baseline": [m.sha(path / "report.json") for path in args.baseline],
                                  "candidate": [m.sha(path / "report.json") for path in args.candidate]}
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open("xb") as stream:
                stream.write(m.encoded(result))
            print("Comparison: " + result["status"])
            return 0 if result["status"] == "comparison_available" else 2
        if args.command == "run-engineering":
            m.check(30 <= args.seconds <= 14400 and 1 <= args.interval <= 30 and args.seconds / args.interval <= 4000,
                    "use 30–14400 seconds, interval 1–30, at most 4000 samples")
        report = Report(args.output, args.profile, "engineering" if args.command == "run-engineering" else "preflight")
        print("Run directory: " + str(report.root), flush=True)
        if args.command == "preflight":
            report.report["gaps"] = m.gaps(p)
            report.finish("blocked")
            return 2
        from pilot_runtime import Observer
        observer = Observer(report, args.image, args.seconds, args.interval)
        outcome = "observed"
        try:
            observer.run()
            if any(value["status"] == "failed" for value in report.report["references"].values()):
                outcome = "failed"
        except (Exception, KeyboardInterrupt) as error:
            outcome = "interrupted" if isinstance(error, KeyboardInterrupt) else "failed"
            report.report["error"] = {"type": type(error).__name__, "location": [
                {"file": Path(frame.filename).name, "line": frame.lineno} for frame in traceback.extract_tb(error.__traceback__)]}
        finally:
            try:
                observer.cleanup()
                report.report["checks"]["cleanup"] = True
            except (Exception, KeyboardInterrupt) as error:
                outcome = "failed"
                report.report["cleanup_error_type"] = type(error).__name__
            report.finish(outcome)
        print("Outcome: " + outcome + "; production remains unqualified", flush=True)
        return 0 if outcome == "observed" else 130 if outcome == "interrupted" else 1
    except (ValueError, OSError, KeyError, TypeError) as error:
        print("pilot input/evidence: " + type(error).__name__, file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
