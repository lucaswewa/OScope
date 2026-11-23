#!/usr/bin/env python3
"""Read-only Linux USB/SDK preflight. Never opens devices or loads the SDK."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import stat
import struct
import sys
import tomllib
import uuid


def load_profile(path):
    raw = path.read_bytes()
    profile = tomllib.loads(raw.decode("utf-8-sig"))
    if set(profile) != {"version", "camera", "stage", "sdk"}:
        raise ValueError("profile requires only version, camera, stage and sdk")
    if type(profile["version"]) is not int or profile["version"] != 1:
        raise ValueError("unsupported hardware profile version")
    for role in ("camera", "stage", "sdk"):
        fields = {"library", "sha256"} if role == "sdk" else {"vid", "pid", "serial"}
        table = profile[role]
        if not isinstance(table, dict) or set(table) != fields:
            raise ValueError(f"{role}: expected fields {sorted(fields)}")
        if any(not isinstance(value, str) or value != value.strip() for value in table.values()):
            raise ValueError(f"{role}: fields must be strings without surrounding whitespace")
        for field in fields & {"vid", "pid"}:
            if table[field] and not re.fullmatch(r"[a-fA-F0-9]{4}", table[field]):
                raise ValueError(f"{role}.{field}: expected four hexadecimal digits")
            table[field] = table[field].lower()
    sdk = profile["sdk"]
    if sdk["library"] and not Path(sdk["library"]).is_absolute():
        raise ValueError("sdk.library must be an absolute path")
    if sdk["sha256"] and not re.fullmatch(r"[a-fA-F0-9]{64}", sdk["sha256"]):
        raise ValueError("sdk.sha256 must be a SHA-256 hex digest")
    sdk["sha256"] = sdk["sha256"].lower()
    if all(profile["camera"].values()) and profile["camera"] == profile["stage"]:
        raise ValueError("camera and stage must identify different USB devices")
    return profile, hashlib.sha256(raw).hexdigest()


def node_status(path):
    try:
        return {"path": str(path), "character_device": stat.S_ISCHR(path.stat().st_mode),
                "apparent_read_write_access": os.access(path, os.R_OK | os.W_OK)}
    except OSError:
        return {"path": str(path), "character_device": False, "apparent_read_write_access": False}


def usb_inventory(sysfs=Path("/sys/bus/usb/devices"), dev=Path("/dev/bus/usb")):
    devices, errors = [], []
    if not sysfs.is_dir():
        return [], ["USB sysfs is unavailable"]
    for entry in sorted(sysfs.iterdir()):
        if not (entry / "idVendor").exists():
            continue  # USB interfaces have no separate vendor/product identity.
        try:
            def read(name):
                target = entry / name
                return target.read_text(encoding="utf-8").strip() if target.exists() else ""
            bus, number = int(read("busnum")), int(read("devnum"))
            devices.append({"sysfs_name": entry.name, "vid": read("idVendor").lower(),
                            "pid": read("idProduct").lower(), "serial": read("serial"),
                            "manufacturer": read("manufacturer"), "product": read("product"),
                            "speed_mbps": read("speed"), "bus": bus, "device": number,
                            "node": node_status(dev / f"{bus:03d}" / f"{number:03d}")})
        except (OSError, ValueError) as error:
            errors.append(f"{entry.name}: cannot read USB identity: {error}")
    return devices, errors


def sdk_inventory(settings):
    result = {"path": settings["library"], "sha256": None, "elf64_x86_64_shared": False,
              "expected_sha256_matches": False, "loaded": False, "issues": []}
    issues = result["issues"]
    if not settings["library"]:
        issues.append("Linux SDK library path is not configured")
        return result
    path = Path(settings["library"])
    try:
        if not path.is_file():
            raise ValueError("not a regular SDK file")
        digest = hashlib.sha256()
        with path.open("rb") as stream:
            header = stream.read(64)
            digest.update(header)
            for block in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(block)
        result["sha256"] = digest.hexdigest()
        # Metadata only: ELF class, byte order, version, ET_DYN and EM_X86_64.
        # This does not establish that the library loads or its ABI is usable.
        result["elf64_x86_64_shared"] = (
            len(header) == 64 and header[:7] == b"\x7fELF\x02\x01\x01"
            and struct.unpack_from("<HH", header, 16) == (3, 62))
        result["expected_sha256_matches"] = result["sha256"] == settings["sha256"]
        if not result["elf64_x86_64_shared"]:
            issues.append("SDK is not an ELF64 little-endian x86_64 shared object")
        if not settings["sha256"]:
            issues.append("Expected SDK SHA-256 is not configured; review and pin the observed digest")
        elif not result["expected_sha256_matches"]:
            issues.append("SDK SHA-256 differs from the instrument profile")
    except (OSError, ValueError) as error:
        issues.append(f"Cannot inspect SDK file: {error}")
    return result


def check_prerequisites(profile, devices, sdk, system, machine, uid):
    issues, matched = [], {}
    if system != "Linux" or machine.lower() not in ("x86_64", "amd64"):
        issues.append("Collector must run on Linux x86_64")
    if uid == 0:
        issues.append("Run as the actual non-root service/developer user to check apparent permissions")
    for role in ("camera", "stage"):
        expected = profile[role]
        if not all(expected.values()):
            issues.append(f"{role}: configure VID, PID and a stable serial; bus numbers are not identity")
            matched[role] = []
            continue
        found = [d for d in devices if all(d[key] == value for key, value in expected.items())]
        matched[role] = found
        if len(found) != 1:
            issues.append(f"{role}: expected exactly one matching device, found {len(found)}")
        elif not (found[0]["node"]["character_device"] and found[0]["node"]["apparent_read_write_access"]):
            issues.append(f"{role}: matching USB node lacks apparent non-root read/write access")
    issues.extend(sdk["issues"])
    return {"ready": not issues, "issues": issues, "matched": matched}


def collect(profile_path):
    profile, profile_hash = load_profile(profile_path)
    devices, errors = usb_inventory()
    sdk = sdk_inventory(profile["sdk"])
    system, machine = platform.system(), platform.machine()
    uid = os.geteuid() if hasattr(os, "geteuid") else None
    preflight = check_prerequisites(profile, devices, sdk, system, machine, uid)
    preflight["issues"].extend(errors)
    preflight["ready"] = not preflight["issues"]
    image = os.environ.get("OSCOPE_HARDWARE_IMAGE_ID")
    if image and not re.fullmatch(r"sha256:[0-9a-f]{64}", image):
        raise ValueError("OSCOPE_HARDWARE_IMAGE_ID must be a frozen Docker image ID")
    return {"schema_version": 1, "scope": "inventory_only", "run_id": str(uuid.uuid4()),
            "collected_at_utc": datetime.now(timezone.utc).isoformat(),
            "collector_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "profile_sha256": profile_hash, "backend_image_id": image,
            "environment": {"system": system, "machine": machine, "kernel": platform.release(),
                            "uid": uid, "container": Path("/.dockerenv").exists()},
            "usb": devices, "sdk": sdk, "preflight": preflight,
            "qualification": {"status": "not_run", "pending": [
                "SDK load and camera access", "sustained acquisition and image correctness",
                "stage limits, stop and motion", "USB reconnect and lifecycle recovery"]}}


def write_report(report, target):
    text = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    if target == "-":
        print(text, end="")
    else:
        path = Path(target)
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("x", encoding="utf-8", newline="\n") as stream:
            stream.write(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", required=True, type=Path)
    parser.add_argument("--report", required=True, help="New report path, or - for stdout")
    parser.add_argument("--require-ready", action="store_true", help="Fail on incomplete inventory prerequisites")
    args = parser.parse_args()
    try:
        report = collect(args.profile)
        write_report(report, args.report)
    except (OSError, ValueError) as error:
        parser.exit(2, f"hardware inventory: {error}\n")
    ready = report["preflight"]["ready"]
    print(f"Inventory collected; prerequisites {'ready' if ready else 'incomplete'}; hardware qualification NOT RUN",
          file=sys.stderr)
    if args.require_ready and not ready:
        parser.exit(2)


if __name__ == "__main__":
    main()
