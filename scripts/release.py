#!/usr/bin/env python3
"""R12 engineering release CLI. Customer deployment and product promotion are unsupported."""
import argparse
import json
import os
from pathlib import Path
import shutil
import sys
import uuid

import release_bundle as b
import release_state as s


def initialize(root, trust, instrument, pc, baseline, port):
    b.check(root.is_absolute() and root.resolve() == root and len(root.parts) >= 4 and not root.exists(), "use a new dedicated absolute root")
    for value in (instrument, pc, baseline):
        b.identifier(value)
    trusted = b.policy(trust)
    root.mkdir(parents=True, exist_ok=False, mode=0o700)
    (root / "trust").mkdir(mode=0o700)
    shutil.copyfile(trust, root / "trust/trust.json")
    for key in trusted["keys"]:
        shutil.copyfile(trust.parent / key["public_key"], root / "trust" / key["public_key"])
    from release_runtime import Runtime
    runtime = Runtime(root)
    identity = {"schema": 1, "scope": b.SCOPE, "installation_id": str(uuid.uuid4()), "instrument_serial": instrument,
                "pc_serial": pc, "baseline_id": baseline, "port": port, "linux_versions": runtime.p.linux_versions(),
                "trust_sha256": b.sha(root / "trust/trust.json")}
    b.check(type(port) is int and 1024 <= port <= 65535, "invalid port")
    b.atomic(root / "installation.json", identity)
    s.record(root, {"type": "initialized", "time": s.now(), "identity_sha256": b.sha(root / "installation.json")})
    return identity


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    sub = commands.add_parser("lab-keygen")
    sub.add_argument("--output", type=Path, required=True)
    sub.add_argument("--engineering", action="store_true", required=True)
    sub = commands.add_parser("pack")
    for field in ("image", "release-id", "baseline", "key-id"):
        sub.add_argument("--" + field, required=True)
    sub.add_argument("--sequence", type=int, required=True)
    for field in ("output", "key", "trust"):
        sub.add_argument("--" + field, type=Path, required=True)
    sub.add_argument("--engineering", action="store_true", required=True)
    sub = commands.add_parser("verify")
    sub.add_argument("--bundle", type=Path, required=True)
    sub.add_argument("--trust", type=Path, required=True)
    for name in ("install", "apply", "rollback", "recover"):
        sub = commands.add_parser(name)
        sub.add_argument("--root", type=Path, required=True)
        sub.add_argument("--engineering", action="store_true", required=True)
        if name in ("install", "apply"):
            sub.add_argument("--bundle", type=Path, required=True)
        if name == "install":
            sub.add_argument("--trust", type=Path, required=True)
            for field in ("instrument", "pc", "baseline"):
                sub.add_argument("--" + field, required=True)
            sub.add_argument("--port", type=int, required=True)
    sub = commands.add_parser("inventory")
    sub.add_argument("--root", action="append", type=Path, required=True)
    sub.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "lab-keygen":
            print(json.dumps({"scope": b.SCOPE, "fingerprint": b.lab_keygen(args.output)}))
        elif args.command == "pack":
            manifest, sha = b.pack(args.output, args.image, args.release_id, args.sequence, args.baseline, args.key, args.key_id, args.trust)
            print(json.dumps({"scope": b.SCOPE, "release_id": manifest["release_id"], "manifest_sha256": sha}))
        elif args.command == "verify":
            manifest, sha = b.verify(args.bundle, args.trust)
            print(json.dumps({"scope": b.SCOPE, "signature": "verified", "manifest_sha256": sha,
                              "release_id": manifest["release_id"], "image_id": manifest["backend"]["image_id"], "product_qualified": False}))
        elif args.command == "inventory":
            values = [s.inventory(root) for root in args.root]
            b.check(len({v["installation_id"] for v in values}) == len(values), "duplicate installation in inventory")
            args.output.parent.mkdir(parents=True, exist_ok=True)
            with args.output.open("xb") as stream:
                stream.write(b.dumps({"schema": 1, "scope": b.SCOPE, "generated": s.now(), "installations": values}))
            print("Inventory written: " + str(args.output))
        else:
            b.check(sys.platform == "linux" and os.geteuid() == 0, "run runtime operations as Linux root")
            from release_runtime import Runtime
            if args.command == "install":
                b.verify(args.bundle, args.trust)  # Authenticate before creating instrument state.
                initialize(args.root, args.trust, args.instrument, args.pc, args.baseline, args.port)
            with s.locked(args.root):
                driver = Runtime(args.root, sys.stdin.readline().strip() if args.command == "install" else None)
                if args.command == "recover":
                    result = s.recover(args.root, driver)
                else:
                    result = s.change(args.root, getattr(args, "bundle", None), driver, args.command == "rollback")
                print(json.dumps(result))
                if result.get("outcome") in ("failed", "interrupted"):
                    return 1
        return 0
    except (OSError, ValueError, KeyError, TypeError, RuntimeError) as error:
        # No child output, token, bootstrap password or private key contents.
        print("release: " + type(error).__name__ + ": " + (str(error) if isinstance(error, ValueError) else "operation failed; inspect private installation state"), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
