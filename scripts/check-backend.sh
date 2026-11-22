#!/usr/bin/env bash
# Run inside the Linux development image. Fail on the first unsuccessful check.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != x86_64 ]; then
  echo 'The backend gate targets Linux x86_64. Run it through scripts/dev.sh backend.' >&2
  exit 2
fi
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cargo xtask check-layering
cargo run --package ex-core-tour --locked -- --smoke
cargo deny --locked check
