#!/usr/bin/env bash
# Explicit CPU-only compilation, independent of runtime fallback.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
scope=(--workspace)
selected=("$@")
if [ ${#selected[@]} -gt 0 ]; then
  scope=()
  for package in "${selected[@]}"; do scope+=(--package "$package"); done
fi
wants() {
  [ ${#selected[@]} -eq 0 ] && return 0
  for package in "${selected[@]}"; do [ "$package" != "$1" ] || return 0; done
  return 1
}
python3 scripts/check-cuda-features.py cpu
cargo clippy "${scope[@]}" --all-targets --no-default-features --locked -- -D warnings
cargo test "${scope[@]}" --no-default-features --locked -- --skip the_recorder_keeps_up_with_the_test_sensor_at_the_design_rate
if wants ex-cuda-check; then cargo run -p ex-cuda-check --no-default-features --locked -- --smoke; fi
if wants ex-acquisition; then cargo run -p ex-acquisition --no-default-features --locked -- --smoke; fi
if wants ex-pipeline; then cargo run -p ex-pipeline --no-default-features --locked -- --smoke; fi
