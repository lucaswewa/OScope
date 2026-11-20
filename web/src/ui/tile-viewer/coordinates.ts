// The tile viewer's coordinates (W3.8, ADR-0050): an image placed on the
// stage, each of its pixels a square of the stage in µm; and OpenSeadragon's
// viewport, whose unit is the image's width, its origin the image's top left.

/** Where an image lies on the stage. */
export interface Placement {
  /** The stage's x at the image's left edge, in µm. */
  x_um: number;
  /** The stage's y at its top edge. */
  y_um: number;
  /** A pixel's side, in µm. */
  pixel_um: number;
  /** Its width, in pixels. */
  width: number;
  /** Its height. */
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

export interface Rect extends Point {
  width: number;
  height: number;
}

/** A place on the stage, in µm, as a point of the image, in its pixels. */
export function stageToImage(at: Placement, um: Point): Point {
  return { x: (um.x - at.x_um) / at.pixel_um, y: (um.y - at.y_um) / at.pixel_um };
}

/** A point of the image, in its pixels, as a place on the stage, in µm. */
export function imageToStage(at: Placement, px: Point): Point {
  return { x: at.x_um + px.x * at.pixel_um, y: at.y_um + px.y * at.pixel_um };
}

/** A place on the stage as a point of the viewport, in image widths. */
export function stageToViewport(at: Placement, um: Point): Point {
  const px = stageToImage(at, um);
  return { x: px.x / at.width, y: px.y / at.width };
}

/** A point of the viewport as a place on the stage. */
export function viewportToStage(at: Placement, point: Point): Point {
  return imageToStage(at, { x: point.x * at.width, y: point.y * at.width });
}

/** A rectangle of the stage, in µm, as one of the viewport. */
export function stageRectToViewport(at: Placement, um: Rect): Rect {
  const corner = stageToViewport(at, um);
  const across = at.pixel_um * at.width;
  return { ...corner, width: um.width / across, height: um.height / across };
}

/** A field of view `width` × `height` µm about `centre`, as a rectangle of the stage. */
export function fieldAbout(centre: Point, width: number, height: number): Rect {
  return { x: centre.x - width / 2, y: centre.y - height / 2, width, height };
}
