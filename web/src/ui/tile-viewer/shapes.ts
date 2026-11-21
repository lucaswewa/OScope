// Shapes drawn over the tile viewer's image (W5.1): a scan's frames, its
// path, its region, each by its points on the stage, in µm, and drawn in the
// image's pixels so that they zoom with it.
import { stageToImage, type Placement, type Point, type Rect } from "./coordinates";

/** What a shape is, which its look follows: a scan's tiles done, next and at fault too (W5.2). */
export type ShapeKind = "frame" | "first" | "path" | "region" | "done" | "next" | "fault";

/** A shape on the stage: closed, but for a path. */
export interface Shape {
  kind: ShapeKind;
  points: readonly Point[];
}

/** A shape's points in the image's pixels, as SVG writes them. */
export function pointsIn(at: Placement, shape: Shape): string {
  return shape.points
    .map((point) => {
      const px = stageToImage(at, point);
      return `${px.x.toFixed(2)},${px.y.toFixed(2)}`;
    })
    .join(" ");
}

/** A rectangle of the stage as a closed shape. */
export function rectShape(kind: ShapeKind, rect: Rect): Shape {
  const { x, y, width, height } = rect;
  return {
    kind,
    points: [
      { x, y },
      { x: x + width, y },
      { x: x + width, y: y + height },
      { x, y: y + height },
    ],
  };
}

/** The rectangle two corners make, square with the stage, whichever way round. */
export function between(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(b.x - a.x),
    height: Math.abs(b.y - a.y),
  };
}
