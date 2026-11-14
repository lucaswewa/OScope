# ex-record

Recording at the design rate (P29, [ADR-0022](../../docs/adr/0022-scratch-recording-and-ingest.md)): the simulated MIChrome 20's frames, 16 bits at 14.5 fps, 580 MB/s, written to the scratch disk as the camera gives them; how fast, and whether any were dropped; then the recording ingested into a project as OME-Zarr, by a job.

## How to try it

Build it in release: a debug build can't render or ingest at this rate.

```sh
cargo run --release -p ex-record --features cuda
```

It starts the simulated camera over the H&E slide at 10×, rendering on the GPU, and waits for its first frames: the first takes a few seconds to render. Then it records for 10 s into `.oscope/ex-record/scratch/recordings/`, saying each second how far it has got, and ingests the recording into a new project, `ex-record`:

```text
The scratch space, .oscope\ex-record\scratch: 231 GB free (Ok).
Recording the simulated MIChrome 20, rendering on the GPU: frames of 39.9 MB at 14.5 fps, 579 MB/s, for 10 s.
    1.1 s      15 frames     0.60 GB   568 MB/s   dropped 0   queue at most 1 of 16
    …
   10.0 s     145 frames     5.79 GB   576 MB/s   dropped 0   queue at most 1 of 16
Recorded 146 frames, 5.83 GB, at 576 MB/s; 0 dropped.
  ingesting: 8 of 146 frames
  …
  ingesting: the pyramid and the thumbnail
Ingested in 18.6 s, 313 MB/s: 146 planes of 5472 × 3648 in 5 levels, 3.87 GB compressed from 5.83 GB.
The acquisition: .oscope\ex-record\data\projects\<project>\acquisitions\<acquisition>\raw.ome.zarr
```

- **dropped** counts the frames the camera lost between those recorded: none, if the recorder kept up. **queue at most** says how many frames waited at once to be written, of the 16 the recorder holds. When they fill, the camera waits, and loses the frames whose time passes meanwhile.
- **Without `--features cuda`,** or without an NVIDIA GPU, the simulator renders on the CPU, at about 10 frames a second, and drops the rest; the example says so. `--test-sensor` records without the renderer.
- **The ingest** makes an acquisition with a plane for each frame, compressed with zstd, and deletes the recording. The acquisition opens in Python as [`ex-projects`' do](../ex-projects/README.md#opening-a-store-in-python): zarr 3.4.2 and ome-zarr-py 0.21.0 see its five levels of 146 planes. Its scale is in pixels until P32 calibrates the objectives.

## Options

- **`--seconds N`:** records for N seconds, 10 by default.
- **`--test-sensor`:** records the fake camera's moving stripes instead, in the MIChrome 20's frames, at exactly the design rate: the recorder alone, without the renderer.
- **`--keep-gb N`:** rolls, deleting the oldest segments to stay within N GB. Such a recording has lost its oldest frames by design, so it isn't ingested: delete it from the scratch folder when you are done.
- **`--scratch FOLDER`:** puts the scratch space on another disk, a fast one.
- **`--smoke`:** records 1.5 s of the test sensor at a quarter of the size, ingests it, checks every frame is there, the first as the sensor drew it, and exits.

## The 10-minute run

§5.20's budget is 580 MB/s for 10 minutes with no dropped frames. This writes 348 GB, keeping at most 100 GB on the disk, and reports every 10 s:

```sh
cargo run --release -p ex-record --features cuda -- --seconds 600 --keep-gb 100
```

P29's notes give the owner's PC's result. Delete the recording from `.oscope/ex-record/scratch/recordings/` afterwards.

## What it writes

```text
.oscope/ex-record/scratch/recordings/<recording>/recording.json, 000000.frames, 000001.frames, …
.oscope/ex-record/data/projects/<project>/acquisitions/<acquisition>/raw.ome.zarr
```

A segment holds 4 GiB, about 100 of the MIChrome 20's frames: each frame's metadata and pixels, with a CRC-32, and an index at the end. A recording that a crash cut short is recovered the next time the scratch space opens.
