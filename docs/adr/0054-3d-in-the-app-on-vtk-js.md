# ADR-0054: 3D in the app, on VTK.js

- Status: Accepted
- Date: 2026-11-23
- Phase: W7.7

## Context

- **A depth-from-focus result is a surface** (P42): its heights in µm, and its all-in-focus image. The Workspace shows it to turn and measure (W7.7), and W7.8 shows volumes.
- **P42's example draws one with VTK.js,** vendored into its page. The app needs it as a package, in its own bundle.
- **The app's rules** (ADR-0039): a library that owns part of the page is wrapped once, as OpenSeadragon is in the tile viewer (ADR-0050) and Vue Flow in the graph editor (ADR-0053).
- **Not every browser has WebGL2,** and a test's browser may not.

## Decision

**1. VTK.js, `@kitware/vtk.js` 38.0.5, BSD-3-Clause,** for the surface: its mesh, its texture, its lights and colour maps, and the turning and zooming of its camera.

**2. In a chunk of its own, loaded when first needed:** the Workspace's 3D tab imports it only when a surface is first shown, so the app's first load doesn't carry it.

**3. Imported only in `web/src/features/workspace/three-d/`:** a lint rule refuses it anywhere else.

**4. The surface is made by the page, not by VTK.js's readers:**
- the heights and the image come from the media plane's volume routes, as bricks, at the finest level whose part holds a few hundred thousand voxels: coarser from afar;
- the page makes the mesh, two triangles a square of heights, leaving out a square whose height isn't a number;
- the page samples a profile itself, along a line drawn on the height map.

**5. Without WebGL2,** the tab says so, and shows the height map as an image: the profile still works.

## Consequences

- **The first load is as it was:** VTK.js's chunk comes only with the first surface.
- **The mesh and the profile are the app's own code,** tested without a browser.
- **W7.8's volumes can use the same chunk.**
- **A test's surface is drawn by its browser's WebGL**, in software where it has no GPU, so its baseline is one computer's, as every baseline is.

## Alternatives considered

- **three.js:** MIT, and smaller, but a scene graph to build the surface's lighting and colour maps on; VTK.js has them, and its volume rendering serves W7.8.
- **VTK.js vendored, as P42's example does:** no version to check, nor licence to enforce; a package is.
- **WebGL by hand:** a renderer, its camera's controls and its picking, to write and test.
- **The server's PLY mesh, P42's export:** a fixed stride, not coarser from afar, and no texture's coordinates.
