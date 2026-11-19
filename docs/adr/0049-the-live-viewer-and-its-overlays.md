# ADR-0049: The live viewer and its overlays

- Status: Accepted
- Date: 2026-11-19
- Phase: W3.1

## Context

- **W1.5's preview client** draws the camera's newest frame on a canvas, with WebGL2, placed by a view: the sensor's point at the canvas's centre, and how many device pixels a sensor pixel spans (ADR-0047). Zoomed in, it asks the server for the region it shows, at up to the sensor's resolution.
- **Control's view shows that image,** fitted, zoomed, panned, and read out (W3.1). W3.3 moves the stage by pointing at it; W3.5 and W3.6 draw over it: the crosshair, a grid, an ROI, a line profile, the saturated pixels and false colour.
- **Three kinds of pixel meet there:** the page's CSS pixels, where the pointer is; the canvas's device pixels, which the view counts in; and the sensor's, which the frames' headers and the camera's region are in. A fourth length is the sample's: `objective/pixel_at_sample`, a sensor pixel's side through the fitted objective. The stage's own axes need the camera-to-stage calibration (P32), which only W3.3 uses.
- **The keys are shared:** the shell's shortcuts take the digits for the tabs (ADR-0048), and W3.3 takes the arrows, and Page Up and Page Down, for jogging.
- **A frame stops coming for more than one reason:** the camera stopped, it was unplugged, or its exposure is long.

## Decision

**1. One view, its numbers in the sensor's pixels.** The viewer (`web/src/features/live/`) keeps W1.5's view, and turns everything else into it: a pointer's CSS pixels into the canvas's device pixels, and those into the sensor's (`viewer.ts`). What the overlays draw is placed in the sensor's pixels too, so that each holds its place under zoom and pan.

**2. Zoom and pan:**
- **The wheel zooms about the pointer,** a notch a step of 1.25, a hard flick four steps at most. Dragging pans.
- **On the focused image:** `+` and `-` zoom about the middle, `0` fits the frame, `1` shows one sensor pixel on one device pixel, and `F` fills the screen. Handled there, they don't reach the shell's shortcuts. The arrows are left to W3.3.
- **The toolbar** has the same, with full screen and the view saved as a PNG.
- **Past one to one,** a sensor pixel spans more than a device pixel, and the renderer draws the pixels as squares, nearest rather than smoothed: what the sensor saw, not what interpolation adds.

**3. Streaming's states:**
- **Not streaming:** `camera/streaming` says no.
- **Waiting for a frame:** it streams, and no frame has come since this page saw it start.
- **Live:** frames come.
- **Stale:** no frame for three of the last gaps between frames, and never sooner than 2 s, so that a long exposure is slow, not stale.

**4. The readouts:** the state; the rate and the latency, from ADR-0047's numbers; the zoom, 100 % at one to one; the region of the sensor the last frame shows; and the place under the pointer, in the sensor's pixels and in µm at the sample from the sensor's centre, along the sensor's axes. The stage's coordinates of that place come with the calibration, in W3.3.

**5. The scale bar** is 1, 2 or 5 times a power of ten in µm, the longest that fits 160 CSS pixels. It comes from `pixel_at_sample`, and there is none while no objective is fitted.

**6. The PNG is the preview, as shown:** the canvas, drawn again and taken in one task, so that WebGL's drawing buffer still holds it. The raw frame is a capture's to save. It is saved through the host adapter (ADR-0046), and full screen is the host's too.

**7. Overlays are layers over the canvas,** in the order W3.5 and W3.6 add them:
- **Geometry** (the crosshair, the grid, an ROI, a profile's line, the scale bar) is drawn as SVG or HTML over the canvas, placed by the view. Its handles are elements, which the keyboard and a screen reader reach.
- **Pixels** (the saturation mask, false colour) are drawn by the renderer, in its fragment shader: per pixel, at the frame's rate.

## Consequences

- **Gains:**
  - The pointer, the readouts and every overlay agree, through one transform.
  - A long exposure doesn't look like a fault, and a stopped stream doesn't look live.
  - Past one to one, the user sees the sensor's own pixels.
- **Costs:**
  - The µm readout is from the sensor's centre, not a stage position, until W3.3's calibration.
  - The keys on the image are only for the focused image.
  - A PNG of the preview is no measurement: its JPEG has lost detail, and its look has been applied.

## Alternatives considered

- **The place under the pointer in the stage's coordinates now:** needs the active calibration, which W3.3 brings with pointing at the image.
- **Overlays drawn in WebGL, all of them:** handles drawn by a shader don't take the keyboard's focus, nor speak to a screen reader.
- **Stale after a fixed second:** a camera exposing for two seconds would always look stale.
- **The PNG from the raw frame:** that is a capture, saved with its metadata; the viewer's button saves what its user sees.
- **Smoothing past one to one:** pleasant, but it shows detail the sensor didn't record.
