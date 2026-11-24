#!/usr/bin/env python3
"""Guard a clean engineering WSL template; never extracts or sanitizes an installed instrument."""
import argparse
import configparser
import hashlib
import json
import re
from pathlib import Path, PurePosixPath
import tarfile
import uuid


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def audit(path, baseline_id):
    before = path.stat()
    issues, documents = [], {}
    wanted = {"etc/machine-id", "etc/shadow", "etc/wsl.conf", "etc/oscope-host-baseline.json", "etc/os-release"}
    seen = set()
    with tarfile.open(path, "r:*") as archive:
        for index, member in enumerate(archive):
            if index > 200000:
                raise ValueError("template contains too many entries")
            name = PurePosixPath(member.name)
            if name.is_absolute() or ".." in name.parts or "\\" in member.name:
                issues.append("unsafe archive path: " + member.name)
                continue
            normalized = name.as_posix()
            if normalized in seen:
                issues.append("duplicate archive path: " + normalized)
            seen.add(normalized)
            if member.isdev() or member.isfifo():
                issues.append("device/FIFO entries are not allowed: " + normalized)
            state_roots = ("var/lib/oscope", "var/lib/oscope-host", "var/lib/docker", "var/log/journal")
            if not member.isdir() and (any(normalized == p or normalized.startswith(p + "/") for p in state_roots)
                    or "/.ssh/" in "/" + normalized or normalized.startswith("etc/ssh/ssh_host_")
                    or normalized.endswith("/.ssh")
                    or normalized.endswith(("/.bash_history", "/.python_history", "/.zsh_history"))):
                issues.append("configured state/credentials/history in template: " + normalized)
            if normalized in wanted:
                if not member.isfile() or member.size > 1024 * 1024:
                    issues.append("invalid template metadata entry: " + normalized)
                    continue
                with archive.extractfile(member) as stream:
                    documents[normalized] = stream.read().decode("utf-8")
    for name in wanted - set(documents):
        issues.append("missing template metadata: " + name)
    if documents.get("etc/machine-id", "").strip() not in ("", "uninitialized"):
        issues.append("template carries an existing Linux machine ID")
    for line in documents.get("etc/shadow", "").splitlines():
        fields = line.split(":")
        if len(fields) < 2 or not fields[1].startswith(("!", "*")):
            issues.append("template carries an unlocked/password-configured account")
    conf = configparser.ConfigParser()
    conf.read_string(documents.get("etc/wsl.conf", ""))
    if conf.get("boot", "systemd", fallback="false").lower() != "true":
        issues.append("template must enable systemd before import")
    if conf.get("interop", "appendWindowsPath", fallback="true").lower() != "false":
        issues.append("template must disable appending Windows executable paths")
    os_release = documents.get("etc/os-release", "")
    if not re.search(r'(?m)^ID="?ubuntu"?$', os_release):
        issues.append("template must be Ubuntu")
    if not re.search(r'(?m)^VERSION_ID="?24\.04"?$', os_release):
        issues.append("template must be Ubuntu 24.04")
    marker = json.loads(documents.get("etc/oscope-host-baseline.json", "{}"))
    if marker.get("baseline_id") != baseline_id or type(marker.get("schema_version")) is not int or marker["schema_version"] != 1:
        issues.append("template baseline identity differs")
    source_digest = digest(path)
    after = path.stat()
    if (before.st_ino, before.st_size, before.st_mtime_ns) != (after.st_ino, after.st_size, after.st_mtime_ns):
        issues.append("template changed while being audited")
    return {"schema_version": 1, "scope": "template_guard_only", "run_id": str(uuid.uuid4()),
            "baseline_id": baseline_id, "source_sha256": source_digest, "ready": not issues,
            "issues": issues, "qualification": "pending"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--baseline-id", required=True)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    try:
        result = audit(args.archive, args.baseline_id)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        with args.report.open("x", encoding="utf-8") as stream:
            json.dump(result, stream, indent=2)
            stream.write("\n")
        if not result["ready"]:
            parser.exit(2, "template guard failed; preserve the report and build a clean template\n")
    except (OSError, ValueError, tarfile.TarError) as error:
        parser.exit(2, f"template audit: {error}\n")


if __name__ == "__main__":
    main()
