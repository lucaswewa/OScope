# ADR-0026: Live stitching

- Status: Accepted
- Date: 2026-11-15
- Phase: P38

## Context

- **D13 asks for both kinds of live stitching:** a mosaic that builds as the stage is jogged by hand, free roaming, and one that builds as a scan's tiles arrive.
- **Plan §5.15** places each frame at its exposure midpoint's position through the calibrated affine, into an in-memory multiscale canvas of 256-pixel tiles. The newest frame wins, or edges are feathered. Dirty tiles are announced on the preview WebSocket. Free roaming takes the live stream's frames only when the stage has moved far enough, and the frame is sharp and not motion-blurred.
- **What already exists:**
  - Each frame on the bus carries the stage's position at its exposure midpoint, interpolated on the stage's track, never guessed ahead (P16).
  - P32 calibrates M, the camera-to-stage affine, for each objective and binning.
  - The preview draws frames in the user's look (ADR-0014).
- **The preview stream is binary.** A teta-wot binary stream can't send text, and blocks after a message's JPEG carry extras (ADR-0035).
- **teta-wot names affordances after their methods,** so `start` and `stop` are every Thing's own (ADR-0034).
- **The camera has a rolling shutter.** The MIChrome 20 reads its rows out over 60 ms, as the simulator does, and a frame's midpoint is its middle row's. At 0.4 mm/s, a frame's last row is exposed 24 µm further along than its first.

## Decision

**1. The canvas** (`oscope_stitch::canvas`) covers a rectangle of the stage.

- **Pixels:** level 0's pixel is `pixel_um`, twice the frames' unless asked otherwise.
- **Levels:** each above halves the one below, rounded up as Deep Zoom's are, up to the first that fits a tile.
- **Tiles** are 256 pixels square, RGBA, alpha being how much of a pixel frames have covered.
- **A frame is drawn through an affine map** from its pixels to level 0's, each canvas pixel sampled bilinearly at its centre.
  - **Newest:** the newest frame wins.
  - **Feather:** a frame's edges fade into what is there over `width_px`; over nothing, it is drawn whole.
- **The levels above** are updated by the quarter of a tile each changed tile makes, as the mean of four pixels weighted by their alpha.
- **Memory is bounded** by a budget: 256 MB unless asked otherwise, and never under five tiles. Past it, the tiles used least recently are written to `scratch/mosaic`, raw, and read back when wanted.
- **Changes are numbered.** Each drawing that changes tiles is a generation, and each tile keeps the one it last changed in. A viewer asks for the tiles changed since the generation it last saw. After a clearing, or for a generation the canvas hasn't had, the answer is that all may have changed.

**2. A frame is placed where it was taken** (`oscope_acquire::mosaic`).

- **It is drawn as the preview draws it,** in the same look and live recipe, at about the canvas's pixel.
- **The affine** goes from the drawn image's pixels to the canvas's:
  - through the sensor's pixels it shows (its ROI, in full-sensor coordinates, at its binning);
  - their offset from the sensor's centre, which is where the stage's position looks;
  - M, from the active calibration of the objective and binning, or else the nominal pixel;
  - the stage's position at the exposure midpoint;
  - and the canvas's corner and pixel.
- **A rolling shutter's rows are each drawn where the stage was as they were exposed.** The stage's velocity is taken from the track, from the exposure's start to its midpoint. Row `r` is exposed a line time × `(r − h / 2)` after the middle row. For a steady velocity, the shift is linear in the row, so the same affine takes it in.
- **Only frames through the objective and at the binning the mosaic began with** are drawn.

**3. Free roaming's gates,** for the newest frame on the bus, as the mosaic keeps up:

| Gate | A frame is drawn only if | Default |
|---|---|---|
| Motion blur | the stage moved at most `blur_px` canvas pixels while a row was exposed | 1 |
| Place | no frame drawn lies within `near` of its shorter side of it … | 0.25 |
| Sharpness | … or it is `sharper` times the sharpest of those, by Brenner's gradient over its mean squared (P34) | 1.1 |

