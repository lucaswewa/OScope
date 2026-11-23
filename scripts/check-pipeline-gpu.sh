#!/usr/bin/env bash
# R05's required pipeline-device checks, independently runnable after a recipe change.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
python3 scripts/check-cuda-features.py default
export OSCOPE_CUDA=1
cargo test -p oscope-pipeline --test pipeline --locked gpu_steps_match_the_cpu_s -- --exact --test-threads=1 --nocapture
cargo test -p oscope-pipeline --test stock --locked gpu_steps_match_the_cpu_s -- --exact --test-threads=1 --nocapture
cargo test -p oscope-pipeline --test filters --locked filter_nodes_on_the_gpu_match_the_cpu_s -- --exact --test-threads=1 --nocapture
cargo run -p ex-pipeline --locked -- --recipe examples/ex-pipeline/recipes/bin-and-measure.json --require-cuda --smoke
