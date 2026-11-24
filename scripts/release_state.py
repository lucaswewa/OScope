"""Release transactions and inventory; private data is never restored automatically."""
from contextlib import contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import time
import traceback
import uuid

import release_bundle as b


def now():
    return datetime.now(timezone.utc).isoformat()


def read(path):
    return b.loads(path.read_bytes())


@contextmanager
def locked(root):
    import fcntl
    b.check(root.is_absolute() and root.resolve() == root and len(root.parts) >= 4,
            "dedicated absolute installation root without symlinks required")
    b.check(root.is_dir() and not any(p.is_symlink() for p in (root, *root.parents)), "existing owned directory required")
    with (root / ".release.lock").open("a") as stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as error:
            raise ValueError("installation is busy") from error
        yield


def history(root):
    path = root / "history.jsonl"
    values, previous = [], "0" * 64
    if path.exists():
        for line in path.read_bytes().splitlines():
            entry = b.loads(line)
            b.check(set(entry) == {"previous", "event", "sha256"} and entry["previous"] == previous, "history chain differs")
            expected = hashlib.sha256(b.dumps({"previous": previous, "event": entry["event"]})).hexdigest()
            b.check(entry["sha256"] == expected, "history checksum differs")
            previous = expected
            values.append(entry)
    return values


def record(root, event):
    entries = history(root)
    previous = entries[-1]["sha256"] if entries else "0" * 64
    payload = {"previous": previous, "event": event}
    entry = {**payload, "sha256": hashlib.sha256(b.dumps(payload)).hexdigest()}
    with (root / "history.jsonl").open("ab") as stream:
        stream.write(json.dumps(entry, separators=(",", ":"), allow_nan=False).encode() + b"\n")
        stream.flush()
        os.fsync(stream.fileno())


def instance(root):
    value = read(root / "installation.json")
    fields = {"schema", "scope", "installation_id", "instrument_serial", "pc_serial", "baseline_id", "port", "linux_versions", "trust_sha256"}
    b.check(isinstance(value, dict) and set(value) == fields and value["schema"] == 1 and value["scope"] == b.SCOPE,
            "not an R12 engineering installation; R10 roots are not converted")
    b.check(str(uuid.UUID(value["installation_id"])) == value["installation_id"], "invalid installation identity")
    for name in ("instrument_serial", "pc_serial", "baseline_id"):
        b.identifier(value[name])
    b.check(type(value["port"]) is int and 1024 <= value["port"] <= 65535, "invalid loopback port")
    b.check(b.sha(root / "trust/trust.json") == value["trust_sha256"], "installed trust policy changed; explicit trust upgrade is not implemented")
    entries = history(root)
    b.check(entries and entries[0]["event"]["type"] == "initialized" and
            entries[0]["event"]["identity_sha256"] == b.sha(root / "installation.json"), "installation identity changed")
    return value


def state(root):
    return read(root / "state.json") if (root / "state.json").exists() else {"active": None, "previous": None, "high_water": 0}


def selection(manifest, fingerprint):
    return {"manifest_sha256": fingerprint, "release_id": manifest["release_id"], "sequence": manifest["sequence"],
            "image_id": manifest["backend"]["image_id"], "revision": manifest["backend"]["revision"], "schemas": manifest["schemas"]}


def cache(root, directory):
    trust = root / "trust/trust.json"
    manifest, fingerprint = b.authenticated_manifest(directory, trust)
    cached = root / "releases" / fingerprint
    cached.parent.mkdir(exist_ok=True)
    if not cached.exists():
        # Copy into the owned root, then verify again: do not load mutable source media.
        temporary = cached.with_name(fingerprint + "." + uuid.uuid4().hex + ".partial")
        shutil.copytree(directory, temporary)
        checked, checked_hash = b.verify(temporary, trust)
        b.check(checked == manifest and checked_hash == fingerprint, "source changed while staging release")
        temporary.rename(cached)
    else:
        checked, checked_hash = b.verify(cached, trust)
    b.check(checked == manifest and checked_hash == fingerprint, "cached release differs")
    return selection(manifest, fingerprint), cached


def compatible(identity, current, target, manifest, rollback=False):
    b.check(identity["baseline_id"] in manifest["baseline_ids"], "release does not support installed baseline")
    b.check(target["schemas"] == {"data": 1, "config": 2}, "schema migration is not implemented")
    if current["active"]:
        b.check(current["active"]["schemas"] == target["schemas"], "rollback/update schemas are incompatible")
    if not rollback:
        b.check(target["sequence"] > current["high_water"], "normal updates must exceed sequence high-water mark")


def phase(root, transaction, name):
    transaction["phase"] = name
    transaction["updated"] = now()
    b.atomic(root / "pending.json", transaction)


