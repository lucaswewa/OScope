# ADR-0018: The `.omext` package format

- Status: Accepted
- Date: 2026-11-14
- Phase: P24

## Context

- **D11 and plan §5.8:** extensions add processing nodes, hardware drivers, scan workflows, panels, importers, exporters and reports. Rust extensions are native libraries (D9, ADR-0019); Python extensions run in workers with environments of their own (D10, ADR-0020). Each kind arrives with the feature it extends.
- **teta-wot fixes its Things when the server is built** (ADR-0004). Extensions that add Things, such as drivers and workflows, can only load at start-up, so all extensions load then.
- **Extensions are trusted code.** A native library runs in the server's process, and OScope can't sandbox it. But a package's files must go only where they belong, and a package that doesn't suit this OScope must not load.
- **One package serves both platforms** (ADR-0003), so a Rust extension carries a library for each.
- **Packages arrive over the API,** from the M2 app or `curl`, so the server makes the checks.

## Decision

**1. A package** is a zip file, `.omext`, with `manifest.toml` at its root:

| Field | | What |
|---|---|---|
| `id` | required | Reverse DNS: at least two dotted names of lower-case letters, digits, `-` and `_`, each starting with a letter; at most 100 characters. Lower case only, as the ID names a folder, and two IDs differing in case would share one on Windows |
| `name`, `publisher` | required | Not empty. The publisher is shown with the package's hash |
| `version` | required | Semver 2.0 |
| `license` | required | An SPDX expression. It isn't checked against OScope's own list: an extension and its dependencies are its author's and its user's (the owner's decision for P24–P34) |
| `description` | optional | |
| `oscope-api` | required | A semver range of the extension API it works with, such as `^1.0` |
| `kind` | required | `rust` or `python` |
| `assets` | optional | Files the package uses as they are: panel schemas and report templates |
| `[provides]` | required | Lists of names for `nodes`, `drivers`, `workflows`, `panels`, `importers`, `exporters` and `reports`: dotted names, each given once, and at least one in all |
| `[rust.libraries]` | a Rust package's | A path in the package for each platform it is built for: `windows-x86_64`, `linux-x86_64` |
| `[python]` | a Python package's | `requires-python`, `dependencies` (as uv takes them) and `entry`, a module's dotted name |

Unknown fields are refused, so a typo is caught. A manifest that can't be read, and whose `oscope-api` range doesn't match this OScope, is refused for its API first: it may come from a newer API, with fields this OScope lacks. Every path the manifest names must be in the package.

**2. The extension API** has a version of its own, apart from OScope's: **1.0.0** from P24. A minor version adds to it, such as a new kind of contribution. A major version breaks it. A package installs and loads only if its `oscope-api` range matches, and a Rust package only with a library for this platform.

**3. Unpacking with care.** A package is refused, before anything is written, unless:
- it is a zip file of at most 10,000 entries, saying they unpack to at most 4 GiB in all;
- each entry's name is a relative path of `/`-separated parts, each a name that Windows and Linux both create as it is. So no absolute paths, no `.` or `..` parts, no empty parts, none of `\ : * ? " < > |` or control characters (no drive letters or backslashes, then), no name ending in a dot or a space, and no name Windows keeps for a device (`CON`, `NUL`, `COM1`…);
- no two names differ only in case;
- no entry is a symbolic link or encrypted.

Entries unpack only to the paths checked, into a fresh folder, never over a file, and zip refuses an entry that unpacks to more than it says or fails its checksum. Permissions in the package are ignored. A package that fails part-way leaves nothing behind.

**4. On disk,** in the data root:

```text
<data root>/extensions/
├─ .incoming/        packages arriving and unpacking, emptied at each start
└─ <id>/
   ├─ state.json     the version installed, its package's SHA-256, enabled, uninstalled
   └─ <version>/     the package, unpacked
```

`state.json` is written to a file beside it, then renamed over it.

**5. The lifecycle.** Every change waits for the next start, which acts on the files:
- **Installing** checks the package (§3) and the manifest against this OScope (§2), then unpacks it into a folder of its version, beside any other. An extension installed anew is enabled; one installed again keeps its enabled state.
- **Installing a package again** changes nothing. Its SHA-256 says it is the same. The same ID and version from another package is a conflict: a new package needs a new version.
- **Enabling and disabling** set the state.
- **Uninstalling** an extension this start didn't load removes it at once. One it did load is marked, and the next start removes it: on Windows, a loaded library can't be deleted.
- **At start-up,** OScope empties `.incoming`, removes the extensions marked uninstalled, and every version but the installed one. An extension whose folder can't be read is left out, with a warning, and left on disk.

**6. Loading and the registries.** A start loads the extensions that are enabled and suit this OScope, **in the order of their IDs.** It records each name they provide in a registry for its contribution: the name, and the extension that provides it. **An extension that provides a name an extension loaded before it provides isn't loaded at all,** and says why. Loading a contribution's code is each kind's phase: Rust nodes in P25, Python nodes in P26, and so on. Each extension's state tells **the version this start loaded and the version the next start will,** computed with the same rule from its files. The server needs a restart when they differ for any extension.

**7. The `extensions` Thing:**
- **properties:** `installed` (each extension's manifest, its package's SHA-256, enabled, uninstalled, the versions loaded and next, and why the next start can't load it), `provided` (the registries), `restart_needed`, and `host` (the extension API and the platform);
- **`GET extension/{id}`:** one extension, or 404;
- **actions:** `enable`, `disable`, `uninstall`;
- **`POST install`:** the upload route. The request's body is the package, up to 1 GiB, which arrives in `.incoming` and is checked there. It answers the extension: 201 when it unpacked the package, 200 when it was there already. 400 for a package it refuses, with why; 409 for a conflict; 413 for a package too large;
- **the `changed` event:** an extension's ID, when it is installed, enabled, disabled or uninstalled.

Errors take ADR-0034's shape: `unknown` (404), `conflict` (409), `refused` (400), `internal` (500).

**8. Packing.** `oscope-server ext pack <folder>` writes `{id}-{version}.omext`. It packs every file in the folder, in the order of their names, deflated, with the same time (1980-01-01) and permissions (0644) for each. So a folder always packs to the same bytes and hash. It refuses what installing would: a wrong manifest, a file it names that is missing, a symbolic link, or a name a package can't have.

**9. Not yet:**
- **The control lease** (P49) will guard installing, enabling, disabling and uninstalling, as every change of state.
- **Signatures:** none. The SHA-256 and the publisher are shown, so a user can check a package against its source.

## Consequences

- **A package's files can't land outside its folder,** whatever its names say, and a package unpacks the same way on Windows and Linux.
- **Installing is safe to repeat,** so a client can retry an upload, and the M2 app can install what it is given without asking first.
- **A newer version installs while the old one is loaded,** and replaces it at the next start, so an upgrade needs no downtime until the restart.
- **The load order is fixed and plain:** by ID. It needs no dependencies between extensions. If one ever needs another, a later ADR adds them.
- **Each kind's phase only loads code.** Checking, storing and the registries are done once, here.
- **A change always needs a restart.** That is the cost of teta-wot's Things being fixed at start-up. `restart_needed` tells a client, and the `system` Thing's `restart` does it (ADR-0015).
- **What a package claims isn't checked yet:** that its library is a library, or its entry a module. P25 and P26 check when they load, and report through the same state.

## Alternatives considered

- **A tarball, or a wheel-like format.** Zip has an index, so the manifest is read and every name checked before anything unpacks, and every platform has tools for it. A tarball must be read through to list it.
- **A JSON manifest.** TOML is easier to write by hand and takes comments. OScope's own configuration is TOML already.
- **Loading at once, without a restart.** teta-wot fixes its Things at start-up, and a native library can't be unloaded safely. Waiting for the next start keeps one rule for every kind.
- **The extension API's version as OScope's own.** Then every OScope release would break every extension's range, even when the extension API didn't change.
- **Replacing a version in place.** On Windows, a loaded library's folder can't be replaced, and a failed install would leave nothing working. Versions side by side, chosen by the state, avoid both.
- **The newest version installed wins, with no state.** A downgrade would then need an uninstall first. The state says which version, and installing chooses it.
- **Loading the extensions that don't conflict, and leaving out only the names that do.** An extension loaded in part would behave in ways its author never tried. All or nothing is clearer.
- **Signing packages.** OScope has no publisher keys to check against, and a lab installs what it trusts. The hash is shown instead. Signatures can come with a later ADR.
