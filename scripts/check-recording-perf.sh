#!/usr/bin/env bash
# Synthetic full-sensor recording budget, independent of routine correctness.
# Run on an idle host: the unchanged test writes about 1.2 GB in two seconds.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
cargo test -p oscope-acquire --test recorder --locked the_recorder_keeps_up_with_the_test_sensor_at_the_design_rate -- --exact --test-threads=1 --nocapture
