#!/usr/bin/env bash
# Real extension execution, separate from the routine backend gate.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
command -v uv >/dev/null
# Hosted checkouts need not be owned by the container's UID. Keep the SDK's
# test environment and pytest cache out of the source mount.
sdk_work=$(mktemp -d /tmp/oscope-sdk-tests.XXXXXX)
trap 'rm -rf -- "$sdk_work"' EXIT
UV_PROJECT_ENVIRONMENT="$sdk_work/venv" uv run --project python/oscope-ext --locked --no-managed-python --python /usr/bin/python3 pytest -q -p no:cacheprovider python/oscope-ext/tests
export OSCOPE_PYTHON=1
# Each enabled test requires an installed worker and real shared-memory calls.
cargo test -p oscope-ext --test python --locked -- --test-threads=1 --nocapture
cargo run -p ex-rust-ext --locked -- --smoke
cargo run -p ex-python-node --locked -- --smoke
