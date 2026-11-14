# ex-omext

Extensions' packages (P24, [ADR-0018](../../docs/adr/0018-the-omext-package-format.md)): an `.omext` packed from a folder, installed through the API's upload route, and listed by the `extensions` Thing. Every change waits for the next start, as extensions load only then.

The extension is a dummy, `org.oscope.examples.hello`, in [`hello/`](hello): a Python extension that provides a node, `hello.greet`, and a panel, `hello.greeting`. OScope runs no Python extension before P26, so it installs, loads and lists it, and nothing of it runs.

## How to try it

```sh
cargo run -p ex-omext
```

It packs `hello/` into `.oscope/ex-omext/scratch/org.oscope.examples.hello-0.1.0.omext` and prints its SHA-256. Packing the same folder gives the same bytes each time. Then it serves on port 8230, installs the package through the API as a client would, and prints what is installed:

```text
ex-omext: POST /api/v1/extensions/install: installed
ex-omext:   org.oscope.examples.hello 0.1.0 by OScope examples: loaded none, the next start 0.1.0
```

Then it serves until Ctrl-C. When the `system` Thing's `restart` asks, it builds its server again, as a start does: that loads the installed extensions. `oscope-server` exits with code 75 for that instead, and its supervisor starts it again (ADR-0015).

**The page,** `http://127.0.0.1:8230/`:

- **Installed:** each extension's name, ID, publisher, what it provides, the start of its package's SHA-256 (the whole of it on hover), and where it stands: the version this start loaded, and what the next start will do.
- **Start again** shows while a change waits for the next start. It restarts the server, and the table shows the extension *loaded 0.1.0*.
- **Provided by the extensions loaded:** each name, and the extension that provides it.
- **Disable**, **Enable** and **Uninstall**, each for the next start. Uninstalling an extension this start didn't load removes it at once.
- **Install** uploads an `.omext` you choose. Pack your own with `cargo run -p oscope-server -- ext pack <folder>`.

**Things to try:**

- **Install the same package again:** it answers *Installed already*, and nothing changes.
- **A new version:** change `version` in `hello/manifest.toml` to `0.2.0`, pack it with `cargo run -p oscope-server -- ext pack examples/ex-omext/hello --out .oscope/hello-0.2.0.omext`, and install that file from the page. The table shows *loaded 0.1.0* and *the next start loads 0.2.0*. Start again: 0.2.0 is loaded, and 0.1.0's folder is gone from `.oscope/ex-omext/data/extensions/org.oscope.examples.hello/`. Put the version back afterwards.
- **Another API:** change `oscope-api` to `"^2.0"`, pack it the same way and install it: refused, as this OScope offers the extension API 1.0.0. Put it back afterwards.

**With `curl`:**

```sh
curl http://127.0.0.1:8230/api/v1/extensions/installed
curl http://127.0.0.1:8230/api/v1/extensions/extension/org.oscope.examples.hello
curl http://127.0.0.1:8230/api/v1/extensions/provided
curl -X POST --data-binary @.oscope/ex-omext/scratch/org.oscope.examples.hello-0.1.0.omext \
     -H "Content-Type: application/octet-stream" http://127.0.0.1:8230/api/v1/extensions/install
curl -X POST -H "Content-Type: application/json" -d '{"id": "org.oscope.examples.hello"}' \
     http://127.0.0.1:8230/api/v1/extensions/disable
curl -X POST -H "Content-Type: application/json" -d '{}' http://127.0.0.1:8230/api/v1/system/restart
curl -N http://127.0.0.1:8230/api/v1/extensions/changed
```

The upload route answers 201 when it unpacked the package, 200 when it was there already, 400 for a package it refuses (with why), 409 for the same version from another package, and 413 for one over 1 GiB.

**`--smoke`** installs the package twice, checks that a package for another API and one with an entry named `../escaped.txt` are refused, restarts the server, and checks that the extension is loaded and its names registered.

## Files

- `src/main.rs`: packing, installing and listing as a client, and the restarts.
- `hello/`: the dummy extension: `manifest.toml`, a Python module, and a panel's schema as an asset.
- `page/`: the page.
