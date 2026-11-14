# P28: Projects, OME-Zarr store, SQLite index

- Status: In review
- Branch / PR: `m1/p28-projects-store` / [#42](https://github.com/lucaswewa/OScope/pull/42)
- Size: L · estimated ~3,600 before it started (records and folders ~400; OME-Zarr through `zarrs` ~450; the index, its search, paging and rebuild ~350; thumbnails ~100; the `projects` Thing ~350; the server's wiring ~50; tests ~800; `ex-projects` and its page ~450; and a fifth on top); one pull request, at the owner's standing choice for P24–P34 · measured measured **3,722** hand-written lines added (Rust code 2,267, of which `oscope-store` 1,980; tests 1,042; `ex-projects` 408; glue 5), and 9 removed. **That is 2,822 over L's 900, and about 3 % over the estimate.**

## Summary

Where every acquisition, result and report lives ([spec](../phases.md#p28); D15, D16; plan §5.12; [ADR-0021](../../adr/0021-projects-ome-zarr-and-the-index.md)).

- **`oscope-store`:**
  - **projects and their records** in folders under the data root, each a versioned document; a record drafted, then committed; changes under a lock that other processes respect;
  - **images as OME-Zarr v3,** through `zarrs` 0.22: NGFF 0.5 multiscales, chunks of 512 × 512 pixels holding a plane's channels, zstd;
  - **the SQLite index,** in `index.sqlite` beside the jobs: search, paging, and a rebuild from the folders that equals the live index;
  - **thumbnails,** made when a record with an image is committed.
- **The `projects` Thing,** for the Gallery: list, search and page; one project's contents; thumbnails; create, rename, set the sample, tag, set metadata, delete; rebuild the index.
- **`oscope-server`** opens the projects at start-up. **`oscope-core`** gains `ReportId`.
- **`ex-projects`:** two projects of the simulated slide, a Gallery-style page of their cards, and a script that opens a store with zarr and ome-zarr-py.

## What was built

| Where | What |
|---|---|
| `oscope-store` `records.rs` | `Project`, `Acquisition` (and `FrameRecord`), `ResultRecord` (and `Provenance`, `NodeVersion`, `Input`), `Report`: versioned documents (`oscope.project`… version 1); `Record`, `RecordKind` |
| `oscope-store` `image.rs` | `ImageWriter` (regions at level 0, then the pyramid), `ImageReader`, `ImageInfo`, `Samples` (`u8`, `u16`, `f32`, interleaved), `PixelSize`; NGFF 0.5's metadata |
| `oscope-store` `index.rs` | The index: its schema and version, pages and search, records, the rebuild's replacement, its rows |
| `oscope-store` `store.rs` | `Store`: `create`, `project`, `contents`, `change`, `delete`, `list`; `acquisition`, `result`, `report`, giving a `Draft`; `thumbnail`; `rebuild_index`, `index_rows` |
| `oscope-store` `thumbnail.rs` | JPEG thumbnails of at most 256 pixels |
| `oscope-store` `tests/ngff-0.5` | NGFF 0.5's `image.schema`, `strict_image.schema` and `_version.schema`, unmodified, with their notice |
| `oscope-api` `projects.rs` | The `projects` Thing; `ApiBuilder::projects` |
| `oscope-server` | The store opened at start-up, its index at `{data root}/index.sqlite`; in memory for the printed documents |
| `oscope-core` | `ReportId` |
| `examples/ex-projects` | The example, its page (`index.html`, `app.js`), and `open_store.py` |

## How to try it

```sh
cargo run -p ex-projects        # the Gallery's cards on http://127.0.0.1:8233/
```

The [README](../../../examples/ex-projects/README.md) says what the page does, how each project's folder looks, and how to open a store in Python (`open_store.py`, through uv) or napari.

## Performance

No budget of plan §5.20 is P28's. On the owner's PC, in release builds:

| | Time |
|---|---|
| A MIChrome 20 frame, 5,472 × 3,648 in 16 bits (40 MB), written at level 0 | 67–76 ms: 525–594 MB/s |
| The same frame with its pyramid, four levels more | 265–270 ms |
| A 512 × 512 tile read from level 0 | 0.7–1.0 ms |
| An acquisition of 2,048 × 1,536 RGB committed: its pyramid, its thumbnail, its document and the index | 103 ms |
| Making a project | 1.45 ms |
| A page of 50 of 1,000 projects; a search of them by text and tag | 1.25 ms; 0.23 ms |
| Rebuilding the index of 1,000 projects from their folders | 96 ms |

Each was measured three times or more, by a test written for it and then removed.

**Compression:** zstd at level 3 stores the simulated H&E fields of `ex-projects` in 17.1–19.0 MB, for 24.8 MB of samples over their three levels: 1.30 to 1.44 times smaller, as the simulator's noise allows. The frame above, with 400 levels of random noise in every pixel, took 47.3 MB for its 53 MB.

Writing level 0 alone runs at about P29's 580 MB/s, but not with the pyramid. P29 records frames to a scratch log first, and its ingest job writes them into stores afterwards.

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P24–P34. It came in about 3 % over the estimate, and 2,822 lines over L's 900. The store's folders, drafts and lock, 604 lines, took more than estimated; the tests are more than a quarter.
- **`zarrs` 0.22, not 0.23.** 0.23 requires `paste`, which RustSec marks unmaintained (RUSTSEC-2024-0436), and `cargo deny` refuses it. I tried to add the advisory to `deny.toml`'s `ignore`, with a reason; auto mode's checks stopped me, as that is your decision. 0.22.10 (2025-11-29) has none of it, and everything P28 uses. If you would rather have 0.23, its advisory needs your word.
- **The `zstd` crates and three advisories.** RustSec's RUSTSEC-2026-0391, -0398 and -0403 are fixed in `zstd` 0.13.4 and `zstd-safe` 7.4.0, released on 2026-11-01. P28 was written against 0.13.3 and 7.3.0, and took the fixed versions once two weeks old, on 2026-11-15 after 14:12 UTC, before opening the pull request.
- **`jsonschema` 0.33.0, not 0.60.** You chose the `jsonschema` crate for the NGFF checks. Its newer versions need `borrow-or-share` 0.2.4, whose licence, MIT-0, isn't among `deny.toml`'s. 0.33.0 takes `fluent-uri` 0.3, which takes any `borrow-or-share` 0.2: 0.2.1 is licensed CC0-1.0 OR MIT, both allowed. 0.2.0, 0.2.1 and 0.2.2 came out the same day, and differ only in their licence.
- **NGFF's schemas' licence is inferred.** The schemas live in `ome/ngff-spec`, which has no licence file. `ome/ngff`, which holds it as a submodule, licenses its reports under the W3C Software and Document License, and its software under Apache-2.0: both allow copying with the notice. The fixtures carry the W3C notice and the copyright line of the specification.
- **The stores open in Python:** zarr 3.4.2 and ome-zarr-py 0.21.0 read every level, the axes and the scales, through uv with packages at least two weeks old. uv's newest CPython had no wheel for `numcodecs`, so the README asks for 3.13. **napari itself is yours to try,** as you decided.
- **A project's thumbnail is its newest acquisition's,** not its newest image's: a derived grey map shows a slide less well than the slide does. Without acquisitions, its newest image result's.
- **The records' instrument, objective, calibration, camera and plan are JSON** for now: `oscope-store` is in L2 beside `oscope-hal`, and may not depend on it. P29 and P30, which write acquisitions, fill them.
- **Deleting a project removes it,** with everything in it: there is no wastebasket. The page asks before it deletes.

**Other choices:**

- **The folders are the truth.** Every change writes them first, then the index. A rebuild reads them alone.
- **The index keeps each record's whole document,** so listing a page or a project's contents reads no folder.
- **The image's levels** halve until one fits a chunk, each pixel the mean of the 2 × 2 below it, and are centred on those pixels by a `translation`: napari and ome-zarr-py then overlay them exactly.
- **A record's thumbnail is asked for by its ID,** which must be a UUID: `thumbnail/{id}/..%2F..` could otherwise read a `thumbnail.jpg` outside the project.
- **Thumbnails** come from the level of at most 1,024 pixels across that gives the largest in whole steps: 200 × 117 from a 1,200 × 700 image, 228 from the MIChrome 20's whole sensor.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-store` | `images` (4) | **Round trips** at every level and every plane, for `u8`, `u16` and `f32`, and 1 to 4 channels, of 1,301 × 1,029 written in strips of uneven heights: each level equal to its definition, the mean of 2 × 2 pixels or fewer at an odd edge, and a region across chunks' edges read back. **The MIChrome 20's whole sensor,** 5,472 × 3,648 in 16 bits, in five levels, the last in one chunk. **NGFF 0.5:** the metadata valid against the official `image.schema` and `strict_image.schema`, and the schemas shown to refuse a wrong version or missing axes; the translation of a level, the units; each array a Zarr v3 array of chunks of 512, compressed with zstd. **Refused:** no pixels, 5 channels, names for some channels, a pixel of no size, a region outside, samples of the wrong type or number |
| `oscope-store` | `projects` (6) | **Projects** made, trimmed, tagged; listed the most recently changed first; searched in any case, with `%` and `_` plain characters; by whole tags; paged; renamed, retagged, their metadata set and removed; refused and not found; deleted. **Records** drafted, not listed until committed, their folder gone when dropped; an acquisition with its pyramid, thumbnail and document; results with their provenance and files, with or without an image; a report; the project changed by each commit; the thumbnails of the project and of each record. **The rebuilt index** equals the live one, row for row, into an empty index and in place, leaving out a draft and finishing a half-deleted project. **Eight writers at once,** each with its own store and connection, as other processes would have: 24 projects, 24 metadata keys in one project with none lost, 8 acquisitions, and the rebuilt index equal. **Thumbnails** of a dim 16-bit image stretched to white, and of an 8-bit one shown as it is. **A page** holds 500 projects at most |
| `oscope-api` | `projects` (2) | **Over HTTP:** made, with the `changed` event; listed, searched by text and tag, paged; renamed, its sample set, tagged, its metadata set and removed; an acquisition from another store on the same folders, in the project's contents and thumbnails; refusals (400) and unknowns (404), an ID that isn't one (400); deleted; the index rebuilt. **The TD** (a snapshot) |
| `ex-projects` | `--smoke` | Two projects listed, each with its thumbnail and its acquisition's store, read back and of NGFF 0.5; a search |

The page's `app.js` ran under the Node harness against the example: no error, two cards, `1–2 of 2`. `open_store.py` read both stores with zarr and ome-zarr-py.

**Where they ran:** {{REPLAYS}} On Windows, the `images`, `projects` (both) suites and `ex-projects --smoke` passed 5 times out of 5 each with their binaries pinned to two cores: 11.2–11.9 s for `images`, with the MIChrome 20's whole frame, and 5.3–6.1 s for the smoke.

**Mutation checks** (each broke the code on purpose, then the tests were run): seventeen of seventeen caught. Reading the tests against the list first found three gaps, now covered: no report in the rebuild's test, no check that a thumbnail is stretched, and no test of the page's limit.

| Broken on purpose | Caught by |
|---|---|
| Changes without the store's lock | `writers_at_once_lose_nothing` |
| A pyramid's mean always over four pixels, at odd edges too | `images_round_trip_at_every_level_for_each_sample_type` |
| Coarser levels without their translation | `the_metadata_is_ngff_0_5_s` |
| Fewer levels: halving while both sides are over a chunk | `a_full_michrome_20_frame_round_trips`, `the_metadata_is_ngff_0_5_s` |
| A region outside the image accepted | `images_that_do_not_hold_are_refused` |
| Images of more than 4 channels accepted | the same |
| Search's `%` and `_` taken as wildcards | `projects_are_made_changed_listed_searched_and_deleted` |
| Tags matched in part | the same |
| A page of any size | `a_page_holds_at_most_500_projects` |
| The schema's version not kept | `writers_at_once_lose_nothing` |
| A dropped draft's folder kept | `records_are_drafted_then_committed_with_their_thumbnails` |
| Half-deleted projects left by a rebuild | `a_rebuilt_index_equals_the_live_one` |
| Reports left out of a rebuild | the same |
| A project's cover from its results first | `records_are_drafted_then_committed_with_their_thumbnails` |
| Thumbnails of 16-bit images not stretched | `thumbnails_stretch_dim_images_and_show_8_bit_ones_as_they_are` |
| Not found answered as an internal error | `projects_are_made_listed_changed_and_deleted_over_http` |
| A record's thumbnail asked for by any path, not only an ID | `records_are_drafted_then_committed_with_their_thumbnails`, `projects_are_made_listed_changed_and_deleted_over_http` |

## Follow-ups and known gaps

- **Ingest and the scratch recorder** (P29) write acquisitions through drafts; **importers** (P30) read other tools' OME-Zarr.
- **Sharding,** if a slide's tens of thousands of chunk files are too many (P39, P40).
- **A wastebasket,** if deleting at once proves too final.
- **A `Draft` borrows its `Store`:** a job that writes over many calls may want an owned one.

## Clean room

Nothing was copied from `microscope` or any other codebase.

- **NGFF 0.5's metadata** follows the specification, <https://ngff.openmicroscopy.org/0.5/>, and is checked against its own schemas.
- **The pyramid** is the mean of 2 × 2 pixels, each level centred on the pixels it averages: the box filter, with no published method of OScope's own.
- **The example's luminance** uses ITU-R BT.709's weights (0.2126, 0.7152, 0.0722) on linear light.
