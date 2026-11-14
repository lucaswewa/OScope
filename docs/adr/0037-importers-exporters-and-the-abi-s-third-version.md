# ADR-0037: Importers, exporters, and the ABI's third version

- Status: Accepted
- Date: 2026-11-14
- Phase: P30

## Context

- **D11:** extensions add importers and exporters, among other things. P30 brings data in and out: images (TIFF, PNG, JPEG) and multi-page TIFF stacks in; OME-TIFF, 16-bit TIFF and PNG out; and the same kinds from extensions, in Rust and in Python.
- **Plan §4.3** routes downloads through `GET /api/v1/data/{project}/{path}`, with Range requests. The `capture` Thing makes single captures and bursts, as jobs.
- **ABI 2** (ADR-0019, ADR-0036) carries nodes, and drivers as a JSON method protocol with a camera's capture into OScope's buffer. Its layouts are the ABI, so any change to it is a new version.
- **Python workers** (ADR-0020, ADR-0036) answer messages, and pass frames through shared memory.
- **No planned ADR covers this.** This one takes the next free number above ADR-0036's, 0037.

## Decision

**1. Importers and exporters are traits of `oscope-store`:**

- **an importer** names its format (an ID, a name, its files' extensions), says what a file holds (`probe`: an image description of its size, samples, channels and planes) and reads each plane (`read`);
- **an exporter** names its format, and writes an image to a file, asking for each plane in turn.

OScope's own are TIFF, a page or a stack of them, PNG and JPEG in; and OME-TIFF, 16-bit TIFF and PNG out. The `projects` Thing lists them all, OScope's first, at `GET /api/v1/projects/formats`.

**2. Imports and exports:**

- **An import** reads a file into a new acquisition of a project, a plane for each page, keeping its samples' type and channels. The acquisition's record says what it was imported from.
- **An export** writes a record's image to its folder's `exports/`, named as the record. It writes through a hidden part file, renamed when whole.
- **Both are jobs,** `projects.import` and `projects.export`. An import's importer is the one named, or else the first whose extensions its file's name has. An import's path is a file on the server: who may submit jobs may read images the service can.

**3. OME-TIFF:**

- **Pages:** a page for each plane, its samples as they are (8 or 16 bits, or 32-bit floats), and the channels interleaved: 1, 3 or 4. TIFF holds no grey-and-alpha, so 2 channels are refused.
- **The first page's description** holds the OME-XML (2016-06): `DimensionOrder="XYCZT"`, the sample type, the sizes, the planes on `Z`, the pixel size in µm (the schema's unit when none is given), and one channel of all the samples, named with the channels' names.
- **ASCII:** TIFF's text is ASCII, so other characters in names are XML references.
- **Over 4 GB,** it is BigTIFF.

OScope's importer reads the pixel size and names back. 16-bit TIFF is the same pages in 16 bits, without the OME-XML. PNG is the first plane, in 8 bits or 16.

**4. The ABI's third version.** It adds two functions to the extension's table:

- **`read`:** what `params`, JSON, ask of a driver, into OScope's buffer;
- **`write`:** OScope's buffer given to a driver, with `params`.

Importers and exporters are two more kinds of driver, in the description beside cameras, stages and lights, and in the manifest's `provides.importers` and `provides.exporters`.

| Kind | JSON methods | Pixels |
|---|---|---|
| importer | `format` (`{"name", "extensions"}`), `probe({"path"})` (the image description) | `read({"path", "plane"})`: a plane's samples, interleaved and little-endian |
| exporter | `format`, `begin({"path", "image", "name"})`, `end` | `write({"plane"})`: a plane's samples, interleaved and little-endian |

An importer's or exporter's format ID is its name. An exporter writes one image at a time.

**5. Python** speaks the same methods. The worker takes two more messages, `read` and `write`, each with a shared-memory segment: the importer's plane into it, or OScope's plane from it, as a flat array of bytes.

**6. The data route,** `GET /api/v1/data/{project}/{path}`:

- **What it serves:** any file of the project, among them exports, renditions, thumbnails, documents and the OME-Zarr stores' files, which a Zarr reader can open over HTTP;
- **Ranges:** one byte range (`a-b`, `a-` or `-n`, RFC 9110 §14), answered with 206 and `Content-Range`; a range beyond the file with 416. Several ranges, or a form it doesn't know, are ignored, and the whole file is sent;
- **Never outside the project:** each part of the path must be a plain name, not hidden, without `\` or `:`;
- **Content types** by the file's extension, `nosniff`, and the file streamed in chunks of 1 MiB.

**7. Captures** are jobs, `capture.single` and `capture.burst` (1 to 10,000 frames):

- **The frames:** the recorder takes those whose exposure starts after the job does, and stops by itself at the count (ADR-0022);
- **Ingest:** the recording is ingested into a new acquisition, then deleted;
- **The rendition:** the first plane at full resolution, demosaiced bilinearly if the camera gave a Bayer mosaic, and stretched as its thumbnail is, as PNG or JPEG, `rendition.png` or `.jpg` in the acquisition's folder;
- **Waiting:** a capture waits 10 s at most for a frame.

## Consequences

- **Every Rust extension builds again** for ABI 3. ABI 2's libraries are refused, saying so.
- **Readers elsewhere:** `tifffile` reads the OME-TIFF exports as OME series, with their planes on `Z`.
- **Interleaved channels** are one OME channel of several samples: the channels' names are joined with ", " in its name.
- **An import holds a plane in memory** at a time, and an export likewise.

## Alternatives considered

- **Importers and exporters through files only,** OScope writing a standard file for the extension. It doubles the I/O, and an exporter couldn't write planes as they come.
- **Reusing `capture` for an importer's planes.** A capture takes no parameters, so the plane and the file would be state set by earlier calls, and a race between callers.
- **The OME-XML in Unicode.** TIFF's ASCII type refuses it, and readers expect ASCII: references keep any name.
- **A planar OME-TIFF,** each channel its own page. Interleaved keeps RGB as other tools write it, and a plane a page.
- **`tower-http`'s file serving.** It isn't in the lock. Single ranges are a page of code.
