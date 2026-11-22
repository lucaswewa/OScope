# Container development environment

R01 establishes a Linux x86_64 development container before any microscope code is
ported. It will contain the pinned Rust toolchain, CUDA compilation tools, Python
and uv. Tool checks and a required-device CUDA probe validate the environment;
they do not establish camera support or production readiness.

Environment commands and verification evidence are added with the R01 tooling.
See [the phase plan](migration-plan.md) for the source port sequence and
[the factory workflow](factory-flow.md) for production installation.
