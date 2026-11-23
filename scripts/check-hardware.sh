#!/usr/bin/env bash
# Tool/fixture tests only. No attached device, Rust, Node or vendor SDK required.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -m unittest discover -s tests/hardware -v
