# ex-logs

OScope's logs on a page (P05):

- the live tail, over the media plane's WebSocket;
- filters: by level, by the start of the target, and by text;
- a button for each level, which writes a note through the `logs` Thing;
- the daily files, to download.

To give the tail something to show, the example writes a `tick` every 2 seconds. Every tenth second the tick is a warning inside a job, `demo-N`, so the record carries `job=demo-N`.

## How to try it

```sh
cargo run -p ex-logs
```

It prints `ex-logs: open http://127.0.0.1:8213/`. Open that address in a browser. You should see:

- **the tail:** the server's first records and the ticks, newest at the bottom. Each level has its own colour, and a critical record is white on red;
- **the filter:** choose `warn` and press *Apply*. The table clears, then shows only the job warnings;
- **the buttons:** type a note and press a level. The note appears in the tail if the filter lets it through;
- **Pause:** it freezes the table. Unticking it reloads the newest records;
- **the daily file,** today's `oscope-YYYY-MM-DD.jsonl`, with a download link. Each line is one record, as JSON.

The console shows the same records. The example keeps its files in `.oscope/ex-logs/data/logs/`. Another port: `cargo run -p ex-logs -- --port 8214`.

The server answers more than the page, at the same address:

- `/api/v1/logs/records?level=warn&limit=5`: the five newest warnings and worse, newest first;
- `/api/v1/logs/records?text=tick&before=N`: the next page, where `N` is a page's `next`;
- `/api/v1/logs/files`: the daily files;
- `/api/v1/logs/`: the `logs` Thing's description.

**`--smoke`** runs headless, on a port the system picks, with a temporary data root:

```sh
cargo run -p ex-logs -- --smoke
```

It checks four things:

- the page and its script are served;
- a note at each level, written as the buttons do, comes back over the tail, in order;
- the query returns the six notes, newest first;
- today's file holds them.

It prints what it saw, then `smoke: ok`. If a check fails, it says which, and exits with code 1.
