#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/.."
compose=(docker compose -f compose.frontend.yaml)
case "${1:-check}" in
  check|install|dist)
    mkdir -p artifacts
    exec 9>artifacts/frontend.lock
    flock 9
    ;;
esac
case "${1:-check}" in
  build) "${compose[@]}" build frontend ;;
  browser-build) "${compose[@]}" build integration ;;
  check) "${compose[@]}" run --rm --no-deps -T frontend bash /workspace/scripts/check-frontend.sh ;;
  install) "${compose[@]}" run --rm --no-deps -T frontend npm ci --no-audit --no-fund ;;
  dist) "${compose[@]}" run --rm --no-deps -T frontend npm run build ;;
  dev) "${compose[@]}" run --rm --no-deps frontend npm run dev ;;
  shell) "${compose[@]}" run --rm --no-deps frontend bash ;;
  *) echo 'usage: scripts/frontend.sh [build|browser-build|check|install|dist|dev|shell]' >&2; exit 2 ;;
esac
