// The live preview, for a page (W1.5): its canvas's renderer, the view onto
// the frame, the client, and what the stream is asked for, following the
// canvas's size and the view. It starts when the page is mounted, and stops
// when it goes.
import { onBeforeUnmount, onMounted, shallowRef, type Ref } from "vue";

import type { PixelRect } from "./message.ts";
import type { PreviewAsk, StatsRequest } from "./params.ts";
import { PreviewClient, type PreviewPlace } from "./preview.ts";
import { createRenderer, type Renderer } from "./renderer.ts";
import type { DecodedFrame } from "./stream.ts";
import { fitted, kept, panned, roiFor, zoomed, type Size, type View } from "./view.ts";

/** What a page chooses: the rest, the viewport and the region, follow its canvas and its view. */
export interface PreviewChoices {
  quality?: number | undefined;
  max_fps?: number | undefined;
  stats?: StatsRequest | undefined;
}

/** Whether `rect` lies inside `extent`. */
const inside = (rect: PixelRect, extent: PixelRect | undefined) =>
  extent !== undefined &&
  rect.x >= extent.x &&
  rect.y >= extent.y &&
  rect.x + rect.width <= extent.x + extent.width &&
  rect.y + rect.height <= extent.y + extent.height;

/** The preview on `canvas`: its state, numbers and statistics, and the view's controls. */
export function usePreview(
  canvas: Ref<HTMLCanvasElement | undefined>,
  choices: PreviewChoices = {},
  place?: PreviewPlace,
) {
  let renderer: Renderer | undefined;
  let observer: ResizeObserver | undefined;
  let size: Size = { width: 0, height: 0 };
  let chosen = { ...choices };
  let drawing = false;
  /** The sensor's pixels a whole frame shows, as the last frame asked for whole said. */
  const extent = shallowRef<PixelRect>();
  const view = shallowRef<View>();
  const kind = shallowRef<Renderer["kind"]>();

  const region = () =>
    view.value && extent.value ? roiFor(view.value, extent.value, size) : undefined;
  const ask = (): PreviewAsk => ({
    ...chosen,
    width: size.width,
    height: size.height,
    roi: region(),
  });

  function paint(frame: DecodedFrame): void {
    if (!renderer) return frame.bitmap.close();
    const { roi } = frame.header;
    // A frame asked for whole gives the extent; a region still on its way
    // after the whole frame was asked for again lies inside it.
    if (!region() && !inside(roi, extent.value)) {
      extent.value = roi;
      view.value = view.value ? kept(view.value, roi, size) : fitted(roi, size);
    }
    renderer.show(frame.bitmap, roi);
    if (view.value) renderer.draw(view.value);
  }

  const client = new PreviewClient(paint, ask(), place);

  /** Draws again at the next refresh, without a new frame. */
  function redraw(): void {
    if (drawing) return;
    drawing = true;
    requestAnimationFrame(() => {
      drawing = false;
      if (renderer && view.value) renderer.draw(view.value);
    });
  }

  /** The view changed: drawn again, and the region asked for. */
  function moved(to: View): void {
    view.value = to;
    client.setAsk(ask());
    redraw();
  }

  function resized(): void {
    if (!renderer) return;
    const whole = !region();
    size = renderer.size();
    if (view.value && extent.value) {
      view.value = whole ? fitted(extent.value, size) : kept(view.value, extent.value, size);
    }
    client.setAsk(ask());
    redraw();
  }

  onMounted(() => {
    if (!canvas.value) return;
    renderer = createRenderer(canvas.value);
    kind.value = renderer.kind;
    size = renderer.size();
    client.setAsk(ask());
    client.start();
    observer = new ResizeObserver(resized);
    observer.observe(canvas.value);
  });

  onBeforeUnmount(() => {
    observer?.disconnect();
    client.stop();
    renderer?.dispose();
    renderer = undefined;
  });

  return {
    state: client.state,
    numbers: client.numbers,
    stats: client.stats,
    mask: client.mask,
    /** How it draws: WebGL2, or a 2D canvas. */
    kind,
    view,
    extent,
    /** Zooms by `factor` about the canvas's point `at`, in device pixels. */
    zoomBy(factor: number, at: [number, number]): void {
      if (view.value && extent.value) moved(zoomed(view.value, factor, at, extent.value, size));
    },
    /** Moves the view by `by`, in device pixels. */
    panBy(by: [number, number]): void {
      if (view.value && extent.value) moved(panned(view.value, by, extent.value, size));
    },
    /** The whole frame again. */
    whole(): void {
      if (extent.value) moved(fitted(extent.value, size));
    },
    /** Each sensor pixel on one device pixel, the point under `at` kept (W3.1). */
    oneToOne(at: [number, number]): void {
      if (view.value && extent.value) {
        moved(zoomed(view.value, 1 / view.value.scale, at, extent.value, size));
      }
    },
    /** The canvas's size, in device pixels. */
    size: (): Size => size,
    /** The view as drawn now, as a PNG: the preview, not the raw frame (W3.1). */
    snapshot(): Promise<Blob | undefined> {
      return new Promise((done) => {
        if (!renderer || !view.value || !canvas.value) return done(undefined);
        // Drawn and taken in one task: a WebGL canvas keeps its drawing until then.
        renderer.draw(view.value);
        canvas.value.toBlob((blob) => done(blob ?? undefined), "image/png");
      });
    },
    /** The quality, the rate or the statistics, from now on. */
    choose(more: PreviewChoices): void {
      chosen = { ...chosen, ...more };
      client.setAsk(ask());
    },
  };
}
