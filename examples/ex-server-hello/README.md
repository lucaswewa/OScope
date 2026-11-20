# ex-server-hello

OScope's server with its `system` Thing, and a page beside it at the same origin (P04).

The page reads the Thing's Description, then every property it lists, once a second:

- the version and the Git commit;
- the operating system and the processor;
- the memory;
- the uptime;
- the data and scratch roots, with their free space;
- the scratch space's health: whether there is room to record (P29).

Below them, it shows the Thing Description itself.

## How to try it

```sh
cargo run -p ex-server-hello
```

It prints `ex-server-hello: open http://127.0.0.1:8211/`. Open that address in a browser. You should see the table of properties, with the uptime counting up. Ctrl-C stops the server.

- **Another port:** `cargo run -p ex-server-hello -- --port 8212`.
- **The files:** the example keeps its data and scratch roots in `.oscope/ex-server-hello/`.
- **The page** is [`page/index.html`](page/index.html) and [`page/app.js`](page/app.js). Edits show when you reload, without a restart.

The server answers more than the page, at the same address:

- `/api/v1/system/`, the Thing Description;
- `/api/v1/system/os`, one property;
- `/openapi.json` and `/docs`, the OpenAPI document and its pages;
- `/api/v1/media/ping`, the media plane's `pong`.

**`--smoke`** checks what the page needs, on a port the system picks, with a temporary data root:

```sh
cargo run -p ex-server-hello -- --smoke
```

It checks four things:

- the page and its script are served;
- the Thing Description lists 10 properties;
- each property answers at the address its form gives;
- the media plane answers `pong`.

It prints each property's value, then `smoke: ok`. If a check fails, it says which, and exits with code 1.
