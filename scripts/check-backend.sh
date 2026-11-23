#!/usr/bin/env bash
# Run inside the Linux development image. Fail on the first unsuccessful check.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != x86_64 ]; then
  echo 'The backend gate targets Linux x86_64. Run it through scripts/dev.sh backend.' >&2
  exit 2
fi
for script in scripts/check-*.sh; do bash -n "$script"; done
cargo fmt --all --check
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
python3 scripts/check-cuda-features.py default
cargo clippy "${scope[@]}" --all-targets --locked -- -D warnings
# The full-sensor pacing budget runs independently on an idle host.
cargo test "${scope[@]}" --locked -- --skip the_recorder_keeps_up_with_the_test_sensor_at_the_design_rate
RUSTDOCFLAGS='-D warnings' cargo doc "${scope[@]}" --no-deps --locked
cargo xtask check-layering
if wants ex-core-tour; then cargo run --package ex-core-tour --locked -- --smoke; fi
if wants ex-compute-info; then cargo run --package ex-compute-info --locked -- --smoke; fi
if wants ex-cuda-check; then cargo run --package ex-cuda-check --locked -- --smoke; fi
if wants ex-hal-fakes; then cargo run --package ex-hal-fakes --locked -- --smoke; fi
if wants ex-frame-bus; then cargo run --package ex-frame-bus --locked -- --smoke; fi
if wants ex-sim-camera; then cargo run --package ex-sim-camera --locked -- --smoke; fi
if wants ex-acquisition; then cargo run --package ex-acquisition --locked -- --smoke; fi
if wants ex-record; then cargo run --package ex-record --locked -- --smoke; fi
if wants ex-pipeline; then cargo run --package ex-pipeline --locked -- --smoke; fi
cargo deny --locked check
