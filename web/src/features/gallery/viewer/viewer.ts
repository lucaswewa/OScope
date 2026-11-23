// The deep-zoom viewer's logic (W6.4): a record's image as the tile viewer
// shows it, from the server's Deep Zoom tiles (ADR-0028), placed on the
// stage; its look, the server's black, white and gamma, and a colour map
// for one channel, applied in the app; and the records it can show.
import type { Look } from "@/api/media";
import type {
  Acquisition,
  AnyRecord,
  Contents,
  ImageInfo,
} from "@/features/gallery/project/project";
import type { MapName } from "@/features/live/colourMaps";
import type { Placement } from "@/ui/tile-viewer/coordinates";

/** How an image is drawn: its plane; black and white, from 0 to 1 of its samples' range; gamma; and a colour map. */
export interface ViewLook {
  plane: number;
  black: number;
  white: number;
  gamma: number;
  map: MapName;
}

/** As its thumbnail is: the first plane, the whole range, no curve, its own colours. */
export const PLAIN: ViewLook = { plane: 0, black: 0, white: 1, gamma: 1, map: "none" };

/** The Deep Zoom levels of an image, from a pixel to its full size: ⌈log₂ longest⌉ + 1. */
export function dziLevels(width: number, height: number): number {
  const longest = Math.max(width, height, 1);
  return Math.ceil(Math.log2(longest)) + 1;
}

/** What the server is asked for, for `look`: the plane, and black, white and gamma when they aren't plain. */
export function lookOf(look: ViewLook, planes: number): Look {
  const plane = Math.min(Math.max(0, Math.round(look.plane)), Math.max(0, planes - 1));
  const plain = look.black === PLAIN.black && look.white === PLAIN.white && look.gamma === 1;
  if (plain) return { plane };
  // Black below white, as the server asks.
  const black = Math.min(look.black, look.white - 0.001);
  return { plane, black, white: look.white, gamma: look.gamma };
}

/** What tells a look's tiles from another's. */
export function lookKey(look: Look, map: MapName): string {
  const { plane = 0, black = "", white = "", gamma = "", channel = "" } = look;
  return [plane, black, white, gamma, map, channel].join(":");
}

/**
 * A tile's pixels, RGBA, through a colour map's 256 colours, in place: its
 * level is its grey's, the red of a one-channel image's tile.
 */
export function mapPixels(pixels: Uint8ClampedArray, table: Uint8Array): void {
  for (let i = 0; i < pixels.length; i += 4) {
    const level = pixels[i]! * 3;
    pixels[i] = table[level]!;
    pixels[i + 1] = table[level + 1]!;
    pixels[i + 2] = table[level + 2]!;
  }
}

/** A record's image, if it has one. */
export function imageOf(one: AnyRecord): ImageInfo | undefined {
  if (one.kind === "acquisition") return one.record.image;
  if (one.kind === "result") return one.record.image ?? undefined;
  return undefined;
}

/** The records with an image to view: acquisitions, then results. */
export function viewable(contents: Contents): AnyRecord[] {
  return [
    ...contents.acquisitions.map((record) => ({ kind: "acquisition" as const, record })),
    ...contents.results
      .filter((record) => record.image)
      .map((record) => ({ kind: "result" as const, record })),
  ];
}

/** The stage's place of an acquisition's frame: its centre, in µm. */
function frameAt(acquisition: Acquisition | undefined, frame: number) {
  const frames = acquisition?.frames ?? [];
  return (frames[frame] ?? frames[0])?.position ?? undefined;
}

/**
 * Where `one`'s image lies on the stage, as plane `plane` shows it:
 * - an acquisition's plane, about its frame's place, the stage's at its
 *   centre: a scan's planes are its tiles;
 * - a result, about the first frame of the acquisition it was made from:
 *   a stitched mosaic starts at its first tile's place (P39);
 * - else at the stage's origin, its pixels its own.
 * A pixel is the image's own size, or a µm where it has none.
 */
export function placementOf(one: AnyRecord, contents: Contents, plane = 0): Placement | undefined {
  const image = imageOf(one);
  if (!image) return undefined;
  const pixel = image.pixel_size?.x && image.pixel_size.x > 0 ? image.pixel_size.x : 1;
  const { width, height } = image;
  let at: { x: number; y: number } | undefined;
  let field = { width, height };
  if (one.kind === "acquisition") at = frameAt(one.record, plane);
  else if (one.kind === "result") {
    const input = one.record.provenance.inputs.find((each) => each.kind === "acquisition");
    const from = input && contents.acquisitions.find((each) => each.id === input.id);
    at = frameAt(from, 0);
    // A mosaic's first tile is its acquisition's frame: its corner is that frame's.
    if (from) field = { width: from.image.width, height: from.image.height };
  }
  if (!at) return { x_um: 0, y_um: 0, pixel_um: pixel, width, height };
  return {
    x_um: at.x - (field.width * pixel) / 2,
    y_um: at.y - (field.height * pixel) / 2,
    pixel_um: pixel,
    width,
    height,
  };
}
