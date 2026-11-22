# R03 Compute and algorithms

- Status: Complete locally; owner review pending
- Date: 2026-11-22
- Source: `44bc08a53f98dc29db1fa6b488a4f6269732a71a`
- Owner approval: Pending
- Scope: compute, imaging, stitching and volume algorithms; no devices or frontend

## Port specification

Port `oscope-compute`, `oscope-imaging`, `oscope-stitch` and `oscope-volume`,
including kernels, benchmarks and regression tests. Preserve algorithms and
numerical tolerances. Port the standalone `ex-compute-info` and `ex-cuda-check`
examples. Examples that require simulator/API/store dependencies remain with
those phases rather than pulling the unported application into R03.

Normal compute/imaging/volume/example builds enable CUDA. Workspace dependency
declarations disable implicit dependency defaults so an explicit workspace
`--no-default-features` build genuinely tests CPU-only compilation. Preserve
runtime CPU fallback when a CUDA-enabled build cannot initialize the GPU.

Remove the Windows NVCC/MSVC build hook and preserve the Linux fatbin build,
including disabled fused multiply-adds. Reuse source dependency versions and
existing test-build optimization settings. Resource sharing and algorithm tuning
remain outside the parity port.

Provide separate backend, GPU-equivalence, runtime-fallback and CPU-only checks.
Required GPU checks set `OSCOPE_CUDA=1`, require the actual backend and fail on a
missing device. A CUDA smoke example's CPU fallback is not GPU evidence.

## Manual test

### Prepare the environment

From PowerShell, open the existing Linux checkout:

```powershell
wsl -d Ubuntu-24.04 --cd /home/lucas/src/OScope-container
```

In WSL, run `sudo bash scripts/dev.sh build` if the development image is absent.
Check that `git branch --show-current` reports `codex/linux-container-rebuild`.
The GPU steps require a working NVIDIA Windows driver and WSL/container GPU access.

### Inspect actual CUDA processing

Enter the GPU-enabled container from WSL:

```sh
sudo bash scripts/dev.sh shell
```

Inside the container:

```sh
cargo run -p ex-compute-info --locked -- --smoke
cargo run -p ex-cuda-check --locked -- --require-cuda --smoke
```

Expect CUDA to be listed first as available, the real GPU name, and `smoke: ok`.
The CUDA example compares conversion, binning and downsampling on a full 20 MP
frame; all comparisons must say `exact`, and the frame pool's memory must be
pinned. Transfer and kernel times vary; do not substitute a timing or a CPU
fallback message for a successful GPU comparison.

### Inspect runtime fallback and required-device failure

Still inside the same CUDA-enabled container:

```sh
cargo run -p ex-cuda-check --locked -- --device 99 --smoke
echo $?
cargo run -p ex-cuda-check --locked -- --device 99 --require-cuda --smoke
echo $?
```

On this single-GPU development host, device 99 is unavailable. The first command
must report CPU fallback, print `smoke: ok`, and return `0`. The second must print
`CUDA required but unavailable` and return nonzero. Both commands retain CUDA in
the build. The test chooses an unavailable device; it does not change the host
driver or device attachment.

### Run the default and GPU algorithm checks

Exit the container to return to WSL, then run:

```sh
exit
sudo bash scripts/dev.sh backend
echo $?
sudo bash scripts/dev.sh gpu-tests
echo $?
```

Both exit codes must be `0`. The backend gate runs with CUDA compiled in but
without GPU injection, exercising runtime fallback and CPU references. Its
optional GPU test bodies do not provide GPU evidence. `gpu-tests` requires the
GPU, sets `OSCOPE_CUDA=1`, and runs the actual compute, colour, registration and
volume CUDA suites. Missing hardware or a failed numerical comparison fails the
GPU gate. Numerical tolerances are preserved from the source tests.
The release-only registration timing test is excluded explicitly; this gate
provides numerical-equivalence evidence, not production performance qualification.

### Run an explicit CPU-only build

From WSL:

```sh
sudo bash scripts/dev.sh cpu-tests
echo $?
```

Expect resolved CUDA features to be disabled for all five feature-bearing members,
CPU-only Clippy/tests to pass, the example to report CUDA absent from the build,
and exit code `0`. This checks compilation without CUDA, separately from runtime
fallback in the default build.

Record the commit, GPU/driver, backend selection, comparison results and all gate
exit codes. No microscope hardware is used. Containers are removed automatically;
retain the cache volumes. Device services and acquisition are later phases.

## Implementation

Ported the four algorithm crates and their existing kernels, benchmarks and tests.
All algorithm, kernel and existing test contents match the pinned source exactly.
Crate changes are confined to CUDA defaults/feature forwarding and removal of the
Windows compiler hook. The source's optimized development/test-build settings are
retained for compute/imaging/volume and third-party dependencies.

The two standalone examples provide the runnable checkpoint. `ex-cuda-check` adds
`--require-cuda` and `--device N`, with executable-level tests distinguishing
successful fallback from failed required-device execution. Examples depending on
unported simulator/API/storage services remain deferred.

The documentation rule now requires every phase note to contain a manual test
procedure. R00 and R01 were backfilled, R02 approval was recorded, and a phase-note
template was added. The document checker rejects a phase missing its procedure.

## Validation evidence

| Check | Result |
|---|---|
| Default workspace compilation | Passed with CUDA enabled by default |
| `scripts/dev.sh backend` | Passed formatting, Clippy, full workspace tests/doc tests, docs, layering, example smoke and dependency policy without GPU injection |
| `scripts/dev.sh cpu-tests` | Passed feature resolution, CPU-only Clippy/tests and the CPU-only example |
| Resolved CUDA features | Enabled for the 5 expected members by default; absent in the explicit CPU-only build |
| `scripts/dev.sh gpu-tests` | 13 real numerical GPU tests passed; no skipped-body messages; the release-only registration timing test is explicitly filtered out |
| Required GPU withheld | The GPU gate exited unsuccessfully with `CUDA required but unavailable` |
| Invalid-device CLI behavior | Tests passed for successful CPU fallback and failed required CUDA on device 99 |
| Lockfile | 147 retained packages; every version, source and checksum matches the pinned source lock |
| Source parity | Algorithm implementations, kernels and original tests unchanged |
| Manual-procedure rule | A temporary phase without `## Manual test` was rejected, then removed |

The GPU run used the RTX 3070 Ti Laptop GPU through the existing WSL/container
environment. Demosaic medians in this run were 4.56 ms (superpixel), 5.79 ms
(bilinear) and 6.09 ms (Malvar-He-Cutler). Deconvolution differed from the CPU by
at most 1.56e-6 relative to the largest voxel in the tested cases, within the
source tolerance. These observations are not production host qualification.

Persistent GPU evidence is in `artifacts/r03/gpu.log` and
`artifacts/r03/missing-gpu.log` in the WSL checkout. Default/CPU gate completion
was recorded from their successful process exit results; temporary `/tmp` logs
were cleared during WSL lifecycle changes. Required-device results remain
separate from optional GPU bodies in the no-device backend gate.

Hosted CI was previously blocked before execution by GitHub account billing.
Local validation is complete; hosted execution and owner acceptance are recorded
separately. No device, camera, stage or frontend implementation is included.

## Next phase

After owner acceptance, R04 ports the HAL, simulator and the dependency-closed
acquisition/frame-bus checkpoint. Preserve CUDA defaults and provide the phase's
manual procedure before requesting acceptance. Hardware qualification remains R09.
