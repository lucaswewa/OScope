// The tile viewer's images (W3.8, ADR-0050): an image of tiles, each level
// half the next, the coarsest 0, as OpenSeadragon opens it. Its tiles are
// fetched by the caller, as only the API layer talks to the server (ADR-0039)
// and media come with the session's token (ADR-0045); a tile with nothing
// drawn is shown as nothing, not as a failure.
import type OpenSeadragon from "openseadragon";

/** An image of tiles. */
export interface TiledImage {
  /** Its size at its finest level, in pixels. */
  width: number;
  height: number;
  /** A tile's side, in pixels. */
  tileSize: number;
  /** How many levels: each half the next, the coarsest 0. */
  levels: number;
  /** What tells its tiles from another image's, and from its own before they changed. */
  key: string;
  /** A tile, in OpenSeadragon's levels: its picture, or nothing where nothing is drawn. */
  load: (level: number, col: number, row: number, signal: AbortSignal) => Promise<Blob | undefined>;
}

/** A job's own: what stops its fetch. */
type Loading = { stop?: AbortController };

/** A tile with nothing drawn: a transparent pixel, stretched over it. */
function nothing(): CanvasRenderingContext2D | null {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 1;
  return canvas.getContext("2d");
}

/** The tile source OpenSeadragon opens for `image`: its tiles fetched by `image.load`. */
export function sourceOf(image: TiledImage) {
  return {
    width: image.width,
    height: image.height,
    tileSize: image.tileSize,
    tileOverlap: 0,
    minLevel: 0,
    maxLevel: image.levels - 1,
    // The tile's name in the viewer's cache: the image's key keeps a changed tile apart.
    getTileUrl: (level: number, col: number, row: number) => `${image.key}/${level}/${col}/${row}`,
    downloadTileStart(job: OpenSeadragon.ImageJob): void {
      const stop = new AbortController();
      (job.userData as Loading).stop = stop;
      const { level, x, y } = job.tile;
      image.load(level, x, y, stop.signal).then(
        (picture) => {
          if (stop.signal.aborted) return;
          if (picture) job.finish(picture, null, "rasterBlob");
          else job.finish(nothing(), null, "context2d");
        },
        (error: unknown) => {
          if (!stop.signal.aborted) job.fail(String(error), null);
        },
      );
    },
    downloadTileAbort(job: OpenSeadragon.ImageJob): void {
      (job.userData as Loading).stop?.abort();
    },
  };
}
