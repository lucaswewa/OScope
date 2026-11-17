# ex-settings

P52's settings: one place for them, and an old configuration file upgraded safely.

1. **A configuration file of version 1,** as P01–P51 wrote them, is written and migrated:
   - `[server]` becomes `[network]`, and `[devices.simulator]` becomes `[simulator]`;
   - its comments and order are kept, and `version = 2` is added;
   - the old file is kept beside it, as `oscope.toml.v1.bak`.
2. **The `settings` Thing:**
   - `system`: the file's data and scratch roots, address and port;
   - `simulator`: the file's simulator settings, whole;
   - `display`: your own display preferences, or else `display_defaults`. Writing your own needs no control lease;
   - `restart_needed`, once the file has changed: a restart takes the change up;
   - `document`, all of them at once, which `import` takes.
3. **The script:**
   - reads the file's settings;
   - sets your display preferences;
   - changes the sample and the port, for the next start;
   - is refused a clock rate of 0, as the next start would refuse it;
   - imports the settings' own document again;
   - prints the file.
4. **The page** sets your display preferences as you change them, shows the file's system settings, and imports a document you edit.

## How to try it

```sh
cargo run -p ex-settings
```

It serves at http://127.0.0.1:8219/, and prints:

```text
ex-settings: migrated: [server] is now [network]; [devices.simulator] is now [simulator]
ex-settings: open http://127.0.0.1:8219/
  the file's: port 8300, the sample "H&E tissue"
  your display: {"theme":"dark","units":"mm","scale_bar":true,"colormap":"viridis"}
  the next start's: the sample H-DAB tissue, port 8301; a restart needed: true
  a clock rate of 0: 400, the simulator's settings: a clock rate of 0
  imported: ["system","simulator","display_defaults","accounts"]
  the file now:
    version = 2
    # ex-settings' configuration, as P01–P51 wrote them: version 1's.

    [network]
    # This computer only.
    address = "127.0.0.1"
    port = 8301

    [storage]
    data_root = "data"
    scratch_root = "scratch"

    [simulator]
    # The sample loaded at start.
    sample = "H-DAB tissue"
  open http://127.0.0.1:8219/ to try them
```

The file is in `.oscope/ex-settings/data/config/`, with `oscope.toml.v1.bak` and `oscope.toml.bak` beside it.

**With the server,** the same file is the one given with `--config`. Migrating one by hand:

```sh
cargo run -p oscope-server -- --config my.toml config migrate
```

## Its checks

`cargo run -p ex-settings -- --smoke` runs the script, then checks that:

- your display preferences are written without the lease, and read back;
- the sample and the port are written, and a restart is needed;
- a clock rate of 0 is refused with a 400;
- the document imports every part;
- the file is version 2's, with its comments, the new port and the new sample;
- the old file is kept as it was;
- the page and its script are served.
