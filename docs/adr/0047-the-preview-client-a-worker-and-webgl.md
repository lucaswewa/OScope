# ADR-0047: The preview client: a worker and WebGL

- Status: Accepted
- Date: 2026-11-18
- Phase: W1.5

## Context

- **The live preview is ADR-0014's stream:** binary messages of a header, a JPEG, then ADR-0035's blocks, on the `preview` Thing's WebSocket. Each text message from the client replaces its parameters, and carries its acknowledgement: the next message it is ready for.
- **The page's thread has the app on it:** Vue, the charts, every control. Decoding a 1920-pixel JPEG takes milliseconds, 14.5 times a second from the simulated camera.
- **What browsers give,** in Chromium, Firefox and WebKit alike:
  - Web Workers, which can hold a WebSocket and decode with `createImageBitmap`, and hand the bitmap over without copying it;
  - WebGL2;
  - `requestAnimationFrame`, on the page's thread only, which stops while the page is hidden.
- **Two clocks:** a header's times are the server's, in nanoseconds; the page's are its own. A worker's `performance.now()` counts from another origin than the page's.

## Decision

**1. A worker holds the stream** (`web/src/api/preview/stream.ts`, run by `worker.ts`):
- its WebSocket, with the session's token as the first message under security;
- the parameters, sent whole each time with the acknowledgement;
- each message read (`message.ts`): a version this app doesn't know stops the stream for good, and says why;
- each JPEG decoded with `createImageBitmap`, and the bitmap handed to the page, the mask's bytes with it;
- the socket opened again when it closes, after 250 ms, doubling to 10 s, and back to 250 ms once a socket stayed open 5 s.

**2. The page draws the newest frame, then acknowledges it** (`preview.ts`):
- it holds at most one frame not yet drawn. A newer one replaces it, the older bitmap closed and counted as replaced;
- it draws at the display's next refresh;
- once drawn, it tells the worker, which acknowledges. The server then holds at most two messages for the page, one shown and one on its way, and the page never acknowledges what it hasn't drawn.

**3. The renderer is WebGL2** (`renderer.ts`): one textured quad, placed by the view as a transform.
- **Panning and zooming** draw the last texture again, with no new frame.
- **Zoomed in,** the page asks for the region it shows (`view.ts`), which the server sends at up to its own resolution.
- **Without WebGL2,** a 2D canvas draws the same.
- **A lost context** is made again when it comes back.

**4. The view's extent is the frame's own:** the sensor's pixels that a frame asked for whole shows, from its header. No camera property is read, and a camera's region of interest is followed.

**5. Its numbers,** over the last second:
- frames drawn a second;
- latency: the server's part, from the exposure's end to the message being ready, and the page's, from its arrival to its drawing, both counted in milliseconds since the epoch. The way between isn't measured, since the clocks differ;
- frames replaced before they were drawn.

**6. It rests while the page is hidden.** Its worker closes the socket, and stays for when the page is shown again: without drawing, the page would acknowledge nothing, and the server would hold its session. A preview that stops ends its worker a second after telling it to close, since a worker ended at once may not send its socket's close.

**7. P18's data is published to the app** as the statistics and the mask of the last frame drawn with them. A message with statistics and no mask means nothing was saturated.

**8. The stream's parameters and statistics are typed by hand.** The OpenAPI document describes only the stream's address. A contract test holds the types to the parameters' schema in the `preview` Thing's description.

## Consequences

- **Gains:**
  - Decoding never holds up the page.
  - A frame drawn is never more than two behind the newest, whatever the link.
  - Panning and zooming answer at once; the sharper region follows.
  - The desktop app's shared memory (W9.3) can be one more way into the same worker.
- **Costs:**
  - A worker for each preview on a page.
  - Each step of a pan sends the whole of the parameters again.
  - The 2D canvas scales on the page's thread, more slowly.
  - The latency shown leaves out the network's part.
- **To watch:**
  - **`OffscreenCanvas`** could move the drawing into the worker too. The live viewer's overlays (W3.1) are drawn by the page, so the frame stays there for now.
  - **WebGL contexts lost** under a busy GPU: the next frame fills the texture again.

## Alternatives considered

- **Decoding on the page's thread,** as P17's example page does: simple, but every frame's decode competes with the app.
- **Acknowledging on arrival:** the server would race ahead of what the page has shown, and a busy page would see stale frames.
- **The worker drawing into an `OffscreenCanvas`:** less handing over, but the overlays to come need the frame where the page draws them.
- **An `<img>` per frame:** a decode the page can't see the end of, and no transform for panning.
