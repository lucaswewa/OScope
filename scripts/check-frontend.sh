#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname -- "${BASH_SOURCE[0]}")/../web"
if command -v cargo >/dev/null || command -v rustc >/dev/null; then
  echo 'Use the independent frontend image; Rust is not part of this gate.' >&2
  exit 2
fi
npm ci --no-audit --no-fund
npm run typecheck
npm run lint
npm run test:unit -- --maxWorkers=2
npm run build
npm run licences
npm run advisories
