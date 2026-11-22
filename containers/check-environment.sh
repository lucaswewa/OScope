#!/usr/bin/env bash
# Verify installed tools, or require a real CUDA kernel. No microscope is needed.
set -euo pipefail
mode=${1:-tools}
case "$mode" in
  tools | gpu) ;;
  *) echo 'usage: oscope-check [tools|gpu]' >&2; exit 2 ;;
esac
test "$(uname -s)" = Linux
test "$(uname -m)" = x86_64
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
uv --version
nvcc --version
python3 - <<'PY'
import ctypes
import subprocess
import tomllib
from pathlib import Path

pin = tomllib.loads(Path('/opt/oscope/rust-toolchain.toml').read_text())
expected = pin['toolchain']['channel']
actual = subprocess.check_output(['rustc', '--version'], text=True).split()[1]
assert actual == expected, (actual, expected)
for library in ('libnvrtc.so.13', 'libcufft.so.12'):
    ctypes.CDLL(library)
    print(f'Loaded {library}')
print('Python and pinned Rust toolchain passed')
PY

scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
printf '%s\n' 'fn main() { assert_eq!(6 * 7, 42); println!("Rust compile and execute passed"); }' \
  > "$scratch/probe.rs"
rustc "$scratch/probe.rs" -o "$scratch/probe"
"$scratch/probe"
if [ "$mode" = gpu ]; then
  # SASS avoids a newer-toolkit PTX requirement on a compatible CUDA 13 driver.
  # A missing device or an incorrect result is a failure, never a skipped check.
  nvcc -O2 --fmad=false -arch=all-major /opt/oscope/gpu-probe.cu -o "$scratch/gpu-probe"
  "$scratch/gpu-probe"
fi
