# ex-checkpoint-c

Checkpoint C (P34): the simulated instrument taken from a rough focus to a captured, calibrated frame, all through the API.

1. **Calibrate the 10×:** the objective check, then the geometry (P32): the pixel size and the camera-to-stage matrix.
2. **Autofocus** (P34): from 4 µm off, as a focus by hand leaves it.
3. **Set the exposure,** over the tissue; then **the white balance,** over clear glass beside the coverslip, the gains given to the preview's look. The stage goes back to the tissue from below, as autofocus left it.
4. **Capture into a project** (P30): a frame, stored as OME-Zarr in a new project, with its PNG rendition.

It prints each step, and how to open the capture in napari.

## How to try it

```sh
cargo run --release -p ex-checkpoint-c
```

It serves at http://127.0.0.1:8238/, whose page links to the calibration, the autofocus and exposure reports, and the projects. It looks through a 1024 × 768 region of the 20 MP sensor's middle, as the simulator renders each new depth of a frame afresh, which takes it seconds for the whole sensor. On the owner's PC, at real time, it printed:

```text
1. Calibrate the 10×'s geometry
   the objective check: plausible; the pixel size 0.24000 µm, turned 0.00°
2. Autofocus
   z = 5001.890 µm, -0.110 µm from the truth (a quarter of the depth of field is 1.97 µm), in 9.3 s
3. Set the exposure, and the white balance over clear glass
   the exposure 17.776779403899873 ms; the white balance's gains [0.9999772560286464,1.0,0.9999580622024395], given to the preview
4. Capture into a project
   stored at .oscope\ex-checkpoint-c\data\projects\01a1ddec-6c2f-7434-9129-2cebe68e53a7\acquisitions\01a1ddec-6e7c-75c0-9215-9543b72306d9\raw.ome.zarr

To open it in napari, with uv:

uv run --no-project --python 3.13 --with "napari[all]" --with napari-ome-zarr \
    napari --plugin napari-ome-zarr ".oscope\ex-checkpoint-c\data\projects\01a1ddec-6c2f-7434-9129-2cebe68e53a7\acquisitions\01a1ddec-6e7c-75c0-9215-9543b72306d9\raw.ome.zarr"
```

## Opening the capture in napari

napari reads OME-Zarr through the `napari-ome-zarr` plugin. With uv, the command the example prints opens it:

```sh
uv run --no-project --python 3.13 --with "napari[all]" --with napari-ome-zarr \
    napari --plugin napari-ome-zarr \
    .oscope/ex-checkpoint-c/data/projects/<project>/acquisitions/<id>/raw.ome.zarr
```

The store's metadata gives napari the pixels' size in micrometres. napari needs a display, so this isn't among the example's checks; the owner's manual test opens it.

## `--smoke`

```sh
cargo run -p ex-checkpoint-c -- --smoke
```

It runs the same on the quarter sensor at ten times real time, through a 768 × 576 region, with a temporary data root. It holds each step to its phase's tests: the objective plausible, a geometry in use, the focus within a quarter of the depth of field of the truth, the exposure converged, and the capture a store. It ends with `smoke: ok`, or says what failed and exits with code 1.
