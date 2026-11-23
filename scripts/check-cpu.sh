#!/usr/bin/env bash
# Explicit CPU-only compilation, independent of runtime fallback.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
python3 scripts/check-cuda-features.py cpu
cargo clippy --workspace --all-targets --no-default-features --locked -- -D warnings
cargo test --workspace --no-default-features --locked -- --skip the_recorder_keeps_up_with_the_test_sensor_at_the_design_rate
cargo run -p ex-cuda-check --no-default-features --locked -- --smoke
cargo run -p ex-acquisition --no-default-features --locked -- --smoke
cargo run -p ex-pipeline --no-default-features --locked -- --smoke
