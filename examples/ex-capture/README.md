# ex-capture

Captures, downloads, importers and exporters (P30, [ADR-0037](../../docs/adr/0037-importers-exporters-and-the-abi-s-third-version.md)): a frame of the simulated camera captured into a project, with its display rendition; the raw frame exported as OME-TIFF and downloaded, whole and by a range; and images imported. All through the API, as a client would.

## How to try it

```sh
cargo run --release -p ex-capture
```

It serves on port 8234, starts the simulated camera, makes a project, `ex-capture`, then goes through the steps and prints what each made:

```text
ex-capture: open http://127.0.0.1:8234/
ex-capture: captured "field", 5472 × 3648 pixels of "bayer16", in project ex-capture
  its rendition: http://127.0.0.1:8234/api/v1/data/<project>/acquisitions/<acquisition>/rendition.png
  exported as OME-TIFF, 39.9 MB, downloaded to .oscope/ex-capture/data/downloads/field.ome.tif
  its first 4 bytes, by a range: "II*\0"
  imported the rendition: 5472 × 3648, RGB of "u8"
  imported the download: 5472 × 3648, grey of "u16"
  the project's files: http://127.0.0.1:8234/api/v1/projects/project/<project>
```

Open the rendition's link: the frame demosaiced and stretched for display. The server keeps serving until Ctrl-C, so you can go on with its API, at `http://127.0.0.1:8234/docs`:

- **The `capture` Thing:** `single` captures the camera's next frame, and `burst` its next 1 to 10,000, each into a new acquisition, as a job. `rendition: "png"` or `"jpeg"` adds the display rendition.
- **The formats:** `GET /api/v1/projects/formats` lists the importers (TIFF and its stacks, PNG, JPEG) and the exporters (OME-TIFF, 16-bit TIFF, PNG), and those of the extensions installed.
- **Exporting:** `POST /api/v1/jobs/submit` with `{"kind": "projects.export", "params": {"project", "record", "format"}}` writes the record's image to its `exports/` folder; the job's result is the file's data path.
- **Importing:** `{"kind": "projects.import", "params": {"project", "path", "name"}}` reads a file on the server into a new acquisition, with the importer its name asks for, or the one `format` names.
- **Downloads:** `GET /api/v1/data/{project}/{path}` serves any file of the project, `Range: bytes=…` too: exports, renditions, thumbnails, documents, and the OME-Zarr stores' files.

**`--smoke`** does the same in a temporary folder, with the camera reading out the sensor's middle 960 × 720, checks what each step made, and exits.

## OME-TIFF

The export is OME-TIFF: a page for each plane, its samples as they are, and in the first page's description the OME-XML (2016-06) of its size, sample type, planes, pixel size and channels' names. Over 4 GB it is BigTIFF. `tifffile` 2026.10.15 reads it as an OME series: a burst of three frames is `(3, 3648, 5472)`, on `Z`, `Y` and `X`.