- **Stillness:** while the stage stays within a quarter of a canvas pixel of where a frame was judged to show a place drawn, the next frames are judged the same without being drawn or measured.
- **Without a position,** or with no track over its exposure, a frame isn't drawn.

**4. A scan's frames are drawn as they are taken.**

- **The hook:** the engine hands each tile's frame to `ScanRig::tiles`, with the stage's settled position. Of a stack, it hands the plane nearest the tile's z.
- **Ungated:** the mosaic draws each in turn, from a queue of 8. A frame that would wait behind a full queue is dropped, and counted.
- **Roaming carries on during a scan,** if asked: where the stage stops, a frame may be drawn before the scan's own, which is drawn over it.

**5. The `live_mosaic` Thing:**

- **`state`:** the canvas, whether it draws and roams, how many frames became what, its tiles and their memory, and its generation.
- **`begin`,** with a region, `pixel_um`, a blend, `roam`, the gates and `memory_mb`: a new mosaic on a new canvas. The one before stops, and its tiles go.
- **`halt`** stops it drawing. Its tiles stay to be seen.
- **`clear`** empties its canvas.
- **Its tiles** are at `GET /api/v1/mosaic/tiles/{level}/{col}/{row}`, level 0 the finest:
  - each is a JPEG at quality 85, its bare pixels black, cut at its level's edge;
  - 404 where nothing is drawn;
  - never cached.

**6. Changes go with the previews.** A client asks with `mosaic: true` in its parameters. Each message after which tiles changed carries a block of kind 4 (`TILES`), JSON:

- `mosaic`: the mosaic's number;
- `generation`;
- `all`, when the viewer should fetch every tile again: on its first message, and after a clearing;
- `tiles`: each `[level, col, row]` changed since the session's last block.

## Consequences

- **A viewer addresses tiles as any pyramid's:** OpenSeadragon in `ex-live-mosaic`, M2's viewer later. It learns of changes from the frames it already takes.
- **Scans and roaming share one canvas,** and P39's stitching can reuse its pyramid.
- **The canvas holds what was shown:** 8 bits, in the look of the moment. A change of look doesn't redraw it. Post-acquisition stitching (P39) works from the raw frames.
- **Placement is only as good as the calibration and the track.** Frames aren't registered against each other until P39.
- **The rolling shutter's correction assumes a steady velocity** over the frame. An accelerating stage shears what it draws.
- **Tiles are drawn and served under one lock:** a frame being drawn, or a tile evicted to disk, delays a tile request by milliseconds.
- **Evicted tiles are raw,** 256 KB each: a whole slide at 2 µm would take some 2 GB of scratch. Compressing them is left for later.
- **No messages, no announcements:** with the camera stopped, nothing is drawn either, and a clearing is announced with the next frame.
- **Stillness shortcut:** with nothing moved, a sharper frame, after the light changes, say, is judged the same until the stage moves.

## Alternatives considered

- **Announcing changes as text messages,** or on a stream of their own: teta-wot's binary streams can't send text (P17), and a second stream is another connection per viewer. Blocks ride on the messages a viewer already takes.
- **Gating on a frame's whole smear,** its rolling readout and exposure: moving at all, nearly every frame would be refused (24 µm at 0.4 mm/s), so roaming would draw only where the stage stopped. Taking the shear into the affine keeps the frames taken on the move.
- **Sharpness against the sharpest frame seen anywhere:** glass is less sharp than tissue by its content, so glass after tissue would never be drawn. Comparing frames of the same place avoids that.
- **A 16-bit canvas of the raw signal:** twice the memory, and a display transform for every tile served. The live mosaic is a view.
- **PNG tiles, transparent where bare:** larger and slower to encode than JPEG on black, which is also what P40's deep zoom serves.
