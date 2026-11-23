#!/usr/bin/env python3
"""Authenticated readiness probe; never print the private token or key."""
import json
import os
from pathlib import Path
import ssl
import sys
import tomllib
import urllib.request


def main():
    file = Path(os.environ.get("OSCOPE_CONFIG", "/var/lib/oscope/data/config/oscope.toml"))
    config = tomllib.loads(file.read_text())
    data = Path(config["storage"]["data_root"])
    if not data.is_absolute():
        data = file.parent / data
    certificate = config.get("security", {}).get("certificate")
    certificate = Path(certificate) if certificate else data / "tls" / "certificate.pem"
    if not certificate.is_absolute():
        certificate = file.parent / certificate
    context = ssl.create_default_context(cafile=str(certificate))
    token = (data / "run" / "local.token").read_text().strip()
    port = config.get("network", {}).get("port", 8210)
    request = urllib.request.Request(
        f"https://127.0.0.1:{port}/ready",
        headers={"Authorization": f"Bearer {token}"},
    )
    with urllib.request.urlopen(request, context=context, timeout=3) as response:
        if response.status != 200 or json.load(response) != {"status": "ready"}:
            raise RuntimeError("not ready")


if __name__ == "__main__":
    try:
        main()
    except Exception:
        print("OScope readiness probe failed", file=sys.stderr)
        sys.exit(1)
