# Engineering release bundles and state-preserving updates

R12 provides backend-only signed engineering bundles and an isolated Linux
installer/update adapter. It does not publish product releases or qualify a
microscope. Read [the phase evidence/manual test](phases/r12-release-lifecycle.md)
and [ADR 0063](adr/0063-release-trust-and-state.md). Rust, Vue, application
dependencies and the existing review deployment are unchanged.

## Boundaries

The current bundle contains one Linux amd64 non-root backend image, LICENSE,
manifest.json and manifest.sig. The manifest records release ID, monotonic
sequence, key ID, supported baseline IDs, data/config schemas, image identity,
source revision and each artifact's size/hash. It records frontend as null.
Production scope is rejected. Complete frontend/host assets, acceptance profiles,
third-party notices/SBOM, promotion evidence and physical upgrade qualification
must be added and reviewed before a full product release. The legacy license
allowlist remains in effect; an engineering bundle is not a license-audited
shipping package.

Docker can identify an image by its configuration digest or its OCI target digest,
depending on the image store. The signed manifest binds the selected image ID and
configuration digest separately. The verifier checks the archive's OCI descriptor
chain/configuration, source revision and Linux amd64 non-root identity without
extracting or executing it. After signature, size/hash and archive verification,
the installer imports its private verified copy and inspects the exact image ID.
It never pulls or builds an image on the target.

Trust is supplied separately from the bundle. OpenSSL 3 verifies an Ed25519
signature over the exact manifest bytes; public key fingerprints are SHA-256 of
DER SubjectPublicKeyInfo. Unknown/revoked keys, altered bytes, extra/missing files,
duplicate JSON fields, unsafe paths and sequence floors fail closed. A bundle
cannot supply its own trusted key. These controls authenticate artifacts against
the supplied policy; they do not prove physical qualification or publisher key
custody. Production trust distribution, rotation/revocation updates, protected
signing and release promotion remain pending. Lab-generated keys must never be
installed as product trust.

## Developer check and isolated lab

Prerequisites: Python 3.12+, OpenSSL 3 on PATH or OSCOPE_OPENSSL. Windows fixtures
also locate the installed Git for Windows OpenSSL. A missing signing tool fails
the gate; tests never skip signature verification.

```bash
bash scripts/check-release.sh
bash scripts/check-gates.sh
```

The required-CUDA lab additionally needs Linux x86_64, root Docker access, NVIDIA
Container Toolkit and a local oscope-backend:r07 image. Allow space for two full
compressed image bundles, two private cached copies and small metadata backups;
check free space on the drive holding the WSL VHD as well as inside Linux. The
guest's sparse filesystem capacity is not physical host free space.

```bash
sudo python3 tests/release/lab.py --image oscope-backend:r07 --output artifacts/r12
```

The printed UUID directory retains lab-report.json, inventory.json, two signed
bundles, disposable lab keys and the private installation state. The second image
differs only by a metadata label: the lab validates mechanics between distinct
immutable images with identical application behavior. It does not establish
compatibility across a new application/data schema. It removes only its labelled
container and retains state/images for review; no image/volume pruning occurs.
Missing/skipped CUDA, failed TLS or lost data fails the run. CPU fallback remains
an application capability, but cannot pass required-GPU release qualification.

The lab performs an actual hard process exit immediately after a real clean
backend shutdown. It then demonstrates durable maintenance, explicit recovery and
compatible rollback. This is interruption evidence, not a physical power-loss or
before-login Windows lifecycle test.

## Packaging and verification on an engineering build station

Use new paths for each attempt. These commands generate **disposable lab keys**
and a signed backend bundle; they do not tag, push or publish a product release:

```bash
python3 scripts/release.py lab-keygen --output artifacts/r12/manual-keys --engineering
sudo python3 scripts/release.py pack --image oscope-backend:r07 \
  --release-id manual-eng-1 --sequence 1 --baseline manual-lab \
  --key-id lab --key artifacts/r12/manual-keys/lab-private.pem \
  --trust artifacts/r12/manual-keys/trust.json \
  --output artifacts/r12/manual-bundle --engineering
python3 scripts/release.py verify --bundle artifacts/r12/manual-bundle \
  --trust artifacts/r12/manual-keys/trust.json
```

Expected: signature verified with product_qualified false and the exact image
ID. Keep the externally approved trust policy separate when copying a bundle
offline. Do not copy a private key to an instrument or inside a bundle. Changing
any manifest, signature or payload byte must make verification fail. A higher
release sequence is a controlled release decision, not a way to bypass a failed
test. Pack rejects existing output directories so failed artifacts are preserved.

## Isolated install, update and rollback

