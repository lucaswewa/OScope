#!/usr/bin/env bash
# Run from a Linux checkout in WSL or native Linux. Shell defaults to GPU access.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if ! command -v docker >/dev/null; then
  echo 'Docker is missing. See docs/development.md for WSL setup.' >&2
  exit 2
fi
base=(docker compose -f compose.yaml)
gpu=("${base[@]}" -f compose.gpu.yaml)
case "${1:-shell}" in
  build) "${base[@]}" build dev ;;
  config) "${base[@]}" config --quiet; "${gpu[@]}" config --quiet ;;
  tools) "${base[@]}" run --rm --no-deps -T dev oscope-check tools ;;
  gpu) "${gpu[@]}" run --rm --no-deps -T dev oscope-check gpu ;;
  backend) "${base[@]}" run --rm --no-deps -T dev bash scripts/check-backend.sh ;;
  gpu-tests) "${gpu[@]}" run --rm --no-deps -T dev bash scripts/check-gpu.sh ;;
  pipeline-gpu) "${gpu[@]}" run --rm --no-deps -T dev bash scripts/check-pipeline-gpu.sh ;;
  workflows) "${base[@]}" run --rm --no-deps -T dev bash scripts/check-workflows.sh default ;;
  workflows-cpu) "${base[@]}" run --rm --no-deps -T dev bash scripts/check-workflows.sh cpu ;;
  workflows-gpu) "${gpu[@]}" run --rm --no-deps -T dev bash scripts/check-workflows.sh gpu ;;
  workflow-extensions) "${base[@]}" run --rm --no-deps -T dev bash scripts/check-workflow-extensions.sh ;;
  extensions) "${base[@]}" run --rm --no-deps -T dev bash scripts/check-extensions.sh ;;
  recording-perf) "${base[@]}" run --rm --no-deps -T dev bash scripts/check-recording-perf.sh ;;
  cpu-tests) "${base[@]}" run --rm --no-deps -T dev bash scripts/check-cpu.sh ;;
  shell) "${gpu[@]}" run --rm --no-deps dev bash ;;
  shell-cpu) "${base[@]}" run --rm --no-deps dev bash ;;
  *) echo 'usage: scripts/dev.sh [build|config|tools|gpu|backend|gpu-tests|pipeline-gpu|workflows|workflows-cpu|workflows-gpu|workflow-extensions|extensions|recording-perf|cpu-tests|shell|shell-cpu]' >&2; exit 2 ;;
esac
