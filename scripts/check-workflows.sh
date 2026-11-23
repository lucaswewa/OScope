#!/usr/bin/env bash
# Longer simulated calibration, scan and analysis checks, independent of frontend.
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
mode=${1:-default}
flags=()
case "$mode" in
  default) ;;
  cpu) flags+=(--no-default-features) ;;
  gpu)
    export OSCOPE_CUDA=1
    cargo run -p ex-cuda-check --profile test --locked -- --require-cuda --smoke
    cargo test -p oscope-acquire --test preview --locked the_gpu_s_previews_are_the_cpu_s -- --exact --test-threads=1 --nocapture
    ;;
  *) echo 'usage: check-workflows.sh [default|cpu|gpu]' >&2; exit 2 ;;
esac
for package in ex-calibrate ex-autofocus ex-capture ex-scan ex-histo-scan ex-checkpoint-f; do
  timeout --signal=TERM --kill-after=30s 900 \
    cargo run -p "$package" --profile test --locked "${flags[@]}" -- --smoke
done
