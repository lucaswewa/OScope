"""R13 profiles, source-preserving leak references and guarded comparisons."""
import hashlib
import json
import math
from pathlib import Path
import re
import statistics

METRICS = ("preview_window_mean_p95_ms", "workflow_median_seconds", "rss_peak_bytes", "fds_peak", "preview_drop_per_sent")
QUALIFICATIONS = ("hardware", "host", "factory_acceptance", "product_release")
CONTEXT = ("host_fingerprint", "gpu_identity", "kernel", "docker_version", "simulator_config_sha256", "profile_sha256",
           "workload_id", "workflow_bursts", "seconds", "interval", "runner_sources")


def check(condition, message):
    if not condition:
        raise ValueError(message)


def encoded(value):
    return (json.dumps(value, indent=2, allow_nan=False) + "\n").encode()


def loads(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            check(key not in result, "duplicate JSON key")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite JSON")))


def sha(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def number(value, minimum=0):
    try:
        valid = type(value) in (int, float) and math.isfinite(value) and value >= minimum
    except OverflowError:
        valid = False
    check(valid, "finite numeric measurement/limit required")
    return value


def identifier(value):
    check(isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.:-]{0,95}", value), "invalid identity")


def profile(path):
    raw = path.read_bytes()
    p = loads(raw)
    fields = {"schema", "pilot_id", "revision", "qualifications", "cohort", "required_units_per_variant", "required_endurance_seconds",
              "minimum_comparison_runs", "allowed_regression_percent", "workload_id", "workflow_bursts", "budgets"}
    check(isinstance(p, dict) and set(p) == fields, "profile fields differ")
    check(type(p["schema"]) is int and p["schema"] == 1 and type(p["revision"]) is int and p["revision"] > 0, "invalid profile version")
    identifier(p["pilot_id"])
    check(isinstance(p["qualifications"], dict) and set(p["qualifications"]) == set(QUALIFICATIONS), "qualification fields differ")
    for value in p["qualifications"].values():
        if value is not None:
            identifier(value)
    check(isinstance(p["cohort"], list) and len(p["cohort"]) <= 300, "invalid pilot cohort")
    for unit in p["cohort"]:
        check(isinstance(unit, dict) and set(unit) == {"instrument", "pc", "variant"}, "invalid cohort unit")
        for value in unit.values():
            identifier(value)
    for key in ("instrument", "pc"):
        check(len({u[key] for u in p["cohort"]}) == len(p["cohort"]), "duplicate cohort identity")
    for key in ("required_units_per_variant", "required_endurance_seconds"):
        check(p[key] is None or (type(p[key]) is int and p[key] > 0), "positive integer pilot requirement or null required")
    check(type(p["minimum_comparison_runs"]) is int and 3 <= p["minimum_comparison_runs"] <= 20, "at least three independent runs required")
    check(p["workload_id"] == "r07-quarter-preview-bursts-v1" and type(p["workflow_bursts"]) is int and 1 <= p["workflow_bursts"] <= 10,
          "unsupported or unbounded workload")
    check(isinstance(p["budgets"], dict) and set(p["budgets"]) == set(METRICS), "budget fields differ")
    for value in [p["allowed_regression_percent"], *p["budgets"].values()]:
        if value is not None:
            number(value)
    return p, hashlib.sha256(raw).hexdigest()


def gaps(p):
    result = ["Physical pilot adapter and authenticated qualification evidence verification are not implemented."]
    result += ["Missing qualification: " + key for key, value in p["qualifications"].items() if value is None]
    if not p["cohort"]:
        result.append("Representative instrument/PC/variant cohort is not selected.")
    for key in ("required_units_per_variant", "required_endurance_seconds", "allowed_regression_percent"):
        if p[key] is None:
            result.append("Missing pilot requirement: " + key)
    result += ["Missing approved budget: " + key for key, value in p["budgets"].items() if value is None]
    return result


def slope(points):
    """P53 detector: median of all forward pairwise slopes, unchanged definition."""
    values = [(b[1] - a[1]) / (b[0] - a[0]) for i, a in enumerate(points) for b in points[i + 1:] if b[0] > a[0]]
    return statistics.median(values) if values else 0.0


def leak_reference(points, floor, fraction):
    check(len(points) <= 4096, "too many resource samples")
    for index, point in enumerate(points):
        check(len(point) == 2, "invalid resource point")
        number(point[0])
        number(point[1])
        check(index == 0 or point[0] > points[index - 1][0], "sample clock must increase")
    start = max(300.0, points[-1][0] / 4 if points else 0)
    steady = [p for p in points if p[0] >= start]
    result = {"scope": "legacy_software_reference", "warm_up_seconds": start, "steady_samples": len(steady),
              "floor": floor, "fraction": fraction, "status": "insufficient_evidence", "rate_per_hour": None}
    if len(steady) < 10:
        return result
    rate = slope(steady)
    growth = rate * (steady[-1][0] - steady[0][0])
    level = statistics.median(p[1] for p in steady)
    result.update(status="failed" if growth > floor and growth > fraction * level else "passed",
                  rate_per_hour=rate * 3600, estimated_growth=growth, median_level=level)
    return result


def percentile95(values):
    check(bool(values), "no samples")
    return sorted(values)[math.ceil(0.95 * len(values)) - 1]  # Nearest-rank p95, no confidence claim.


def summarize(samples, workflows):
    check(bool(samples) and bool(workflows), "missing measurements")
    for row in samples:
        for key in ("t", "rss_bytes", "fds", "cpu_seconds", "preview_window_mean_ms", "sent", "dropped"):
            number(row[key])
        check(row["gpu"] is True, "required GPU preview was not measured")
    for a, b in zip(samples, samples[1:]):
        check(b["t"] > a["t"] and b["sent"] >= a["sent"] and b["dropped"] >= a["dropped"], "measurement counters reset")
    sent, dropped = samples[-1]["sent"], samples[-1]["dropped"]
    check(sent > 0, "no preview frames")
    timings = [number(w["seconds"]) for w in workflows]
    references = {"memory": leak_reference([(s["t"], s["rss_bytes"]) for s in samples], 32 * 1024**2, .05),
                  "fds": leak_reference([(s["t"], s["fds"]) for s in samples], 32, .10),
                  "preview_drop": {"status": "passed" if dropped <= .01 * sent else "failed", "sent": sent, "dropped": dropped,
                                   "limit": "dropped <= 0.01 * sent", "scope": "legacy_software_reference"}}
    references["memory"]["unit"] = "bytes"
    references["fds"]["unit"] = "file_descriptors"
    metrics = dict(zip(METRICS, (percentile95([s["preview_window_mean_ms"] for s in samples]), statistics.median(timings),
                                max(s["rss_bytes"] for s in samples), max(s["fds"] for s in samples), dropped / sent)))
    return metrics, references


def compare(baseline, candidate, p, profile_sha):
    issues = []
    reports = baseline + candidate
    ids = [r["run_id"] for r in reports]
    check(len(set(ids)) == len(ids), "duplicate run IDs cannot count as independent repetitions")
    if min(len(baseline), len(candidate)) < p["minimum_comparison_runs"]:
        issues.append("Insufficient independent baseline/candidate runs.")
    for r in reports:
        if r["outcome"] != "observed" or not all(r["checks"].values()):
            issues.append("A run did not complete all required functional checks.")
        if r["context"].get("profile_sha256") != profile_sha:
            issues.append("Run profile differs from selected profile.")
        if r["context"].get("other_containers"):
            issues.append("Concurrent containers were observed; timings are not an isolated comparison.")
        if any(r["references"].get(key, {}).get("status") != "passed" for key in ("memory", "fds", "preview_drop")):
            issues.append("Reference soak evidence failed or is insufficient.")
        for key in METRICS:
            if r.get("metrics", {}).get(key) is None:
                issues.append("A required metric was not measured.")
    if reports:
        for key in CONTEXT:
            if any(r["context"].get(key) is None or r["context"].get(key) != reports[0]["context"].get(key) for r in reports):
                issues.append("Incompatible comparison context: " + key)
    images = [{r["image_id"] for r in group} for group in (baseline, candidate)]
    if any(len(group) != 1 for group in images):
        issues.append("Each side must measure exactly one immutable image.")
    result = {"schema": 1, "scope": "engineering_comparison", "status": "not_comparable", "issues": sorted(set(issues)),
              "optimization_approved": False, "production_qualified": False, "shipment_authorized": False}
    if issues:
        return result
    values, failed, missing = {}, False, False
    for key in METRICS:
        before = statistics.median(number(r["metrics"][key]) for r in baseline)
        after = statistics.median(number(r["metrics"][key]) for r in candidate)
        change = (after / before - 1) * 100 if before else None
        budget = p["budgets"][key]
        regression = p["allowed_regression_percent"]
        status = "not_configured" if budget is None or regression is None else "failed" if (
            after > budget or (before == 0 and after > 0) or (change is not None and change > regression)) else "within_configured_limits"
        missing |= status == "not_configured"
        failed |= status == "failed"
        values[key] = {"baseline_median": before, "candidate_median": after, "change_percent": change, "max_budget": budget, "status": status}
    result.update(status="regression_detected" if failed else "budgets_missing" if missing else "comparison_available",
                  metrics=values, same_image_control=images[0] == images[1],
                  note="Configured limits and run medians do not approve an optimization or establish statistical confidence.")
    return result
