# ADR-0021: Projects, OME-Zarr v3 and the SQLite index

- Status: Accepted; extended by [ADR-0027](0027-post-acquisition-stitching.md), [ADR-0029](0029-volumes-and-3d-data-on-the-media-plane.md)
- Date: 2026-11-14
- Phase: P28

## Context

- **D16:** a project is one slide or experiment. It holds acquisitions (raw data), results (derived data, with provenance) and reports. The Gallery shows projects as cards.
- **D15:** raw data is OME-Zarr v3 (OME-NGFF), through the `zarrs` crate, so that napari and Python read it.
- **Plan §5.12** lays out the data root: a folder per project, with versioned JSON beside the data, and `index.sqlite`, rebuildable from the folders, which the job service already uses (ADR-0016).
- **Sizes:** a 15 × 15 mm slide at 20× is about 950 tiles, roughly 38 GB raw before compression. Deep-zoom tiles are 256 pixels (P40); stacks are read a plane at a time.
- **Dependencies** must be at least two weeks old and free of RustSec advisories (ADR-0002). `zarrs` 0.23 requires `paste`, which RustSec marks unmaintained (RUSTSEC-2024-0436). The `zstd` crates' fixes for three advisories (RUSTSEC-2026-0391, -0398 and -0403) were released on 2026-11-01.

## Decision

**1. The folders are the truth.**

```text
{data root}/projects/<project>/project.json
    acquisitions/<id>/acquisition.json, raw.ome.zarr/, thumbnail.jpg
    results/<id>/result.json, image.ome.zarr/ (an image result), files, thumbnail.jpg
    reports/<id>/report.json, files
{data root}/index.sqlite
```

- **IDs** are `oscope-core`'s version 7 UUIDs (`ProjectId`, `AcquisitionId`, `ResultId`, and a new `ReportId`), which name the folders, so that names can change freely.
- **Each record is a versioned document** (`oscope.project`, `oscope.acquisition`, `oscope.result`, `oscope.report`, version 1), written to a temporary file and renamed over the old, so that a reader finds one whole.
  - An acquisition keeps its image's shape, the instrument, the objective, the calibration used, the camera's settings, the plan, and each frame's position and timestamp.
  - A result keeps its provenance: the recipe as it ran, the nodes and their versions (an extension's `id@version` among them), its inputs, and the software.
- **A record is drafted, then committed.** Its folder and its image come first, and its document last, so that nothing lists a record until it is whole. A draft dropped without a commit removes its folder.
- **Changes hold a lock,** that of the file `projects/.lock`, through the standard library's file locks: stores in other processes, and other stores in this one, take turns. A read needs none.
- **Deleting a project** renames its folder to `.deleting-<id>` first, out of every listing, then removes it.

**2. Images are OME-Zarr v3 stores, through `zarrs` 0.22,** with only its `filesystem` and `zstd` features:

- **a Zarr group** whose attributes hold NGFF 0.5's `ome` metadata: one multiscale image of axes `c, z, y, x`; each level's `scale`, in micrometres when the pixel size is known, and a `translation` of (2^k − 1)/2 level-0 pixels at level k, so that every level's pixels are centred on those they average; `type: mean`; and `omero` channels with their labels, colours and ranges;
- **one array per level,** samples of `u8`, `u16` or `f32`, 1 to 4 channels;
- **chunks of every channel of 512 × 512 pixels of one plane:** a deep-zoom tile of 512, or 2 × 2 of 256; a stack read a plane at a time;
- **zstd at level 3,** without checksums;
- **level 0 written in regions,** as the caller has them; then each level the mean of 2 × 2 pixels of the one before (of fewer at an odd edge), until a level fits one chunk.

The reader opens a store of one multiscale image of axes `c, z, y, x`: OScope's own.

**3. The index** lives in `index.sqlite`, beside the job service's table:

- **Its tables are `store_projects` and `store_records`.** Each row holds the record's whole document beside the columns it is listed by, so that listing reads no folders.
- **Its schema's version is in `store_schema`.** The job service counts its own with `PRAGMA user_version`, so the two can't share it.
- **Pages** list projects, the most recently changed first, each with its counts of acquisitions, results and reports.
  - **Search:** a text, in any case, in names, samples and tags, where `%` and `_` are plain characters; and a tag, matched whole.
  - **Paging:** 50 to a page by default, 500 at most.
- **Rebuilding** reads every folder and replaces both tables at once. It leaves out records without their document (drafts), and finishes deleting `.deleting-` folders. A document that doesn't read is left out too, logged and reported, so that one damaged file can't keep the others out. A rebuilt index equals the live one, row for row.

**4. Thumbnails** are JPEGs of at most 256 pixels across, made when a record with an image is committed.

- **The level:** of those at most 1,024 pixels across, the one that gives the largest thumbnail in whole steps.
- **The stretch:** 8-bit images are shown as they are. Others are stretched from their 0.5th to their 99.5th percentile, through sRGB's curve, as a camera's linear data wants.
- **A project's thumbnail** is its newest acquisition's, the slide as it was taken, or, without one, its newest image result's.

**5. The `projects` Thing:**
- **endpoints:** `GET list?text=…&tag=…&offset=…&limit=…`, `project/{id}` (a project and its records), `thumbnail/{id}` and `thumbnail/{id}/{record}`;
- **actions:** `create`, `rename`, `set_sample`, `tag`, `set_metadata` (`null` removes a key), `delete`, and `rebuild_index`, which says how many projects the index then lists and which documents it left out;
- **the event `changed`,** with the project's ID.

The server opens the store at start-up; the printed documents use an index in memory, and make no folder.

**6. The `zstd` crates.** `zarrs` 0.22 takes `zstd` 0.13. Its fixed releases, `zstd` 0.13.4 and `zstd-safe` 7.4.0, are taken at 13 days old, with the owner's permission, and verified once two weeks old, on 2026-11-15.

## Consequences

- **Readable elsewhere:** zarr 3.4.2 and ome-zarr-py 0.21.0 open the stores, and see their levels, axes and scales; napari's plugin reads them through ome-zarr-py.
- **Robust to the index:** losing or damaging `index.sqlite` loses no data. `rebuild_index` makes it again, the job history apart.
- **About 94 crates more,** all from `zarrs` and `jsonschema` (tests only).
- **Many files:** a chunk is a file, so a 38 GB slide at level 0 makes tens of thousands. Zarr's sharding codec would group them; P39 and P40 will say whether that is needed.
- **One writer at a time:** commits and changes wait for the lock, which is held for milliseconds. A record's image is written without it.
- **Readers of other OME-Zarr:** only OScope's own layout is read for now. Importers (P30) bring others.

## Alternatives considered

- **`zarrs` 0.23,** the newest. It requires `paste`, which RustSec marks unmaintained, and the dependency policy refuses that without the owner's decision. 0.22 has every feature OScope uses.
- **A Zarr writer of OScope's own.** Zarr v3 with zstd alone is small. But reading other tools' stores, sharding and more codecs would follow, and the plan chose `zarrs` for them (D15).
- **SQLite as the truth.** One file to back up, but opaque, and one damaged file would lose every project. The folders carry their own documents.
- **The index in a file of its own.** Possible, but plan §5.12 puts projects and jobs in one `index.sqlite`. Separate tables and a separate version count keep them apart.
- **A project's thumbnail from its newest image, a result as likely as an acquisition.** A derived image, such as a grey map, shows a slide less well than the slide does.
- **Thumbnails from the smallest level.** Integer steps from a level of 257 to 512 pixels give thumbnails as small as 129.
