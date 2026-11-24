#!/usr/bin/env python3
"""Select independent gates from committed and local changes; no Node or Cargo."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
NAMES = ("docs", "environment", "backend", "cpu", "frontend", "integration", "extensions", "workflows", "runtime", "hardware", "provisioning")
BACKEND_INPUTS = ("Cargo.toml", "Cargo.lock", "LICENSE", "rust-toolchain.toml", ".cargo/", "crates/", "python/",
                  "examples/", "xtask/", "configs/runtime.toml", "containers/", ".dockerignore")


def git(root, *args):
    return subprocess.check_output(["git", "-c", "safe.directory=" + str(root), *args], cwd=root)


def changed_files(root, base, head="HEAD", local=True):
    """No rename collapsing: old/deleted and new paths both participate."""
    merge = git(root, "merge-base", base, head).decode().strip()
    batches = [git(root, "diff", "--no-renames", "--name-only", "-z", merge, head)]
    if local:
        batches += [git(root, "diff", "--no-renames", "--name-only", "-z", "--cached"),
                    git(root, "diff", "--no-renames", "--name-only", "-z"),
                    git(root, "ls-files", "--others", "--exclude-standard", "-z")]
    return sorted({os.fsdecode(path) for batch in batches for path in batch.split(b"\0") if path})


def backend_input(path):
    if path == "python/oscope-ext/README.md":
        return True  # pyproject.toml requires this file when the SDK is packaged.
    if path.endswith(".md") or path.startswith("containers/frontend."):
        return False
    return any(path.startswith(p) if p.endswith("/") else path == p for p in BACKEND_INPUTS)


def backend_key(root):
    """Hash actual tracked build inputs, including local edits; omit web/docs."""
    digest = hashlib.sha256()
    tracked = git(root, "ls-files", "-z")
    untracked = git(root, "ls-files", "--others", "--exclude-standard", "-z")
    for raw in sorted(set((tracked + untracked).split(b"\0"))):
        if not raw:
            continue
        name = os.fsdecode(raw)
        path = root / name
        if backend_input(name) and path.is_file():
            digest.update(raw + b"\0" + str(path.stat().st_mode & 0o111).encode() + b"\0" + path.read_bytes() + b"\0")
    return digest.hexdigest()


def backend_revision(root):
    if any(backend_input(p) for p in changed_files(root, "HEAD")):
        return "unknown"
    # Include deleted inputs; a list of only today's tracked paths misses a
    # commit whose sole backend change removed a source file.
    for revision in git(root, "log", "--format=%H", "--", *BACKEND_INPUTS).decode().splitlines():
        names = git(root, "show", "--format=", "--name-only", "--no-renames",
                    "--diff-merges=first-parent", "-z", revision)
        if any(backend_input(os.fsdecode(p)) for p in names.split(b"\0") if p):
            return revision
    return "unknown"


def packages(root):
    found = []
    for path in [*root.glob("crates/*/Cargo.toml"), *root.glob("examples/*/Cargo.toml"),
                 root / "xtask/Cargo.toml"]:
        if path.is_file():
            document = tomllib.loads(path.read_text(encoding="utf-8"))
            package = document.get("package", {})
            if "name" in package:
                deps = set()
                for table in ("dependencies", "dev-dependencies", "build-dependencies"):
                    deps.update(document.get(table, {}))
                found.append((package["name"], path.parent.relative_to(root).as_posix(), deps))
    return found


def browser_specs(root, changed, broad=False):
    import re
    specs = sorted((root / "web/tests/e2e").glob("*.spec.ts"))
    if broad:
        return [p.relative_to(root).as_posix() for p in specs]
    chosen = []
    for spec in specs:
        relative = spec.relative_to(root).as_posix()
        text = spec.read_text(encoding="utf-8")
        match = re.search(r"^// Covers: (.+)$", text, re.M)
        covers = match[1].split(",") if match else ["web/src"]
        if relative in changed or any(
                name.startswith(folder.strip().rstrip("/") + "/")
                for name in changed for folder in covers):
            chosen.append(relative)
    return chosen or [p.relative_to(root).as_posix() for p in specs]


def route(root, changed):
    result = {name: False for name in NAMES}
    why = {name: [] for name in NAMES}
    def select(names, path):
        for name in names:
            result[name] = True
            why[name].append(path)
    for path in changed:
        if path.endswith(".md") and not backend_input(path):
            select(("docs",), path)
        elif path in ("scripts/gates.py", "scripts/check-gates.sh", ".github/workflows/gates.yml") or path.startswith("tests/gates/"):
            select(NAMES, path)
        elif path.startswith(("scripts/hardware-", "tests/hardware/")) or path in ("scripts/check-hardware.sh", "configs/hardware.example.toml"):
            select(("hardware",), path)
        elif path.startswith(("scripts/provision-", "tests/provisioning/")) or path in ("scripts/check-provisioning.sh", "configs/host-baseline.example.json"):
            select(("provisioning",), path)
        elif path.startswith("web/"):
            if path.startswith("web/tests/e2e/") or path == "web/playwright.config.ts":
                select(("integration",), path)
            elif path == "web/src/api/generated/openapi.ts":
                select(("frontend", "integration"), path)
            else:
                select(("frontend",), path)
                if not path.endswith(".test.ts") and not path.startswith("web/tests/"):
                    select(("integration",), path)
        elif path.startswith("containers/frontend.") or path in ("compose.frontend.yaml", "compose.web.yaml", "scripts/frontend.sh", "scripts/check-frontend.sh"):
            select(("frontend", "integration"), path)
        elif path.startswith("scripts/integration") or path == ".github/workflows/integration.yml":
            select(("integration",), path)
        elif backend_input(path) or path.startswith("scripts/check-backend") or path in ("scripts/check-cpu.sh", "scripts/check-cuda-features.py", "deny.toml"):
            select(("backend",), path)
            if backend_input(path):
                select(("integration", "runtime"), path)
                if path.startswith(("crates/oscope-ext", "python/", "examples/ext-")) or path.startswith("Cargo."):
                    select(("extensions",), path)
                if path.startswith(("crates/", "Cargo.", "examples/ex-", "python/")):
                    select(("workflows",), path)
            if path.startswith(("containers/", ".cargo/")) or path in ("rust-toolchain.toml", ".dockerignore"):
                select(("environment",), path)
        elif path.startswith("python/") or path == "scripts/check-extensions.sh":
            select(("backend", "extensions"), path)
        elif path in ("compose.yaml", "compose.gpu.yaml", "scripts/dev.sh") or path.startswith("scripts/setup-"):
            select(("environment", "backend"), path)
        elif path.startswith(("compose.runtime", "scripts/runtime", "scripts/check-runtime")):
            select(("runtime", "integration"), path)
        elif path.startswith("scripts/check-workflow"):
            select(("workflows",), path)
        elif path == "scripts/check-docs.py" or path == ".github/workflows/docs.yml":
            select(("docs",), path)
        elif path.startswith(".github/workflows/"):
            select(NAMES, path)
        elif path.startswith(("scripts/check-gpu", "scripts/check-pipeline-gpu", "scripts/check-recording")):
            select(("backend", "workflows"), path)
        else:
            select(NAMES, path)
    if result["backend"]:
        select(("cpu",), "backend CPU compatibility")
    members = packages(root)
    selected = {name for name, folder, _ in members
                if any(p.startswith(folder + "/") for p in changed)}
    broad_backend = any(not p.startswith(("crates/", "examples/", "xtask/"))
                        for p in why["backend"])
    while True:
        expanded = selected | {name for name, _, deps in members if deps & selected}
        if expanded == selected:
            break
        selected = expanded
    scope = sorted(selected) if selected and not broad_backend else []
    broad_browser = result["backend"] or any(
        not p.startswith("web/src/features/") or p.endswith("/locales/en.ts")
        for p in why["integration"])
    return {"gates": result, "packages": scope,
            "specs": browser_specs(root, changed, broad_browser) if result["integration"] else [],
            "changed": changed, "why": why}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default="HEAD")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--committed-only", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--backend-key", action="store_true")
    parser.add_argument("--backend-revision", action="store_true")
    parser.add_argument("--all", action="store_true", help="Explicit complete checkpoint")
    parser.add_argument("--run", choices=NAMES)
    args = parser.parse_args()
    if args.backend_key:
        print(backend_key(ROOT))
        return
    if args.backend_revision:
        print(backend_revision(ROOT))
        return
    plan = route(ROOT, changed_files(ROOT, args.base, args.head, not args.committed_only))
    if args.all:
        plan = route(ROOT, ["scripts/gates.py"])
        plan["full"] = True
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(plan, indent=2) + "\n", encoding="utf-8")
    if not args.run:
        print(json.dumps(plan, indent=2))
        return
    if not plan["gates"][args.run]:
        print(args.run + ": not applicable")
        return
    commands = {
        "docs": ["python3", "scripts/check-docs.py"],
        "environment": ["bash", "scripts/dev.sh", "tools"],
        "backend": ["bash", "scripts/dev.sh", "backend", *plan["packages"]],
        "cpu": ["bash", "scripts/dev.sh", "cpu-tests", *plan["packages"]],
        "frontend": ["bash", "scripts/frontend.sh", "check"],
        "integration": ["bash", "scripts/integration.sh", *[p.removeprefix("web/") for p in plan["specs"]]],
        "extensions": ["bash", "scripts/dev.sh", "extensions"],
        "workflows": ["bash", "scripts/dev.sh", "workflows"],
        "runtime": ["python3", "scripts/check-runtime.py", "--image", os.environ.get("OSCOPE_BACKEND_IMAGE", "oscope-backend:r07"),
                    "--report", "artifacts/runtime.json"],
        "hardware": [sys.executable, "-m", "unittest", "discover", "-s", "tests/hardware", "-v"],
        "provisioning": [sys.executable, "-m", "unittest", "discover", "-s", "tests/provisioning", "-v"],
    }
    subprocess.run(commands[args.run], cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
