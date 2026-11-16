# ADR-0029: Volumes and 3D data on the media plane

- Status: Accepted
- Date: 2026-11-16
- Phase: P41

## Context

- **Plan §5.16 and D8 want volumes:** z-stacks stored as multiscale OME-Zarr with their real, anisotropic voxel size, and served as metadata, orthogonal slices, and bricks at a level that fits the client's GPU, about 512³ by default, for VTK.js to draw.
- **What already exists:**
  - ADR-0021's images: OME-Zarr v3 of axes `c, z, y, x`, in chunks of 512 × 512 pixels of one plane, with a pyramid that halves x and y, never z.
  - ADR-0022's ingest: a recording's frames become an acquisition's planes, on `z`.
  - ADR-0025's z-scan: a stack at one place. Its plan, with the frame's corners on the stage and the stack's step, comes with the recording to the acquisition.
- **A colour camera's frames are kept raw,** their Bayer mosaic (ADR-0028). Level 0 holds the pattern; level 1's pixels are its 2 × 2 cells' means.
- **Stacks are anisotropic:** at 50× a pixel is 0.19 µm, and a stack's step is 0.5 µm or more. A pyramid that halves only x and y soon has voxels much wider than they are deep, every coarse level keeping every plane.

## Decision

**1. A volume is an image whose planes are depths,** `ImageInfo::volume`, recorded in its OME metadata as `"volume": true`. Its voxel is its pixel size, x, y and z. Other images, whose planes are times or tiles, are left as they were.

**2. A volume's pyramid halves z too, as its voxels near a cube:**

- **The rule:** level k halves z once more when the z it has, the step × 2^h after h halvings, is less than its pixel, the pixel × 2^k, and more than one plane is left. A volume of unknown voxels halves z at every level.
- **The means:** a voxel is then the mean of the 2 × 2 × 2 below it, or of 2 × 2 × 1 for the last plane of an odd count.
- **The OME metadata** gives each level's z scale, the step × 2^h, and its translation, the step × (2^h − 1)/2, as ADR-0021 does for x and y.
- **Reads** outside a level's planes or pixels are refused.

**3. A z-scan's frames are a volume.** The ingest takes its voxel from the plan: the frame's top edge on the stage, corner to corner, over its pixels, and the stack's step.

**4. The routes,** under `/api/v1/volume/{project}/{kind}/{id}/`, `kind` being `acquisitions` or `results`:

- **`meta`:** the size, channels, samples and voxel; whether it is a volume, and raw; and each level served, with its size and voxel.
- **`slice?axis=z|y|x&index=&level=&format=`:** an orthogonal slice through a level, its index in that level's voxels, at the finest level served by default.
- **`brick?x=&y=&z=&w=&h=&d=&level=&max=&format=`:** a box given in full-size voxels, the whole volume by default.
  - **Its level:** `level`, or else the finest level whose part of the box holds `max` voxels or fewer, 512³ by default, or the coarsest if none does.
  - **Its part of a level** runs from the level's voxel holding its first voxel to the one past its last.

**5. Slices and bricks are binary:** a header of 64 bytes, then the voxels, x fastest, then y, then z, a voxel's channels together, little-endian.

| Bytes | What |
|---|---|
| 0–3 | `OSV1` |
| 4–5 | the header's length, 64 |
| 6 | the samples: 1 for `uint8`, 2 for `uint16` |
| 7 | the channels |
| 8–11 | the level |
| 12–23 | the size in voxels: x, y, z, as `u32` |
| 24–35 | the origin in the level's voxels: x, y, z, as `u32` |
| 36–47 | the voxel's size in µm: x, y, z, as `f32`, 0 if not known |
| 48–63 | 0 |

- **`uint8` by default,** or `uint16` with `format=u16`.
- **Samples are scaled to fit:** 16 bits to 8 by v × 255 / 65,535, rounded; floats clamped from 0 to 1.

**6. A raw image is served from level 1,** each voxel its cell's mean; level 0 is refused.

- **Its pyramid always has level 1,** however small: a raw image that fits one chunk now has two levels, where ADR-0021 gave it one.
- **A raw image written before,** that fits one chunk, has only level 0, and is served from it, its mosaic as it is.

**7. VTK.js 38.0.4 is vendored** for the examples' pages, as OpenSeadragon is (BSD-3-Clause). `ex-volume` draws a brick with it, as a maximum intensity projection or composited.

## Consequences

- **The z-scan in `ex-volume`:** 21 planes of 1,368 × 912 pixels through the simulated cells, 0.19 µm across and 0.5 µm apart. The pyramid keeps them all at level 1, of 0.38 µm pixels, and halves them to 11 at level 2, of 0.77 µm. The page's brick, level 1's 6.5 million voxels, is 6.6 MB.
- **Old images are read as before:** no image was a volume until now, and an image without the flag isn't one.
- **A slice across y or x** reads a row or a column of every plane, a chunk per plane; a brick reads the chunks its part touches, plane by plane.
- **A brick is made whole in memory,** up to 512³ voxels, 128 MiB in 8 bits or 256 MiB in 16, unless the client asks for fewer.
- **A raw volume is grey:** its cells' means mix red, green and blue. A volume in its colours would be demosaiced plane by plane; none is needed yet.
- **What the page draws** is VTK.js's own rendering on WebGL. The example's checks stop at the bytes served, and the page was run under Node with VTK.js's objects, all but WebGL's; it is looked at in a browser in the manual tests.

## Alternatives considered

- **Chunks of 64 × 64 × 64,** as volume viewers often use: bricks would read fewer chunks, but the thumbnails, deep zoom, the stitch and the ingest all read and write a plane at a time.
- **Never halving z,** as before: a deep stack's coarse levels keep every plane, and a brick's budget goes on depth.
- **Halving z at every level:** right for cubic voxels, but a thin stack of fine pixels would lose its planes in a level or two.
- **Slices as PNG, bricks as JSON:** PNG has to be decoded, and JSON is several times larger and slow to parse. Raw bytes after a header go straight into a typed array, which VTK.js takes as it is.
- **A level chosen only by the client:** `max` lets a client say what its GPU holds without knowing the pyramid.