The automatic lab is the primary complete manual exercise and handles cleanup.
For an individual CLI exercise, use the two verified lab bundles and a new root
on the Linux filesystem. Replace RUN-ID with the lab's printed UUID. Verify the
chosen loopback port is free; never point these tools at an R10 or customer root.

```bash
r12_lab="$PWD/artifacts/r12/RUN-ID"
r12_root="$PWD/artifacts/r12/manual-install"
sudo -v
python3 -c 'import getpass; print(getpass.getpass("New lab operator password: "))' | \
  sudo python3 scripts/release.py install --root "$r12_root" \
  --bundle "$r12_lab/bundle-1" --trust "$r12_lab/keys/trust.json" \
  --instrument MANUAL-001 --pc MANUAL-PC --baseline r12-lab-only \
  --port 18212 --engineering
sudo python3 scripts/release.py apply --root "$r12_root" \
  --bundle "$r12_lab/bundle-2" --engineering
sudo python3 scripts/release.py rollback --root "$r12_root" --engineering
sudo python3 scripts/release.py inventory --root "$r12_root" \
  --output artifacts/r12/manual-inventory.json
```

Use a single-line 16–256 character bootstrap password; it goes through stdin,
never command arguments or reports. Only the initial account is created. Updates
reuse the installed account/TLS identity and do not rotate credentials. The
installer captures observed Linux versions as engineering pins and checks them
on subsequent changes; this is not approval of a production host baseline.

Expected: each completed operation returns outcome passed, pending production
qualification and shipment_authorized false. Normal updates must exceed the
installation's sequence high-water mark. After rollback to sequence 1, an ordinary
apply of sequence 2 is refused because the high-water mark remains 2. Explicit
rollback can select only the previous verified compatible release. It must still
satisfy key revocation/floor and baseline/schema checks.

The updater takes the installation lock and control lease, refuses streaming,
non-idle stage or queued work, marks maintenance, stops only the owned backend,
and makes a consistent private metadata/database snapshot. It supports only the
known data schema 1/config schema 2 and never runs migration scripts. A successful
switch executes required CUDA kernels, verifies TLS/API revision and unchanged
credentials/config/calibration, and checks no acquisition/motion resumed. The
lock/lease covers normal operators; privileged local administrators can bypass
the product and remain part of the operational trust boundary.

## Failed or interrupted change

Stop and diagnose the recorded operation. Failed validation leaves the runtime
stopped with pending.json and maintenance.json retained. No automatic resume or
snapshot restore occurs. After an interruption, use the explicit sequence:

```bash
sudo python3 scripts/release.py recover --root "$r12_root" --engineering
sudo python3 scripts/release.py rollback --root "$r12_root" --engineering
```

Recover first verifies ownership and stops the known runtime. Rollback verifies
the previous cached bundle and schema compatibility, then starts that image over
the **current** data. It preserves acquisitions created after an update. It never
restores a stale database/config snapshot over newer data. A failed initial
installation has no previous good release: retain state for diagnosis instead of
inventing a rollback target or resetting its account. A corrupt history or lost
metadata requires supervised repair; the tool refuses to guess.

## Evidence, inventory and cleanup

Each operation has a fresh UUID, receipt, exact image/manifest/source hashes,
times, outcome, identity and compatibility information. history.jsonl chains
events and receipt hashes; inventory verifies them and the immutable installation
identity. Inventory is the last recorded state, with runtime_observation
not_probed, not a live health or physical acceptance check. Export a new file for
each snapshot. Repeat --root to collect explicitly selected local installations;
duplicate installation IDs are rejected. Central fleet synchronization and signed
inventory retention remain pending.

Export only lab-report.json, inventory.json and operations/*/receipt.json for
review. Keep installation/data, operations/*/private-metadata.tar.gz, signing keys
and any logs private. Snapshots include credentials and are retained for
diagnosis; the current tools do not implement a restoration workflow. Retention
policy must account for this storage before production use.

The automatic lab removes its own container. For a manually created engineering
installation, derive the name as oscope-r12- followed by installation.json's UUID
without hyphens. Compare Docker inspect's oscope.installation label to that UUID
and its data/scratch mount paths to the selected root before stopping/removing
only that container. Keep its private state; never delete volumes or use docker prune. Do not
modify Windows startup tasks or install this adapter beside an R10 fixed-image
supervisor. Qualification of that lifecycle transition, a complete authenticated
product bundle and real physical upgrade/rollback remains open.

## Primary references

- [OpenSSL Ed25519 signing](https://docs.openssl.org/3.0/man1/openssl-pkeyutl/)
- [Docker containerd image store](https://docs.docker.com/engine/storage/containerd/)
- [Docker save](https://docs.docker.com/reference/cli/docker/image/save/)
- [Docker load](https://docs.docker.com/reference/cli/docker/image/load/)
