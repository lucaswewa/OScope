# ADR-0050: The tile viewer

- Status: Accepted
- Date: 2026-11-20
- Phase: W3.8

## Context

- **Several of the app's pages show images of tiles, deep-zoomed:**
  - the live mosaic, drawn as the stage roams (W3.8);
  - the scans' overviews and plans (W5);
  - a project's stitched results and acquisitions in the Gallery (W6).
- **The server serves them as levels of square tiles:**
  - the live mosaic at `/api/v1/mosaic/tiles/{level}/{col}/{row}`, level 0 the finest, a 404 where nothing has been drawn (P38);
  - a record's deep zoom by its descriptor and tiles (ADR-0028).
- **Only `src/api/` talks to the server** (ADR-0039). Where security is on, media come with the session's token, and are shown from what the API layer fetched (ADR-0045). `src/ui/` knows nothing of the API.
- **Milestone 1's examples vendor OpenSeadragon 6.1.1** (BSD-3-Clause) for their deep-zoom pages (D4).
- **The stage's µm are the app's unit for places:** a move, the field of view, a scan's region.

## Decision

**1. OpenSeadragon, wrapped once.**
- The npm package `openseadragon`, 6.1.1, the version Milestone 1's examples vendor. It brings its own types.
- `web/src/ui/tile-viewer/TileViewer.vue` is the only module that imports it: a lint rule refuses it anywhere else.
- The live mosaic, the scans and the Gallery use `TileViewer`.

**2. The caller fetches the tiles.**
- An image is its size, its tile's side, its levels (each half the next, the coarsest 0, as OpenSeadragon counts them), a key, and a `load` function: a tile's picture, or nothing where nothing is drawn.
- The wrapper gives OpenSeadragon a tile source whose `downloadTileStart` calls `load`, and stops it on `downloadTileAbort`. OpenSeadragon makes no request of its own.
- A feature's `load` calls the API layer, which sends the token and maps the server's errors (`api/media.ts`).
- A tile with nothing drawn finishes as a transparent pixel, not as a failure: a live mosaic is mostly empty, and a failed tile is logged.

**3. Places are the stage's µm.**
- An image is placed on the stage: the µm at its top left, and a pixel's side.
- `coordinates.ts` turns them into OpenSeadragon's viewport and back.
- What the viewer is given, and what it says, is in µm:
  - the rectangle marked over the image (the field of view);
  - where to look first;
  - the pointer's place, read out;
  - a double click, said as a place on the stage.

**4. Its controls are the app's.**
- OpenSeadragon's own buttons are off. Zoom in, zoom out and the whole image are the app's toolbar buttons.
- The scale bar is the live viewer's, in 1, 2 or 5 times a power of ten (`ui/lengths.ts`).
- OpenSeadragon's navigator stays, drawn in the theme's tokens.
- A click doesn't zoom, and neither does a double click: a double click is the caller's.
- The canvas drawer: pixels past one to one drawn as squares, and every browser alike.

**5. An image whose tiles change is shown again by a new key.**
- The tiles in view are fetched again, as a new image drawn over the old.
- The old goes once the new one's tiles in view have come, or after 2 s, whatever came: nothing blinks.
- The live mosaic's key is its number and its generation, which its state, read each second, says.

## Consequences

- **Every tile goes through the API layer:** the token, the errors and the session's end are handled as for any other request (ADR-0045), on the web and in the desktop app alike.
- **Callers stay simple:** a place in µm is all they handle, and a new image is a new key.
- **A refresh fetches every tile in view, changed or not.** For the live mosaic that is a few dozen small JPEGs a second, from the server beside it. The preview stream can say which tiles changed (P38's block), but the page keeps only the newest frame, and the server drops messages under load, so a viewer led by it could miss a tile. Following the block, with a periodic full refresh, is a follow-up.
- **OpenSeadragon's own keys work on the focused viewer:** the arrows pan, and `+`, `-` and `0` zoom. It marks the keys it takes, so the shell's shortcuts pass them by (ADR-0048).
- **The navigator's look is OpenSeadragon's, coloured by the tokens.**

## Alternatives considered

- **Leaflet, with a simple coordinate system:** fit for maps of tiles, but its pyramid, its tile loading and its zoom are a map's, and Milestone 1's pages already use OpenSeadragon.
- **OpenLayers:** larger, and a map library too.
- **A viewer of our own, on W1.5's renderer:** the pyramid, the tile cache and the gestures would all be ours to write and test.
- **OpenSeadragon's own fetching, with `ajaxHeaders`:** simpler, but it would talk to the server outside `src/api/`, and keep the token outside the session's handling.
