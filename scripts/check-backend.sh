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
python3 scripts/check-cuda-features.py default
cargo clippy --workspace --all-targets --locked -- -D warnings
# The full-sensor pacing budget runs independently on an idle host.
cargo test --workspace --locked -- --skip the_recorder_keeps_up_with_the_test_sensor_at_the_design_rate
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cargo xtask check-layering
cargo run --package ex-core-tour --locked -- --smoke
cargo run --package ex-compute-info --locked -- --smoke
cargo run --package ex-cuda-check --locked -- --smoke
cargo run --package ex-hal-fakes --locked -- --smoke
cargo run --package ex-frame-bus --locked -- --smoke
cargo run --package ex-sim-camera --locked -- --smoke
cargo run --package ex-acquisition --locked -- --smoke
cargo run --package ex-record --locked -- --smoke
cargo run --package ex-pipeline --locked -- --smoke
cargo deny --locked check
