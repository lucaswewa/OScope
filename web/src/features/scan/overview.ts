// The overview, for the tile viewer (W5.1): the server's picture of the
// slide, a byte each of red, green and blue by rows in base64, made one
// tile of one level, placed on the stage; and, before there is one, the
// stage's travel, blank, for a plan to be drawn on.
import type { ValueOf } from "@/api/things/use";
import type { Placement } from "@/ui/tile-viewer/coordinates";
import type { TiledImage } from "@/ui/tile-viewer/source";

import type { Travel } from "./plan";

export type OverviewView = NonNullable<ValueOf<"/api/v1/scan/last_overview">>;

/** Where the overview lies on the stage. */
export function placementOf(view: OverviewView): Placement {
  return {
    x_um: view.origin.x,
    y_um: view.origin.y,
    pixel_um: view.pixel_um,
    width: view.width,
    height: view.height,
  };
}

/** The overview's pixels as RGBA, opaque, from its RGB in base64. */
export function rgbaOf(
  base64: string,
  width: number,
  height: number,
): Uint8ClampedArray<ArrayBuffer> {
  const rgb = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  const rgba = new Uint8ClampedArray(width * height * 4);
  for (let i = 0, o = 0; i + 2 < rgb.length && o < rgba.length; i += 3, o += 4) {
    rgba[o] = rgb[i]!;
    rgba[o + 1] = rgb[i + 1]!;
    rgba[o + 2] = rgb[i + 2]!;
    rgba[o + 3] = 255;
  }
  return rgba;
}

/** The smallest power of two at least `n`: a tile's side. */
const side = (n: number) => 2 ** Math.ceil(Math.log2(Math.max(1, n)));

/** The overview, one tile of one level. */
export function imageOf(view: OverviewView): TiledImage {
  const { width, height } = view;
  let picture: Promise<Blob> | undefined;
  const draw = () => {
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    canvas
      .getContext("2d")!
      .putImageData(new ImageData(rgbaOf(view.image, width, height), width), 0, 0);
    return new Promise<Blob>((resolve, reject) =>
      canvas.toBlob((blob) =>
        blob ? resolve(blob) : reject(new Error("the overview couldn't be drawn")),
      ),
    );
  };
  return {
    width,
    height,
    tileSize: side(Math.max(width, height)),
    levels: 1,
    key: `overview-${view.origin.x}-${view.origin.y}-${width}x${height}-${view.frames}-${view.image.length}`,
    load: () => (picture ??= draw()),
  };
}

/** A blank pixel's side over the stage's travel, in µm. */
const BLANK_UM = 100;

/** The stage's travel, blank, for a plan to be drawn on before an overview. */
export function blankOf(travel: { x: Travel; y: Travel }): {
  image: TiledImage;
  placement: Placement;
} {
  const width = Math.max(1, Math.ceil((travel.x.max - travel.x.min) / BLANK_UM));
  const height = Math.max(1, Math.ceil((travel.y.max - travel.y.min) / BLANK_UM));
  return {
    image: {
      width,
      height,
      tileSize: side(Math.max(width, height)),
      levels: 1,
      key: `stage-${width}x${height}`,
      load: () => Promise.resolve(undefined),
    },
    placement: { x_um: travel.x.min, y_um: travel.y.min, pixel_um: BLANK_UM, width, height },
  };
}
