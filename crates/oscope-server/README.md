# oscope-server

Linux container entry point for OScope's HTTP/WebSocket API. R06 includes
accounts, control leases, basic simulated devices, jobs, projects, uploads,
stored media, logs and settings. R07 adds capture, preview, calibration and scan
workflows. The Vue frontend is ported separately in R08.

Use the [R06 manual procedure](../../docs/phases/r06-api-runtime.md#manual-test)
for building, creating an account, launching, testing and stopping the container.
The [runtime configuration](../../configs/runtime.toml) binds inside the container
at port 8210; Compose publishes it only on the Windows/WSL host's loopback.
The non-loopback listener requires an account and TLS. The certificate, accounts,
projects, SQLite job history and daily logs live in the state volume. Each start
rotates the private local health token; user sessions expire across starts.

The CLI retains configuration/port/root overrides, printed OpenAPI/TD documents,
account add/remove/passwd/list, config migrate and ext pack. The container entry
point supplies the runtime configuration unless --config is explicit. No Windows
service or --managed sidecar mode is provided. Requests for the deferred
shared-memory preview ring fail explicitly.

The public /health route reports liveness. Authenticated /ready reports
readiness, or HTTP 503 while draining. The health probe validates TLS using the
persisted certificate and reads the mode-0600 local token without printing it.
An unhealthy status is diagnostic; Docker's restart policy supervises process
exit, rather than restarting solely because a probe failed.

SIGTERM/SIGINT stop gracefully with exit 0 and close media sockets with code
1001. The guarded restart action exits 75 so Docker can restart the service.
Host reboot/shutdown actions are absent. Restart commands require the control
lease and confirmation and respect the hardware-job lock.

The runtime image includes Python 3.12, uv and the extension SDK, together with
CUDA libraries and a required-device kernel diagnostic. Rust/Node toolchains and
the Docker socket are excluded. Physical devices and production provisioning
still require R09-R12 qualification.
