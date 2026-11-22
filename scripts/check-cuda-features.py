"""Verify resolved CUDA features for normal and explicit CPU-only builds."""
import json
import subprocess
import sys

mode = sys.argv[1]
if mode not in ("default", "cpu"):
    raise SystemExit("usage: check-cuda-features.py default|cpu")
args = ["cargo", "metadata", "--format-version", "1", "--locked"]
if mode == "cpu":
    args.append("--no-default-features")
metadata = json.loads(subprocess.check_output(args, text=True))
names = {p["id"]: p["name"] for p in metadata["packages"]}
required = {"oscope-compute", "oscope-imaging", "oscope-volume", "ex-compute-info", "ex-cuda-check"}
checked = set()
for node in metadata["resolve"]["nodes"]:
    name = names[node["id"]]
    if name in required:
        enabled = "cuda" in node["features"]
        if enabled != (mode == "default"):
            raise SystemExit(f"{name}: unexpected CUDA features in {mode} mode: {node['features']}")
        checked.add(name)
if checked != required:
    raise SystemExit(f"Missing feature-bearing members: {required - checked}")
print(f"CUDA feature resolution passed: {mode}, {len(checked)} members")
