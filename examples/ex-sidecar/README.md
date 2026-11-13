# ex-sidecar

A small supervisor, as the desktop app will be: it runs the server by the sidecar contract (P19, [ADR-0015](../../docs/adr/0015-the-sidecar-contract-service-mode-and-the-desktop-fast-path.md)).

- It starts the server with `--managed`, and reads its JSON ready line: its port, its process ID, where its local token is, and its preview ring's name.
- It starts it again whenever it exits with code 75.
- On Ctrl-C, it closes the server's stdin, which stops it.

The example runs itself as the server, through `oscope-server`'s library, so there's nothing else to build. The server uses the fake devices, and keeps its folders in `.oscope/ex-sidecar/data/server`.

## How to try it

```sh
cargo run -p ex-sidecar
```

It prints what the ready line says:

```text
ex-sidecar: process 23760 is ready on http://127.0.0.1:65307/, its token in …\run\local.token, its preview ring preview-23760
ex-sidecar: to restart it: curl -X POST http://127.0.0.1:65307/api/v1/system/restart
```

The port is any free one, as the contract has it. From another terminal, ask for a restart with the `curl` it printed:

```text
ex-sidecar: it exited with 75, to be restarted: starting it again
ex-sidecar: process 11984 is ready on http://127.0.0.1:65310/, …
```

- **A new process** on a new port, with a new token.
- **Ctrl-C** closes the server's stdin, and the supervisor says it exited with 0.
- **Any other exit code** stops the supervising, as a wrong configuration's 2 would: restarting wouldn't help.

**`--smoke`** checks it with a temporary data root:

- that the server is ready, with its own process ID in the ready line, and answers;
- that asked to restart, it exits with 75;
- that started again, it is a new process, and answers;
- that with its stdin closed, it exits with 0.

```sh
cargo run -p ex-sidecar -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
