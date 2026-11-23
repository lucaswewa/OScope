// The live mosaic (W3.8, P38): its canvas on the stage, its levels as the
// tile viewer counts them, and the field of view marked over it.
import { mosaicTile } from "@/api/media";
import type { ValueOf } from "@/api/things/use";
import { fieldAbout, type Placement, type Point, type Rect } from "@/ui/tile-viewer/coordinates";
import type { TiledImage } from "@/ui/tile-viewer/source";

export type MosaicState = ValueOf<"/api/v1/live_mosaic/state">;

/** Where the mosaic's canvas lies on the stage, once there is one. */
export function placementOf(state: MosaicState | undefined): Placement | undefined {
  const canvas = state?.canvas;
  if (!canvas) return undefined;
  const { x_um, y_um, pixel_um, width, height } = canvas;
  return { x_um, y_um, pixel_um, width, height };
}

/**
 * The server's level for the viewer's: the viewer counts the levels from the
 * coarsest, at 0, as each halves the next; the server from the finest.
 */
export function serverLevel(level: number, levels: number): number {
  return levels - 1 - level;
}

/**
 * The mosaic as the tile viewer shows it, once it has a canvas: its tiles
 * fetched again as its generation moves on (W3.8; the scan's view, W5.2).
 */
export function mosaicImage(state: MosaicState | undefined): TiledImage | undefined {
  if (!state?.canvas) return undefined;
  const levels = state.levels;
  return {
    width: state.canvas.width,
    height: state.canvas.height,
    tileSize: state.tile,
    levels,
    key: `mosaic-${state.mosaic}-${state.generation}`,
    load: (level, col, row, signal) => mosaicTile(serverLevel(level, levels), col, row, { signal }),
  };
}

/**
 * The field of view about the stage's place, in µm: the camera's region, or
 * its whole sensor, through the objective. Centred: where a region off the
 * sensor's middle lies on the stage is the calibration's to say.
 */
export function fieldOf(
  at: Point | undefined,
  size: { width: number; height: number } | undefined,
  pixel: number | null | undefined,
): Rect | undefined {
  if (!at || !size || !pixel) return undefined;
  return fieldAbout(at, size.width * pixel, size.height * pixel);
}
