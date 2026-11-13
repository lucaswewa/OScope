# ex-shm-reader

A second process reading the server's preview ring, the desktop app's fast path (P19, [ADR-0015](../../docs/adr/0015-the-sidecar-contract-service-mode-and-the-desktop-fast-path.md)). It reports the rate previews come at, and any torn reads.

The ring is named shared memory: one writer, the server, and any number of readers, without locks. Each of its slots holds one preview behind a seqlock, so a reader the writer overtakes reads it again, and never returns half of one.

## How to try it

```sh
cargo run -p ex-shm-reader
```

It starts the server, managed, with the fake camera streaming. It reads the ring for 10 s (`--seconds N` to change), checking each preview's header and JPEG, and says each second how it is going:

```text
ex-shm-reader: reading the ring preview-25188, which process 25188 writes, for 10 s
  1 s: 16 previews; frame 15 at 640 × 480, ready 82 ms after its exposure
  …
ex-shm-reader: 160 previews, 16.0 a second, 0.2 MB/s; 0 torn reads read again, 0 overtaken
```

- **The rate** is the camera's, through acquisition: the ring is never written faster.
- **Torn reads** are reads the writer overtook, and that were read again: rare, at a camera's rate.
- **Overtaken** counts reads abandoned after 100 tries, which only a writer far faster than any camera causes.

**Another server's ring:** set `OSCOPE_RING` to its name, from a managed server's ready line (`shm`), or `preview` for a server whose configuration has `[shm] enabled = true`:

```sh
OSCOPE_RING=preview cargo run -p ex-shm-reader -- --seconds 30
```

The server's preview defaults set the ring's previews: `GET /api/v1/preview/defaults`.

**`--smoke`** reads its own server's ring for 2 s, and checks that at least 3 previews came, each a JPEG with a good header:

```sh
cargo run -p ex-shm-reader -- --smoke
```

It ends with `smoke: ok`, or says what failed and exits with code 1.
