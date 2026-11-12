# oscope-server

The program that runs the microscope. It will hold the configuration, service mode and the sidecar contract. For now, it answers only `--version` and `--help`: it is P01's example, and shows that the workspace builds and runs.

## How to try it

From a clean checkout, with [rustup](https://rustup.rs/) installed:

```sh
cargo run -p oscope-server -- --version
```

The first run installs the pinned toolchain (`rust-toolchain.toml`) and fetches the workspace's dependencies, `teta-wot` v0.2.0 among them. It then prints:

```text
oscope-server 0.1.0
```

`--help` prints the usage. With anything else, or with nothing at all, the program prints the usage to standard error and exits with code 2, since there is nothing to serve yet.
