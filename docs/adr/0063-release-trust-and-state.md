# ADR 0063 — authenticated releases and retained instrument state

- Status: Accepted for engineering implementation; production qualification pending
- Date: 2026-11-23
- Phase: R12

## Decision

Use a detached Ed25519 signature over the exact small UTF-8 manifest bytes.
Verify with OpenSSL 3 against an externally supplied, fingerprint-pinned public
key policy. Never trust a key shipped inside the bundle. The signature binds
artifact hashes/sizes, immutable Docker image/source identity, release sequence,
baseline compatibility and data/configuration schemas. Reject unknown/revoked
keys, duplicate fields, unsafe paths, missing/extra payloads and altered bytes
before importing or running an image. Production key custody and trust-policy
distribution need a separate qualified operational procedure.

Keep instrument identity, active release and append-only operation history
separate. Normal updates increase the per-installation sequence high-water mark;
rollback is an explicit operation to the previous verified compatible selection
and does not lower that mark. Only equal known read/write schemas are supported
initially. No arbitrary migration hooks or payload executables are run.

Serialize changes, acquire control and require no queued/running jobs, camera
streaming or stage movement. Mark maintenance durably before stopping the owned
backend; snapshot metadata/databases only after clean shutdown. Backups remain
private and are never automatically restored. An incomplete/failed operation
leaves maintenance in place and never resumes acquisition. Recovery first stops
the owned runtime; an explicit compatible rollback preserves all current data.

The first adapter targets new, isolated simulated Linux installations and signed
backend-only engineering bundles. Refuse R10 state roots; its fixed-image Windows
supervisor must not restart an old release over new state. Full-product assets,
Windows host integration, physical devices and customer deployment are pending.
No current report is product promotion, physical acceptance or shipment approval.

## Consequences

Offline artifact integrity and publisher authentication are testable independently
of application gates. Monotonic sequence and explicit rollback limit accidental
replay. State-preserving rollback avoids erasing acquisitions made after update.
The local history detects accidental corruption; it is not a signed central
fleet inventory. Operations still needs protected trust distribution, key rotation/
revocation, complete third-party notices, qualified frontend/host bundles, backup
retention and physical upgrade qualification before production use.

## References

- [OpenSSL Ed25519 signing and verification](https://docs.openssl.org/3.0/man1/openssl-pkeyutl/)
- [Docker offline image save](https://docs.docker.com/reference/cli/docker/image/save/)
- [Docker offline image load](https://docs.docker.com/reference/cli/docker/image/load/)
- [Container architecture decision](0062-linux-container-backend.md)
