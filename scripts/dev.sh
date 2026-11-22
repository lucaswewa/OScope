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
  shell) "${gpu[@]}" run --rm --no-deps dev bash ;;
  shell-cpu) "${base[@]}" run --rm --no-deps dev bash ;;
  *) echo 'usage: scripts/dev.sh [build|config|tools|gpu|backend|shell|shell-cpu]' >&2; exit 2 ;;
esac
