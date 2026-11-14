# P30: Capture, downloads, importers and exporters

- Status: In review
- Branch / PR: `m1/p30-capture-io` / [#44](https://github.com/lucaswewa/OScope/pull/44)
- Size: M · estimated ~4,200 before it started (the `capture` Thing, its jobs and the rendition ~400; the data route ~200; importers ~300; exporters ~350; importer and exporter extension kinds, with the ABI's third version, ~800; tests ~1,000; `ex-capture` and the samples ~450; and a fifth on top); one pull request, at the owner's standing choice for P24–P34 · measured **3,783** hand-written lines added (code 2,246: `oscope-store` 894, `oscope-api` 572, `oscope-ext-sdk` 255, `oscope-ext` 255, `oscope-acquire` 204, the Python SDK 52, `oscope-shm` 14; tests 941; examples 564; glue 32), and 126 removed. **That is 3,183 over M's 600, and about 10 % under the estimate.**

## Summary

Single captures, getting data out, and bringing data in, extensions' formats too ([spec](../phases.md#p30); D11; plan §4.3; [ADR-0037](../../adr/0037-importers-exporters-and-the-abi-s-third-version.md)).

- **The `capture` Thing:** a single raw frame, or a burst of 1 to 10,000, into a new acquisition, as a job, with the frame's display rendition, PNG or JPEG, if asked.
- **The data route,** `GET /api/v1/data/{project}/{path}`: a project's files, with Range requests and content types, never outside the project.
- **Importers** of TIFF, a page or a stack, PNG and JPEG; **exporters** of OME-TIFF, with its OME-XML, 16-bit TIFF and PNG; as the `projects.import` and `projects.export` jobs, with their formats listed.
- **Importer and exporter extension kinds,** in Rust, the ABI's third version, and in Python; the samples' PGM (Rust) and `.npy` (Python).
- **The recorder** takes a count of frames, exposed from a moment on: a burst.
- **P29 is marked Done.**

## What was built

| Where | What |
|---|---|
| `oscope-store`, `formats` | The `Importer` and `Exporter` traits, `Format`; OScope's own importers (TIFF, PNG, JPEG) and exporters (OME-TIFF, 16-bit TIFF, PNG); `import`, `export` and `render`, the rendition |
| `oscope-store` | `Samples::from_le_bytes` and `to_le_bytes`, `SampleType::bytes`, a `Debug` for `Store`; the thumbnail's stretch shared with the rendition |
| `oscope-acquire` | `capture_jobs` (`capture.single`, `capture.burst`), `Rig`; the recorder's `Take` and `finished` |
| `oscope-api` | The `capture` Thing; the data route; the `projects.import` and `projects.export` jobs, and `GET projects/formats` |
| `oscope-ext-sdk` | ABI 3: `read` and `write`; the `Importer` and `Exporter` traits, `Format`, `ImageDescription`, `Sample`, `PixelSize`; `importer()` and `exporter()` |
| `oscope-ext` | Importers and exporters loaded from both kinds of extension, as `oscope-store`'s, and checked against the manifest; the library's and the worker's `read` and `write` |
| `oscope-shm` | `Segment::write_bytes` |
| Python SDK | `@driver` of kinds `importer` and `exporter`; the worker's `read` and `write` messages |
| `examples/ext-rust-node` | A PGM importer and exporter |
| `examples/ext-python-node` | A `.npy` importer and exporter |
| `examples/ex-capture` | A capture, its rendition, an OME-TIFF downloaded, and imports, through the API |

## How to try it

[`ex-capture`](../../../examples/ex-capture/README.md):

```sh
cargo run --release -p ex-capture
```

It captures a frame of the simulated camera into a project, with its rendition, exports it as OME-TIFF, downloads it whole and by a range, imports the rendition and the download back, and prints the links. [`ex-rust-ext`](../../../examples/ex-rust-ext/README.md)'s test takes the Rust sample's PGM importer and exporter through the jobs.

## Performance

No §5.20 budget is P30's. On the owner's PC, `ex-capture`'s tour went through with a full MIChrome 20 frame in a debug build. Its smoke, of the sensor's middle 960 × 720, takes 8 s on two cores. The OME-TIFF of a full frame is 39.9 MB.

**Readers elsewhere:** `tifffile` 2026.10.15 reads the OME-TIFF exports as OME series: a frame `(3648, 5472)` on `Y` and `X`, and a burst of three `(3, 3648, 5472)` on `Z`, `Y` and `X`, with the OME-XML's sizes and types as written.

## Design notes and deviations from the plan

**Worth your review:**

- **One pull request,** as you decided for P24–P34. It came in about 10 % under the estimate, and 3,183 lines over M's 600. The extension kinds, across both SDKs and the host, took about 560 lines of code, less than their ~800; the tests are a quarter.
- **ABI 3:** every Rust extension builds again. A library of ABI 2 is refused, saying which version it was built for.
- **An import reads a file on the server,** at the path its job names: whoever may submit jobs may read the images the service can. An upload route, for files from the client, is the M2 app's (a follow-up).
- **A capture is a hardware job:** it holds the hardware lock while it takes its frames, as scans will.
- **Interleaved channels are one OME channel** of several samples, as RGB is usually written; their names are joined with ", " in its name. TIFF has no grey-and-alpha, so an image of 2 channels exports to PNG only.
- **The data route ignores several ranges** in one request, and sends the whole file, as RFC 9110 allows. HTML and other types it doesn't know are served as bytes, with `nosniff`.
- **The `zstd` crates P28 took early** turn two weeks old at 14:12 UTC on 2026-11-15, after this pull request: the notes of the phase in progress then record their check.

**Other choices:**

- **Formats' IDs:** OScope's are `tiff`, `png` and `jpeg` in, and `ome-tiff`, `tiff16` and `png` out. An extension's is its importer's or exporter's name.
- **An import's importer:** the one named, else the first whose extensions the file's name has, OScope's first.
- **Exports** are named as their record, safely, in its folder's `exports/`, written through a hidden part file; exporting again replaces the file.
- **PNG exports the first plane;** OME-TIFF and 16-bit TIFF every plane. 16-bit TIFF scales 8 bits by 257, and floats from 0–1.
- **The rendition** is the first plane, demosaiced bilinearly from a Bayer mosaic, and stretched as the thumbnail is.
- **Captures' recordings** use segments of 1 GiB, not the default 4 GiB.

## Tests

| Crate | Test | What it checks |
|---|---|---|
| `oscope-store` | `formats` (5) | **Every format round-trips,** export then import, across 8 bits, 16 bits and floats, 1 to 4 channels and stacks of 3: OME-TIFF every plane as it was, with its pixel size and channels' names; 16-bit TIFF every plane in 16 bits; PNG the first plane; 2 channels refused by TIFF. **OME-TIFF's metadata:** the schema, the name, the order, type and sizes, the pixel size, an escaped name, the planes, the description in the first page alone. **Imports** of a PNG as it was, a JPEG near it, and a 16-bit stack of four pages; refused: pages of two sizes, a broken file. **Exports refused:** an ID that isn't one, a missing record, a report, a result without an image. **A Bayer capture's rendition** demosaiced: red where the mosaic's red sites are bright |
| `oscope-api` | `capture` (1) | **Over HTTP:** the camera streaming, a single capture with its rendition downloaded as a PNG, a burst of five, the formats, an OME-TIFF export downloaded whole and by three ranges, 416 beyond the end, imported back; the captures' recordings gone from the scratch; an importer named that can't read the file; the data route's refusals (400, 400, 404, 404); bursts of 0 and 10,001 frames refused |
| `oscope-api` | `data` (2, unit) | Ranges as RFC 9110 has them; paths kept in the project |
| `oscope-api` | `devices` | The `capture` Thing's TD (a snapshot) |
| `oscope-acquire` | `recorder` (1 new) | A burst takes the frames exposed after it starts, up to its count, then stops by itself |
| `oscope-ext` | `python` (1 new) | **A Python importer and exporter** take a 16-bit RGB stack of two planes through shared memory, and back; a missing file's error says which importer |
| `oscope-ext`, `oscope-ext-sdk` | `native`, `sdk` | ABI 3's layouts, and its version in the refusals |
| `ex-rust-ext` | `server` | **The Rust sample's PGM importer and exporter** listed after OScope's, and a 16-bit PGM in and out again through the jobs; a manifest without its importer refused |
| `ex-capture` | `--smoke` | The tour, of the sensor's middle 960 × 720 |

**Where they ran:** on Windows, and on Linux under WSL, both with uv, in the local replays (`scripts/ci-local.sh` on Windows, and `scripts/ci-clone.sh` on a fresh clone under WSL, each run alone: 40 of 40 steps each, 393 tests in 156 suites, the Python tests real). On Windows, `oscope-store`'s `formats`, `oscope-acquire`'s `recorder`, `oscope-api`'s `capture`, `oscope-ext`'s `python` (10 times), `ex-rust-ext`'s `server` and `ex-capture --smoke` passed 5 times out of 5 each, with their binaries pinned to two cores. The first pinned runs of `python` failed once in 5, in P26's test of a worker killed mid-call. It slept 500 ms before killing the worker, and asserted that `taskkill /T` succeeded, which a child process ending meanwhile can deny. It now waits for the call to reach the worker, which `test.sleep` marks with a file, and then for the worker to be gone.

**Mutation checks** (each broke the code on purpose, then the tests were run): twenty-one of twenty-one caught. Reading the tests against the list first found four gaps, now covered: a rendition left a mosaic, a capture's recording left in the scratch, an importer named and ignored, and a manifest without its importer.

| Broken on purpose | Caught by |
|---|---|
| OME-XML not written | `ome_tiff_s_metadata_says_what_the_image_is`, and the round trips |
| Pixel sizes not read back | `every_format_round_trips_across_samples_channels_and_stacks` |
| 16-bit PNG read little-endian | the same |
| 8 bits not scaled to 16 | the same |
| Record IDs not checked | `exports_refuse_what_they_cannot_find` |
| A stack's pages of other sizes accepted | `png_jpeg_and_tiff_stacks_import` |
| Names not escaped into ASCII | `ome_tiff_s_metadata_says_what_the_image_is` |
| Renditions not demosaiced | `a_bayer_capture_s_rendition_is_demosaiced` |
| A range's last byte left out | `ranges_are_read_as_rfc_9110_has_them` |
| Hidden files served | `paths_stay_in_the_project` |
| No `Content-Range` | `captures_exports_downloads_and_imports` |
| A burst not stopped at its count | the same |
| Renditions not made | the same |
| A capture's recording kept | the same |
| The format named ignored | the same |
| An exporter's `end` not asked | `a_start_loads_the_sample_and_live_recipes_can_use_its_node` |
| Manifests' importers not checked | `a_library_whose_nodes_are_not_its_manifest_s_does_not_load` |
| An exporter given no pixels (Rust) | `a_start_loads_the_sample_and_live_recipes_can_use_its_node` |
| An exporter given no pixels (Python) | `an_importer_and_an_exporter_in_python_take_a_stack_through_shared_memory` |
| Frames from before a burst taken | `a_burst_takes_its_frames_exposed_after_it_starts_then_stops` |
| A recorder that doesn't stop at its count | the same |

## Follow-ups and known gaps

- **Uploads,** for imports of the client's files (M2).
- **Other tools' OME-TIFF:** several series, planar channels, tiles and pyramids. OScope reads its own, and TIFF's pages.
- **BigTIFF** is chosen above 4 GB, and untested at that size.
- **The capture job's start** comes from the instrument's clock: the recorder's filter by it is tested, the job's use of it is not.
- **Several ranges** in one request, if a client needs them.
- **The rendition with the live display settings,** white balance among them (P35's settings).

## Clean room

Nothing was copied from `microscope` or any other codebase. The formats follow their specifications: TIFF 6.0 and BigTIFF through the `tiff` crate; the OME-XML 2016-06 schema of the Open Microscopy Environment; PNG through the `png` crate and JPEG through `zune-jpeg`; Netpbm's PGM; NumPy's `.npy`; and byte ranges as RFC 9110 §14 defines them. The rendition's bilinear demosaicing is P08's.
