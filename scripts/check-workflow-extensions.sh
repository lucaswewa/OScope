#!/usr/bin/env bash
# Real extension workflow planning and report generation through the API.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
command -v uv >/dev/null
export OSCOPE_PYTHON=1
cargo test -p oscope-api --test scan --locked a_python_workflow_plans_a_spiral_that_the_engine_runs -- --exact --test-threads=1 --nocapture
cargo test -p oscope-api --test reports --locked -- --test-threads=1 --nocapture