def change(root, directory, driver, rollback=False):
    inventory(root)  # Validate persisted identity, receipts and active history before mutation.
    identity = instance(root)
    current = state(root)
    pending = read(root / "pending.json") if (root / "pending.json").exists() else None
    if rollback:
        if pending:
            b.check(pending["phase"] == "recovered_stopped", "run recover before rollback of an incomplete change")
            wanted = pending["before"]["active"]
        else:
            wanted = current["previous"]
        b.check(wanted is not None, "no previous verified release; initial-install recovery requires diagnosis")
        directory = root / "releases" / wanted["manifest_sha256"]
    else:
        b.check(pending is None and not (root / "maintenance.json").exists(), "installation is in maintenance; recover and explicitly rollback")
    target, cached = cache(root, directory)
    manifest = read(cached / "manifest.json")
    compatible(identity, current, target, manifest, rollback)
    if rollback:
        b.check(target == wanted, "previous release metadata differs")
    if pending:
        driver.prepare_recovery(identity, current, pending)
    driver.prepare(identity, current, target, cached)
    if not pending:
        driver.idle()
    operation_id = str(uuid.uuid4())
    operation = root / "operations" / operation_id
    operation.mkdir(parents=True, exist_ok=False, mode=0o700)
    transaction = {"operation_id": operation_id, "kind": "rollback" if rollback else "install" if not current["active"] else "update",
                   "started": now(), "before": current, "target": target, "replaces_pending": pending["operation_id"] if pending else None}
    phase(root, transaction, "prepared")
    b.atomic(root / "maintenance.json", {"operation_id": operation_id})
    record(root, {"type": "started", "operation_id": operation_id, "time": now(), "kind": transaction["kind"],
                  "from": current["active"], "to": target})
    started = time.monotonic()
    try:
        phase(root, transaction, "stopping")
        driver.stop()
        phase(root, transaction, "stopped")
        backup = driver.snapshot(operation)
        phase(root, transaction, "backed_up")
        driver.switch(target, cached)
        phase(root, transaction, "validating")
        evidence = driver.validate(target)
        updated = {"active": target, "previous": current["active"], "high_water": max(current["high_water"], target["sequence"])}
        b.atomic(root / "state.json", updated)
        phase(root, transaction, "committed")
        outcome = "passed"
        result = {"evidence": evidence, "backup": backup}
    except (Exception, KeyboardInterrupt) as error:
        # Stop the candidate, preserve every data file, and require explicit recovery.
        outcome = "interrupted" if isinstance(error, KeyboardInterrupt) else "failed"
        result = {"error_type": type(error).__name__, "failure_stage": transaction["phase"],
                  "error_location": [{"file": Path(frame.filename).name, "line": frame.lineno}
                                     for frame in traceback.extract_tb(error.__traceback__)]}
        try:
            driver.stop()
        except Exception as cleanup_error:
            result["stop_error_type"] = type(cleanup_error).__name__
        phase(root, transaction, outcome)
    receipt = {"schema": 1, "scope": b.SCOPE, "operation_id": operation_id,
               "installation_id": identity["installation_id"], "instrument_serial": identity["instrument_serial"],
               "pc_serial": identity["pc_serial"], "kind": transaction["kind"], "started": transaction["started"],
               "finished": now(), "duration_seconds": round(time.monotonic() - started, 3), "outcome": outcome,
               "from": current["active"], "to": target, "production_qualification": "pending", "shipment_authorized": False,
               "tool_sha256": {name: b.sha(Path(__file__).parent / name) for name in ("release.py", "release_bundle.py", "release_state.py", "release_runtime.py")}, **result}
    with (operation / "receipt.json").open("xb") as stream:
        stream.write(b.dumps(receipt))
        stream.flush()
        os.fsync(stream.fileno())
    record(root, {"type": "finished", "time": now(), "operation_id": operation_id, "outcome": outcome,
                  "receipt_sha256": b.sha(operation / "receipt.json")})
    if outcome == "passed":
        (root / "maintenance.json").unlink()
        (root / "pending.json").unlink()
    return receipt


def recover(root, driver):
    identity = instance(root)
    b.check((root / "pending.json").is_file(), "no incomplete operation")
    pending = read(root / "pending.json")
    driver.prepare_recovery(identity, state(root), pending)
    driver.stop()
    phase(root, pending, "recovered_stopped")
    b.atomic(root / "maintenance.json", {"operation_id": pending["operation_id"], "state": "recovered_stopped"})
    record(root, {"type": "recovered_stopped", "time": now(), "operation_id": pending["operation_id"]})
    return {"installation_id": identity["installation_id"], "state": "maintenance", "action": "explicit compatible rollback required"}


def inventory(root):
    identity, current = instance(root), state(root)
    entries = history(root)
    passed = []
    for entry in entries:
        event = entry["event"]
        if event["type"] == "finished":
            path = root / "operations" / str(uuid.UUID(event["operation_id"])) / "receipt.json"
            b.check(b.sha(path) == event["receipt_sha256"], "operation receipt differs")
            receipt = read(path)
            b.check(receipt["installation_id"] == identity["installation_id"], "receipt belongs to another installation")
            if receipt["outcome"] == "passed":
                passed.append(receipt)
    maintenance = (root / "pending.json").exists() or (root / "maintenance.json").exists()
    if not maintenance and passed:
        b.check(current["active"] == passed[-1]["to"] and current["high_water"] == max(r["to"]["sequence"] for r in passed), "active release history differs")
    return {"schema": 1, "scope": b.SCOPE, "installation_id": identity["installation_id"],
            "instrument_serial": identity["instrument_serial"], "pc_serial": identity["pc_serial"], "baseline_id": identity["baseline_id"],
            "state": "maintenance" if maintenance else "ready" if current["active"] else "uninitialized",
            **current, "history_events": len(entries), "history_tip": entries[-1]["sha256"] if entries else None,
            "runtime_observation": "not_probed", "production_qualification": "pending", "physical_acceptance": "not_run", "shipment_authorized": False}
