# P24: `.omext` packages, registry, `extensions` Thing

- Status: In review
- Branch / PR: `m1/p24-omext` / [#38](https://github.com/lucaswewa/OScope/pull/38)
- Size: M · estimated ~2,050 before it started (the manifest and its checks ~250; the zip reader and writer, guarded against traversal ~200; installing, enabling and the load order, with a registry for each kind ~300; the `extensions` Thing and its upload route ~250; `oscope-server ext pack` ~60; tests ~400; `ex-omext` with its page ~400; and a tenth on top); one pull request, at the owner's standing choice for P24–P34 · measured **2,821** hand-written lines added (code 1,485, of which the Cargo manifests 22; tests 789; examples 547), and 12 removed. **That is 2,221 over M's 600, and 38 % over the estimate.**

## Summary

Extensions' packages, and their life on the server ([spec](../phases.md#p24); plan §5.8; D11; [ADR-0018](../../adr/0018-the-omext-package-format.md)).

- **`oscope-ext`:**
  - the manifest, `manifest.toml`, and its checks;
  - a package reader that refuses, before writing anything, a package whose files could land outside its folder or clash on Windows;
  - packing a folder into the same bytes each time;
  - installed extensions in the data root: installing, enabling, disabling and uninstalling, each for the next start; the load order, and a registry for each kind of contribution.
- **`oscope-api`:** the `extensions` Thing, with an upload route for packages.
- **`oscope-server`:** opens the extensions at each start, and packs a folder with `ext pack`.
- **`ex-omext`:** a dummy extension packed, installed through the API and listed; a page to install, enable, disable and uninstall, and to start the server again.
- **The ports for manual checks** gain 8230–8249, as the owner decided for this run: in CLAUDE.md, `phases.md` and the examples' conventions. `ex-omext` takes 8230.

Nothing loads an extension's code yet: Rust nodes come in P25, Python nodes in P26.

## What was built

| Where | What |
|---|---|
| `oscope-ext` `manifest.rs` | `Manifest`, `Kind`, `Contribution`, `Rust`, `Python`; `parse`, `check` (on its own) and `check_host` (against this OScope); `API_VERSION` 1.0.0; `PLATFORM` and `PLATFORMS`. The rule for paths in a package |
| `oscope-ext` `package.rs` | `Package::open`, which checks every entry and the manifest; `unpack`; `pack`, the same bytes each time; `sha256` |
| `oscope-ext` `installed.rs` | `Extensions`: `open` (a start: clean-up, then the load plan), `empty`, `install`, `enable`, `disable`, `uninstall`, `list`, `get`, `loaded`, `registry`, `restart_needed`, `incoming`. `Extension`, `Loaded`, `Registry` |
| `oscope-ext` | `ExtError`: the manifest, an incompatible API, no build for this platform, the package, a conflict, an unknown ID, a file |
| `oscope-api` `extensions.rs` | The `extensions` Thing: `installed`, `provided`, `restart_needed` and `host`; `GET extension/{id}`; `POST install`, the upload route; `enable`, `disable` and `uninstall`; the `changed` event. `ApiBuilder::extensions` |
| `oscope-server` | `Extensions::open` at each start, `Extensions::empty` for the printed documents; `ext pack <folder> [--out <file>]` |
| `examples/common` | `post_bytes`, a POST of any body |
| `examples/ex-omext` | The dummy extension, `hello/`; packing, installing and listing as a client; starting its server again when asked; the page |

## How to try it

```sh
cargo run -p ex-omext
```

It packs and installs the dummy extension, prints what is installed, and serves the page on `http://127.0.0.1:8230/`. **Start again** loads the extension. The [README](../../../examples/ex-omext/README.md) has more to try: a new version, a package for another API, and `curl`.

## Performance

No budget of plan §5.20 is P24's.

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P24–P34. It came in 38 % over the estimate. The tests took nearly twice theirs (789 against ~400): sixteen hostile packages, every change at the next start, the Thing over HTTP and the command line. The example took more for its server's restart and its checks (547 against ~400), and the lifecycle for its load plan (438 against ~300).
- **The extension API has a version of its own,** 1.0.0, apart from OScope's 0.1.0. A manifest's `oscope-api` range is checked against it. Tied to OScope's version, every OScope release would have broken every extension's range. A minor version adds to the API, such as a new kind of contribution, and a major version breaks it.
- **A new extension is enabled when it is installed,** and loads at the next start. The spec doesn't say. Installing is a deliberate act, and a second step to enable would add nothing.
- **When two extensions provide one name,** the first by ID loads, and the other doesn't load at all, saying why. Loading it in part would make it behave as its author never tried.
- **An extension's licence isn't checked** against OScope's list. An extension and its packages are its author's and its user's, as you decided for Python extensions' packages.
- **Uninstalling an extension this start didn't load removes it at once.** One it did load goes at the next start: Windows can't delete a loaded library.
- **The upload route takes up to 1 GiB.** A Python extension carrying wheels for offline installs can be large. The 413 for a larger package isn't tested: a test would need to send more than 1 GiB.
- **The control lease** (P49) will guard the changes, as every change of state. Until then, any client may make them.
- **`ex-omext` starts its server again in its own process** when `system/restart` asks, so that the page can show extensions loading. It listens again on the same port. A copy of the listening socket, handed to the new server, never accepted a connection on Windows, so it binds the port again: for a few milliseconds connections are refused, and the page and the smoke checks try again.
- **The dummy extension is a Python one,** as it needs no build for each platform. It provides a node and a panel by name only. P26 will decide whether to run it or replace it.

**Other choices:**

- **Manifests refuse unknown fields,** to catch typos. A manifest that can't be read, and whose `oscope-api` range doesn't match, is refused for its API first: it may come from a newer API.
- **Paths in a package** must be ones that Windows and Linux both create as they are, so a package unpacks the same way on both: no `\`, `:` or the other characters Windows refuses, no name ending in a dot or a space, no device names, and no two names differing only in case.
- **Zip 9 refuses an entry that unpacks to more than it says,** on top of OScope's total of 4 GiB from what entries say. A count of OScope's own was dropped, as zip's check came first and made it unreachable.
- **Installs are side by side:** a new version unpacks beside the loaded one, and `state.json` says which the next start loads. It is written beside, then renamed over the old.
- **An upload arrives in `.incoming`,** on the same volume as the extensions, so unpacking needs no copy. A start empties it.
- **Packing is the same each time:** deflated with the pure-Rust zlib backend already in the lock, a fixed time (1980-01-01) and permissions (0644), and the files in name order. The test changes the files' times between two packs.
- **Printing the documents** (`--print-openapi`, `--print-td`) reads no extension, so it still creates nothing.
- **The page shows a package's text as text,** never as HTML: a package is the uploader's.

**New dependencies,** each checked for its licence and age:

| Crate | Version | Released | Licence | Why |
|---|---|---|---|---|
| `zip` | 9.1.3, pinned | 2026-10-27 | MIT | Reading and writing packages. 9.2.0 was under two weeks old |
| `semver` | 1.0.28 | 2026-04-04 | MIT or Apache-2.0 | Versions and ranges |
| `sha2` | 0.10.9 | 2025-04-30 | MIT or Apache-2.0 | Packages' SHA-256. 0.10 shares `sha1`'s `digest`, already in the lock |
| `typed-path` | 0.13.0 | 2026-10-20 | MIT or Apache-2.0 | `zip`'s |
| `nix` | 0.31.3 | 2026-05-11 | MIT | `zip`'s, on Linux |
| `cfg_aliases` | 0.2.2 | 2026-07-16 | MIT | `nix`'s, to build |

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-ext` | `packages` (7) | **Manifests:** 18 mistakes, each refused with where or why; a Rust package's platforms and paths; a newer API's manifest refused for its API, even with fields this OScope lacks. **Sixteen hostile packages,** each refused with nothing written anywhere: `..` (twice), an absolute path, a drive letter, backslashes, `.` and empty parts, a device name, a trailing dot, names differing in case, a symbolic link, entries saying they unpack to over 4 GiB, an entry unpacking to more than it says, no manifest, not a zip, a missing asset. **Another API,** and no build for this platform: refused. **Installing again:** unchanged, its state file untouched; the same version from another package: a conflict. **Each start:** disabling, enabling, a new version beside the loaded one and the old removed after, uninstalling while loaded and while not, an unfinished install cleared. **Two extensions providing a name:** the first by ID loads, and the other says why not. **Packing:** the same bytes after the files' times change, unpacking to the same files; a symbolic link (where one can be made) and a missing asset refused |
| `oscope-api` | `extensions` (3) | **Over HTTP:** an upload installs for the next start (201) and says so in `changed`; again, it changes nothing (200); listed, and by ID, with 404 for none; the next start loads it, and `provided` names it; disabling, enabling and uninstalling, each for the next start. **Refused:** no body, not a zip, another API (400), the same version from another package (409), an action on an unknown ID (404). **The Thing's description,** as a snapshot |
| `oscope-server` | `cli` (1 new) | `ext pack` names the package by its ID and version and prints its SHA-256; `--out` gets the same bytes; a wrong manifest fails with where and why, and writes nothing. The existing test still finds that printing the documents creates nothing |
| `ex-omext` | `--smoke` | Installed, then again unchanged; another API and `../escaped.txt` refused; after `system/restart`, loaded, with its names registered; the page and its script served |

**Where they ran:** on Windows, and on Linux under WSL; the three suites and `ex-omext --smoke` 5 times out of 5 each with their binaries pinned to two cores. The page's script ran under Node against the example's server, with no error, and a restart through the API loaded the extension.

**Mutation checks** (each broke the code on purpose, then the tests were run): all eleven caught.

| Broken on purpose | Caught by |
|---|---|
| `..` allowed in a path | `a_manifest_says_what_is_wrong_with_it`, `a_package_that_could_escape_or_clash_is_refused_and_leaves_nothing` |
| Names differing in case allowed | `a_package_that_could_escape_or_clash_is_refused_and_leaves_nothing` |
| Symbolic links allowed | the same |
| The total size unchecked | the same |
| A package installed again unpacked again | `installing_a_package_again_changes_nothing` |
| Two extensions providing a name both loaded | `of_two_extensions_providing_a_name_the_first_by_id_loads` |
| The API and platform unchecked when installing | `a_package_for_another_api_or_platform_is_refused` |
| Old versions kept at a start | `each_change_takes_effect_at_the_next_start` |
| `.incoming` kept at a start | the same |
| Uninstalled extensions kept at a start | the same |
| The upload route answering 201 when it unpacked nothing | `a_package_uploaded_loads_at_the_next_start` |

## Follow-ups and known gaps

- **Loading code:** Rust nodes in P25, which also refuses names the stock nodes have; Python nodes in P26.
- **The control lease** over the changes (P49).
- **The 413** for a package over 1 GiB isn't tested.
- **Signatures,** if labs want them: a later ADR.

## Clean room

Nothing was copied from `microscope` or any other codebase. No published algorithm is involved. The rules were written from:

- **zip:** PKWARE's *APPNOTE.TXT, .ZIP File Format Specification*, 6.3.10, for the headers' layout that the tests change;
- **names on Windows:** Microsoft's *Naming Files, Paths, and Namespaces*, for the characters and device names Windows refuses;
- **versions:** *Semantic Versioning 2.0.0*.
