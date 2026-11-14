# ex-projects

Projects on disk (P28, [ADR-0021](../../docs/adr/0021-projects-ome-zarr-and-the-index.md)): two projects, each with an acquisition stored as OME-Zarr, a Gallery-style page of their cards, and how to open the stores in napari or Python.

## How to try it

```sh
cargo run -p ex-projects
```

On its first run, it makes two projects in `.oscope/ex-projects/data/projects/`. Each holds an acquisition: a 2048 × 1536 field of the simulated H&E slide, through the 10× objective for the first, and the 20× for the second. The first also holds a result made from it, its luminance, with its provenance. Later runs keep them.

```text
ex-projects: made "H&E section 1", with Island 1, 10x of 2048 × 1536 pixels of 0.240 µm in 3 levels
ex-projects: made "H&E section 2", with Island 1, 20x of 2048 × 1536 pixels of 0.120 µm in 3 levels
ex-projects: open http://127.0.0.1:8233/
```

Open `http://127.0.0.1:8233/`: a card for each project, with its thumbnail, sample, tags and counts.

- **Search** names, samples and tags in the box, or click a tag to show only projects with it.
- **Click a card** for its contents and where each store is, then rename or delete the project.
- **New project** makes an empty one.

The page is a client of the `projects` Thing: `GET /api/v1/projects/list?text=…&tag=…&offset=…&limit=…`, `project/{id}` and `thumbnail/{id}`, and its actions `create`, `rename`, `set_sample`, `tag`, `set_metadata`, `delete` and `rebuild_index`.

**`--width N`** renders fields N pixels wide. **`--smoke`** makes the projects in a temporary folder, at 640 × 480, checks each one's listing, thumbnail and store, and exits.

## Each project's folder

```text
projects/<project>/project.json
projects/<project>/acquisitions/<id>/acquisition.json, raw.ome.zarr/, thumbnail.jpg
projects/<project>/results/<id>/result.json, image.ome.zarr/, thumbnail.jpg
```

Each store is a Zarr v3 group with NGFF 0.5's metadata. Its arrays are `c, z, y, x`, in chunks of 512 × 512 pixels, compressed with zstd. Level 0 is the full resolution, and each next level halves it, down to a level that fits one chunk. `index.sqlite` lists everything, and the `rebuild_index` action makes it again from the folders.

## Opening a store in Python

[`open_store.py`](open_store.py) opens a store with `zarr` and with `ome-zarr-py`, and prints what each sees. With [uv](https://docs.astral.sh/uv/):

```sh
uv run --no-project --python 3.13 --with "zarr>=3" --with ome-zarr \
    python examples/ex-projects/open_store.py \
    .oscope/ex-projects/data/projects/<project>/acquisitions/<id>/raw.ome.zarr
```

```text
zarr 3.4.2: NGFF 0.5, axes ['c', 'z', 'y', 'x']
  level 0: (3, 1, 1536, 2048) uint16, chunks (3, 1, 512, 512), scale [1.0, 1.0, 0.24, 0.24]
  level 1: (3, 1, 768, 1024) uint16, chunks (3, 1, 512, 512), scale [1.0, 1.0, 0.48, 0.48]
  level 2: (3, 1, 384, 512) uint16, chunks (3, 1, 512, 512), scale [1.0, 1.0, 0.96, 0.96]
  the middle pixel of level 0: [49093.0, 27298.0, 50284.0]
ome-zarr: 3 levels, the largest (3, 1, 1536, 2048), axes ['c', 'z', 'y', 'x']
```

`--python 3.13` matters: uv otherwise picks its newest CPython, for which `numcodecs` may have no wheel yet.

## Opening a store in napari

napari reads OME-Zarr through the `napari-ome-zarr` plugin:

```sh
uv run --no-project --python 3.13 --with "napari[all]" --with napari-ome-zarr \
    napari --plugin napari-ome-zarr \
    .oscope/ex-projects/data/projects/<project>/acquisitions/<id>/raw.ome.zarr
```

The store's metadata gives the plugin each channel's name, colour and range, and the pixels' size in micrometres. napari needs a display, so this command isn't among the example's checks.
