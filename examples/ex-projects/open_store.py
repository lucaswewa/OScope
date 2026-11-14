"""Opens an OScope OME-Zarr store with zarr and with ome-zarr-py, and says
what each sees: run it with uv, as the README says.

    uv run --no-project --with "zarr>=3" --with ome-zarr python open_store.py STORE
"""

import sys

import zarr
from ome_zarr.io import parse_url
from ome_zarr.reader import Reader

path = sys.argv[1]

# zarr: the group, its NGFF metadata, and each level's array.
group = zarr.open_group(path, mode="r")
ome = group.attrs["ome"]
multiscale = ome["multiscales"][0]
print(f"zarr {zarr.__version__}: NGFF {ome['version']}, axes {[a['name'] for a in multiscale['axes']]}")
for dataset in multiscale["datasets"]:
    array = group[dataset["path"]]
    scale = dataset["coordinateTransformations"][0]["scale"]
    print(f"  level {dataset['path']}: {array.shape} {array.dtype}, chunks {array.chunks}, scale {scale}")
level0 = group[multiscale["datasets"][0]["path"]]
middle = level0[:, 0, level0.shape[2] // 2, level0.shape[3] // 2]
print(f"  the middle pixel of level 0: {list(map(float, middle))}")

# ome-zarr-py: the image node napari's plugin reads.
nodes = list(Reader(parse_url(path))())
image = nodes[0]
print(f"ome-zarr: {len(image.data)} levels, the largest {tuple(image.data[0].shape)}, "
      f"axes {[a['name'] for a in image.metadata['axes']]}")
