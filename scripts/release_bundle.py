"""Authenticated backend-only engineering bundles; no product promotion."""
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile
import uuid

ROOT = Path(__file__).resolve().parents[1]
SCOPE = "engineering_backend"
MAX_ARCHIVE = 16 * 1024**3
SHA = r"[0-9a-f]{64}"


def check(condition, message):
    if not condition:
        raise ValueError(message)


def identifier(value):
    check(isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}", value), "invalid identifier")
    return value


def sha(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def dumps(value):
    return (json.dumps(value, indent=2, allow_nan=False) + "\n").encode()


def loads(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            check(key not in result, "duplicate JSON field")
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite JSON")))


def atomic(path, value):
    temporary = path.with_name(path.name + "." + uuid.uuid4().hex + ".tmp")
    try:
        with temporary.open("xb") as stream:
            if os.name != "nt":
                os.chmod(temporary, 0o600)
            stream.write(dumps(value))
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        if os.name != "nt":
            fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(fd)
            finally:
                os.close(fd)
    finally:
        temporary.unlink(missing_ok=True)


def openssl(*args):
    binary = os.environ.get("OSCOPE_OPENSSL") or shutil.which("openssl")
    if not binary and os.name == "nt":
        binary = str(Path(os.environ.get("ProgramFiles", "C:/Program Files")) / "Git/usr/bin/openssl.exe")
    check(bool(binary), "OpenSSL 3 is required; set OSCOPE_OPENSSL")
    version = subprocess.run([binary, "version"], capture_output=True, stdin=subprocess.DEVNULL, timeout=10)
    check(version.returncode == 0 and re.match(rb"OpenSSL 3\.", version.stdout), "OpenSSL 3 is required")
    result = subprocess.run([binary, *map(str, args)], capture_output=True, stdin=subprocess.DEVNULL, timeout=30)
    check(result.returncode == 0, "OpenSSL operation failed; signature/key unavailable or invalid")
    return result.stdout


def public_der(path):
    raw = openssl("pkey", "-pubin", "-in", path, "-outform", "DER")
    check(len(raw) == 44 and raw[:12] == bytes.fromhex("302a300506032b6570032100"), "Ed25519 public key required")
    return raw


def policy(path):
    value = loads(path.read_bytes())
    check(isinstance(value, dict) and set(value) == {"schema", "scope", "minimum_sequence", "keys"}, "invalid trust policy")
    check(type(value["schema"]) is int and value["schema"] == 1 and value["scope"] == SCOPE, "unsupported trust scope")
    check(type(value["minimum_sequence"]) is int and value["minimum_sequence"] >= 1, "invalid sequence floor")
    check(isinstance(value["keys"], list) and 0 < len(value["keys"]) <= 16, "invalid key list")
    seen = set()
    for key in value["keys"]:
        check(isinstance(key, dict) and set(key) == {"id", "public_key", "sha256", "revoked"}, "invalid key policy")
        identifier(key["id"])
        check(key["id"] not in seen, "duplicate key identity")
        seen.add(key["id"])
        check(re.fullmatch(r"[A-Za-z0-9_.-]+\.pem", key["public_key"] or "") is not None, "unsafe key path")
        check(type(key["revoked"]) is bool and re.fullmatch(SHA, key["sha256"] or ""), "missing key fingerprint")
    return value


def lab_keygen(output):
    output.mkdir(parents=True, exist_ok=False, mode=0o700)
    private, public = output / "lab-private.pem", output / "lab-public.pem"
    openssl("genpkey", "-algorithm", "ED25519", "-out", private)
    if os.name != "nt":
        private.chmod(0o600)
    openssl("pkey", "-in", private, "-pubout", "-out", public)
    fingerprint = hashlib.sha256(public_der(public)).hexdigest()
    atomic(output / "trust.json", {"schema": 1, "scope": SCOPE, "minimum_sequence": 1,
           "keys": [{"id": "lab", "public_key": public.name, "sha256": fingerprint, "revoked": False}]})
    return fingerprint


def validate_manifest(value):
    fields = {"schema", "scope", "release_id", "sequence", "key_id", "baseline_ids", "schemas", "backend", "license", "frontend"}
    check(isinstance(value, dict) and set(value) == fields, "manifest fields differ")
    check(type(value["schema"]) is int and value["schema"] == 1 and value["scope"] == SCOPE, "unsupported manifest scope")
    for name in ("release_id", "key_id"):
        identifier(value[name])
    check(type(value["sequence"]) is int and 0 < value["sequence"] < 2**53, "invalid release sequence")
    check(isinstance(value["baseline_ids"], list) and 0 < len(value["baseline_ids"]) <= 32, "baseline list required")
    for baseline in value["baseline_ids"]:
        identifier(baseline)
    check(len(set(value["baseline_ids"])) == len(value["baseline_ids"]), "duplicate baseline")
    schemas = value["schemas"]
    check(isinstance(schemas, dict) and set(schemas) == {"data", "config"}, "schemas required")
    check(all(type(v) is int and 1 <= v <= 999 for v in schemas.values()), "invalid schema versions")
    check(value["frontend"] is None, "this adapter supports backend-only engineering bundles")
    for name, filename in (("backend", "backend-image.tar.gz"), ("license", "LICENSE")):
        artifact = value[name]
        expected = {"file", "sha256", "size"} | ({"image_id", "config_digest", "revision", "platform"} if name == "backend" else set())
        check(isinstance(artifact, dict) and set(artifact) == expected, "invalid artifact fields")
        check(artifact["file"] == filename and re.fullmatch(SHA, artifact["sha256"] or ""), "invalid artifact path/hash")
        check(type(artifact["size"]) is int and 0 < artifact["size"] <= MAX_ARCHIVE, "invalid artifact size")
    backend = value["backend"]
    check(re.fullmatch("sha256:" + SHA, backend["image_id"] or "") and re.fullmatch("sha256:" + SHA, backend["config_digest"] or "") and
          re.fullmatch(r"[0-9a-f]{40}", backend["revision"] or "") and backend["platform"] == "linux/amd64", "invalid image identity")
    return value


def archive_identity(path, backend):
    """Read metadata without extracting or running an archive; bind config bytes to image ID."""
    documents, seen, total, metadata_bytes, blobs = {}, set(), 0, 0, {}
    with tarfile.open(path, "r:gz") as archive:
        for index, member in enumerate(archive):
            name = PurePosixPath(member.name)
            check(index < 10000 and not name.is_absolute() and ".." not in name.parts and "\\" not in member.name,
                  "unsafe image archive path/count")
            check(name.as_posix() not in seen and (member.isfile() or member.isdir()), "duplicate/special archive entry")
            seen.add(name.as_posix())
            total += member.size
            check(total <= 32 * 1024**3, "image archive exceeds unpacked limit")
            if member.isfile() and member.name.startswith("blobs/sha256/"):
                value = hashlib.sha256()
                small = bytearray()
                with archive.extractfile(member) as stream:
                    for block in iter(lambda: stream.read(1024 * 1024), b""):
                        value.update(block)
                        if member.size <= 1024 * 1024:
                            small.extend(block)
                check(member.name == "blobs/sha256/" + value.hexdigest(), "OCI blob digest differs")
                blobs["sha256:" + value.hexdigest()] = member.size
                if member.size <= 1024 * 1024:
                    metadata_bytes += member.size
                    check(metadata_bytes <= 16 * 1024**2, "image metadata exceeds limit")
                    documents[member.name] = bytes(small)
            elif member.isfile() and (member.name == "manifest.json" or member.size <= 1024 * 1024):
                metadata_bytes += member.size
                check(member.size <= 1024 * 1024 and metadata_bytes <= 16 * 1024**2, "image metadata exceeds limit")
                documents[name.as_posix()] = archive.extractfile(member).read()
    check("manifest.json" in documents, "Docker save manifest is missing")
    manifest = loads(documents["manifest.json"])
    check(isinstance(manifest, list) and len(manifest) == 1, "exactly one backend image required")
    config_name = manifest[0].get("Config")
    check(config_name in documents, "image configuration is missing")
    config_raw = documents[config_name]
    config_digest = "sha256:" + hashlib.sha256(config_raw).hexdigest()
    if config_digest != backend["image_id"]:
        check("index.json" in documents, "archive image ID differs")
        roots = loads(documents["index.json"])["manifests"]
        check(len(roots) == 1 and roots[0]["digest"] == backend["image_id"], "archive image ID differs")
        def connects(descriptor, depth=0):
            digest = descriptor["digest"]
            check(depth < 8 and digest in blobs and blobs[digest] == descriptor["size"], "invalid OCI descriptor")
            key = "blobs/sha256/" + digest.removeprefix("sha256:")
            check(key in documents, "OCI metadata missing or too large")
            node = loads(documents[key])
            if "manifests" in node:
                return any(connects(child, depth + 1) for child in node["manifests"]
                           if child.get("platform", {}).get("os", "linux") == "linux" and
                           child.get("platform", {}).get("architecture", "amd64") == "amd64")
            check(all(layer["digest"] in blobs and blobs[layer["digest"]] == layer["size"] for layer in node["layers"]), "OCI layer missing")
            return node["config"]["digest"] == config_digest and node["config"]["size"] == len(config_raw)
        check(connects(roots[0]), "OCI image does not reference selected configuration")
    config = loads(config_raw)
    check(config["os"] == "linux" and config["architecture"] == "amd64" and config["config"]["User"] == "1000:1000",
          "Linux amd64 non-root image required")
    check(config["config"].get("Labels", {}).get("org.opencontainers.image.revision") == backend["revision"], "archive source revision differs")
    return config_digest


def authenticated_manifest(directory, trust):
    directory, trust = directory.resolve(), trust.resolve()
    check(not trust.is_relative_to(directory), "trust policy must be external to the bundle")
    expected = {"manifest.json", "manifest.sig", "backend-image.tar.gz", "LICENSE"}
    check(directory.is_dir() and {p.name for p in directory.iterdir()} == expected, "bundle has missing/extra files")
    check(all(p.is_file() and not p.is_symlink() for p in directory.iterdir()), "regular bundle files required")
    check((directory / "manifest.json").stat().st_size <= 65536 and (directory / "manifest.sig").stat().st_size == 64,
          "manifest/signature size invalid")
    raw = (directory / "manifest.json").read_bytes()
    manifest = validate_manifest(loads(raw))
    trusted = policy(trust)
    check(manifest["sequence"] >= trusted["minimum_sequence"], "release is below trust policy sequence floor")
    key = next((k for k in trusted["keys"] if k["id"] == manifest["key_id"]), None)
    check(key is not None and not key["revoked"], "unknown or revoked release key")
    key_path = trust.parent / key["public_key"]
    check(not key_path.is_symlink() and not key_path.resolve().is_relative_to(directory), "external trusted key required")
    check(hashlib.sha256(public_der(key_path)).hexdigest() == key["sha256"], "public key fingerprint differs")
    openssl("pkeyutl", "-verify", "-pubin", "-inkey", key_path, "-rawin", "-in", directory / "manifest.json", "-sigfile", directory / "manifest.sig")
    return manifest, hashlib.sha256(raw).hexdigest()


def verify(directory, trust):
    manifest, fingerprint = authenticated_manifest(directory, trust)
    for name in ("backend", "license"):
        artifact = manifest[name]
        path = directory / artifact["file"]
        check(path.stat().st_size == artifact["size"] and sha(path) == artifact["sha256"], "artifact size/hash differs")
    check(archive_identity(directory / manifest["backend"]["file"], manifest["backend"]) == manifest["backend"]["config_digest"], "signed config digest differs")
    return manifest, fingerprint


def docker(*args):
    result = subprocess.run(["/usr/bin/docker", *map(str, args)], capture_output=True, text=True, timeout=300)
    check(result.returncode == 0, "Docker release operation failed")
    return result.stdout


def pack(output, image, release_id, sequence, baseline, key, key_id, trust):
    identifier(release_id)
    identifier(baseline)
    identifier(key_id)
    check(type(sequence) is int and sequence > 0, "positive release sequence required")
    info = loads(docker("image", "inspect", image))[0]
    output.mkdir(parents=True, exist_ok=False)
    image_id = info["Id"]
    # Stream compressed image bytes; do not materialize multi-GB archives in memory.
    with (output / "backend-image.tar.gz").open("xb") as target:
        process = subprocess.Popen(["/usr/bin/docker", "image", "save", image_id], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        try:
            with gzip.GzipFile(filename="", mode="wb", fileobj=target, mtime=0, compresslevel=1) as compressed:
                shutil.copyfileobj(process.stdout, compressed, 1024 * 1024)
            check(process.wait(timeout=300) == 0, "Docker image save failed")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            process.stdout.close()
    shutil.copyfile(ROOT / "LICENSE", output / "LICENSE")
    def artifact(name):
        return {"file": name, "sha256": sha(output / name), "size": (output / name).stat().st_size}
    manifest = {"schema": 1, "scope": SCOPE, "release_id": release_id, "sequence": sequence, "key_id": key_id,
                "baseline_ids": [baseline], "schemas": {"data": 1, "config": 2}, "frontend": None,
                "backend": {**artifact("backend-image.tar.gz"), "image_id": image_id,
                            "revision": info["Config"].get("Labels", {}).get("org.opencontainers.image.revision"), "platform": "linux/amd64"},
                "license": artifact("LICENSE")}
    manifest["backend"]["config_digest"] = archive_identity(output / "backend-image.tar.gz", manifest["backend"])
    validate_manifest(manifest)
    (output / "manifest.json").write_bytes(dumps(manifest))
    openssl("pkeyutl", "-sign", "-rawin", "-inkey", key, "-in", output / "manifest.json", "-out", output / "manifest.sig")
    return verify(output, trust)
