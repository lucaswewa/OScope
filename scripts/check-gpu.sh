#!/usr/bin/env bash
# Required-device gate, inside the GPU-enabled development container.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
python3 scripts/check-cuda-features.py default
export OSCOPE_CUDA=1
cargo run -p ex-cuda-check --locked -- --require-cuda --smoke
cargo test -p oscope-compute --test cuda --locked -- --test-threads=1 --nocapture
cargo test -p oscope-imaging --test gpu --locked -- --test-threads=1 --nocapture
# Select numerical equivalence explicitly. The separate release-only registration
# timing test is not part of this gate and must not appear as a skipped pass.
cargo test -p oscope-imaging --test registration_gpu --locked the_gpu_agrees_with_the_cpu -- --exact --test-threads=1 --nocapture
cargo test -p oscope-volume --test cuda --locked -- --test-threads=1 --nocapture
