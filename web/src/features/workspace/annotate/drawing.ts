// Drawing a shape (W7.5; the behaviour spec in the phase's notes): presses,
// from the pointer or the keyboard, made into a shape; the shape as it is
// drawn, through the pointer, to show and measure; and the arrows' nudge.
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";

import { made, rectBetween, type Kind, type Shape } from "./shapes";

/** A shape being drawn: its kind, its points so far, and a rectangle's first corner from the keyboard. */
export interface Draft {
  kind: Kind;
  points: Point[];
  corner?: Point;
}

const distance = (a: Point, b: Point) => Math.hypot(b.x - a.x, b.y - a.y);

/**
 * A press at `at` while drawing: the draft as it goes on, and the shape it
 * ends, if it does. Two presses within `near` µm are one place: they end a
 * line or a polygon, as does a polygon's press on its first point.
 */
export function pressed(draft: Draft, at: Point, near: number): { draft: Draft; done?: Shape } {
  const { kind, points } = draft;
  const fresh = { kind, points: [] };
  if (kind === "point") return { draft: fresh, done: { kind, at } };
  if (kind === "rectangle" || kind === "ellipse") {
    if (!draft.corner) return { draft: { ...fresh, corner: at } };
    const done = made(kind, [], rectBetween(draft.corner, at));
    return done ? { draft: fresh, done } : { draft };
  }
  const [first, last] = [points[0], points.at(-1)];
  const closing = kind === "polygon" && first && points.length >= 3 && distance(first, at) <= near;
  if ((last && distance(last, at) <= near) || closing) {
    const done = made(kind, points);
    if (done) return { draft: fresh, done };
    return { draft };
  }
  return { draft: { kind, points: [...points, at] } };
}

/** The shape being drawn, through `pointer`, or as a rectangle is dragged: what shows, and is measured. */
export function drafted(draft: Draft, pointer?: Point, dragged?: Rect): Shape | undefined {
  const { kind } = draft;
  if (kind === "point") return undefined;
  if (kind === "rectangle" || kind === "ellipse") {
    const rect =
      dragged ?? (draft.corner && pointer ? rectBetween(draft.corner, pointer) : undefined);
    return rect && made(kind, [], rect);
  }
  const points = pointer ? [...draft.points, pointer] : draft.points;
  if (kind === "polygon" && points.length >= 3) return { kind, points };
  return points.length >= 2 ? { kind: "line", points } : undefined;
}

const ARROWS: Record<string, Point> = {
  ArrowLeft: { x: -1, y: 0 },
  ArrowRight: { x: 1, y: 0 },
  ArrowUp: { x: 0, y: -1 },
  ArrowDown: { x: 0, y: 1 },
};

/** An arrow key's move, in µm: a screen pixel, or ten with Shift, at `scale` screen pixels to a µm. */
export function nudge(key: string, shift: boolean, scale: number): Point | undefined {
  const arrow = ARROWS[key];
  if (!arrow) return undefined;
  const step = (shift ? 10 : 1) / Math.max(scale, 1e-9);
  return { x: arrow.x * step, y: arrow.y * step };
}
