// Annotations' shapes (W7.5; the behaviour spec in the phase's notes): each
// in µm from the image's top-left corner, as P44 keeps them; what each
// measures, by the same formulas as the server's; each as GeoJSON and back;
// and each edit: moved, and reshaped by a handle.
import type { Point, Rect } from "@/ui/tile-viewer/coordinates";

export type Kind = "point" | "line" | "rectangle" | "ellipse" | "polygon";
export const KINDS: readonly Kind[] = ["point", "line", "rectangle", "ellipse", "polygon"];

interface Common {
  /** Its ID, once kept. */
  id?: string | undefined;
  /** Its name, if it has one. */
  name?: string | undefined;
}
export type Shape = Common &
  (
    | { kind: "point"; at: Point }
    | { kind: "line" | "polygon"; points: Point[] }
    | { kind: "rectangle"; rect: Rect }
    | { kind: "ellipse"; centre: Point; rx: number; ry: number }
  );

/** An ellipse's polygon's points: 128, as P44 keeps it (the spec's bounds). */
export const ELLIPSE_POINTS = 128;

/** The points a shape is drawn and measured by: a closed shape's ring, open. */
export function outlineOf(shape: Shape): Point[] {
  switch (shape.kind) {
    case "point":
      return [shape.at];
    case "line":
    case "polygon":
      return shape.points;
    case "rectangle": {
      const { x, y, width, height } = shape.rect;
      return [
        { x, y },
        { x: x + width, y },
        { x: x + width, y: y + height },
        { x, y: y + height },
      ];
    }
    case "ellipse":
      return Array.from({ length: ELLIPSE_POINTS }, (_, k) => {
        const angle = (2 * Math.PI * k) / ELLIPSE_POINTS;
        return {
          x: shape.centre.x + shape.rx * Math.cos(angle),
          y: shape.centre.y + shape.ry * Math.sin(angle),
        };
      });
  }
}

export const closed = (kind: Kind): boolean =>
  kind === "rectangle" || kind === "ellipse" || kind === "polygon";

const distance = (a: Point, b: Point) => Math.hypot(b.x - a.x, b.y - a.y);

/** The length through `points`, and back to the first if `round`. */
export function lengthOf(points: readonly Point[], round = false): number {
  let sum = 0;
  for (let i = 1; i < points.length; i++) sum += distance(points[i - 1]!, points[i]!);
  if (round && points.length > 2) sum += distance(points.at(-1)!, points[0]!);
  return sum;
}

/** A ring's area by the shoelace formula (Braden, 1986). */
export function areaOf(points: readonly Point[]): number {
  let twice = 0;
  for (let i = 0; i < points.length; i++) {
    const [a, b] = [points[i]!, points[(i + 1) % points.length]!];
    twice += a.x * b.y - b.x * a.y;
  }
  return Math.abs(twice) / 2;
}

/** What a shape measures: a line's length, or a closed shape's perimeter and area. */
export function measureOf(shape: Shape): { length: number; area: number } {
  const points = outlineOf(shape);
  const round = closed(shape.kind);
  return { length: lengthOf(points, round), area: round ? areaOf(points) : 0 };
}

/** A GeoJSON feature, as P44 keeps one. */
export interface Feature {
  type: "Feature";
  id?: string | null;
  geometry:
    | { type: "Point"; coordinates: number[] }
    | { type: "LineString"; coordinates: number[][] }
    | { type: "Polygon"; coordinates: number[][][] };
  properties: Record<string, unknown>;
}

const pair = (point: Point) => [point.x, point.y];

/** `shape` as P44 keeps it. */
export function featureOf(shape: Shape): Feature {
  const properties: Record<string, unknown> = shape.name ? { name: shape.name } : {};
  const id = shape.id ? { id: shape.id } : {};
  if (shape.kind === "point")
    return {
      type: "Feature",
      ...id,
      geometry: { type: "Point", coordinates: pair(shape.at) },
      properties,
    };
  if (shape.kind === "line") {
    const coordinates = shape.points.map(pair);
    return { type: "Feature", ...id, geometry: { type: "LineString", coordinates }, properties };
  }
  const ring = outlineOf(shape).map(pair);
  if (shape.kind === "rectangle") Object.assign(properties, { shape: "rectangle" });
  if (shape.kind === "ellipse") {
    const { centre, rx, ry } = shape;
    Object.assign(properties, { shape: "ellipse", ellipse: { x: centre.x, y: centre.y, rx, ry } });
  }
  const coordinates = [[...ring, ring[0]!]];
  return { type: "Feature", ...id, geometry: { type: "Polygon", coordinates }, properties };
}

const at = (pair: number[]): Point => ({ x: pair[0] ?? 0, y: pair[1] ?? 0 });
const isNumber = (value: unknown): value is number => typeof value === "number";

/** A kept feature as a shape: a rectangle and an ellipse by their `shape`, others by their geometry. */
export function shapeOf(feature: Feature): Shape {
  const common = {
    id: feature.id ?? undefined,
    name: typeof feature.properties.name === "string" ? feature.properties.name : undefined,
  };
  const { geometry, properties } = feature;
  if (geometry.type === "Point") return { ...common, kind: "point", at: at(geometry.coordinates) };
  if (geometry.type === "LineString")
    return { ...common, kind: "line", points: geometry.coordinates.map(at) };
  const ring = (geometry.coordinates[0] ?? []).map(at).slice(0, -1);
  const ellipse = properties.ellipse as Record<string, unknown> | undefined;
  if (
    properties.shape === "ellipse" &&
    ellipse &&
    [ellipse.x, ellipse.y, ellipse.rx, ellipse.ry].every(isNumber)
  )
    return {
      ...common,
      kind: "ellipse",
      centre: { x: ellipse.x as number, y: ellipse.y as number },
      rx: ellipse.rx as number,
      ry: ellipse.ry as number,
    };
  if (properties.shape === "rectangle" && ring.length === 4) {
    const [xs, ys] = [ring.map((p) => p.x), ring.map((p) => p.y)];
    const [x, y] = [Math.min(...xs), Math.min(...ys)];
    return {
      ...common,
      kind: "rectangle",
      rect: { x, y, width: Math.max(...xs) - x, height: Math.max(...ys) - y },
    };
  }
  return { ...common, kind: "polygon", points: ring };
}

/** `shape` moved by `by`. */
export function moved(shape: Shape, by: Point): Shape {
  const shift = (point: Point) => ({ x: point.x + by.x, y: point.y + by.y });
  switch (shape.kind) {
    case "point":
      return { ...shape, at: shift(shape.at) };
    case "line":
    case "polygon":
      return { ...shape, points: shape.points.map(shift) };
    case "rectangle":
      return { ...shape, rect: { ...shape.rect, ...shift(shape.rect) } };
    case "ellipse":
      return { ...shape, centre: shift(shape.centre) };
  }
}

/** A shape's handles: a line's and a polygon's points, a rectangle's corners, an ellipse's axes' ends. */
export function handlesOf(shape: Shape): Point[] {
  if (shape.kind === "ellipse") {
    const { centre: c, rx, ry } = shape;
    return [
      { x: c.x + rx, y: c.y },
      { x: c.x, y: c.y + ry },
      { x: c.x - rx, y: c.y },
      { x: c.x, y: c.y - ry },
    ];
  }
  return outlineOf(shape);
}

/** `shape` with handle `index` at `to`: the opposite corner, or end, staying. */
export function reshaped(shape: Shape, index: number, to: Point): Shape {
  switch (shape.kind) {
    case "point":
      return { ...shape, at: to };
    case "line":
    case "polygon":
      return { ...shape, points: shape.points.map((point, i) => (i === index ? to : point)) };
    case "rectangle": {
      const opposite = outlineOf(shape)[(index + 2) % 4]!;
      return { ...shape, rect: rectBetween(opposite, to) };
    }
    case "ellipse": {
      const opposite = handlesOf(shape)[(index + 2) % 4]!;
      const across = index % 2 === 0;
      const centre = across
        ? { x: (opposite.x + to.x) / 2, y: shape.centre.y }
        : { x: shape.centre.x, y: (opposite.y + to.y) / 2 };
      return across
        ? { ...shape, centre, rx: Math.abs(to.x - opposite.x) / 2 }
        : { ...shape, centre, ry: Math.abs(to.y - opposite.y) / 2 };
    }
  }
}

/** The rectangle two corners make, square with the image. */
export function rectBetween(a: Point, b: Point): Rect {
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(b.x - a.x),
    height: Math.abs(b.y - a.y),
  };
}

/** A shape of `kind` made from what was drawn: its points, or a dragged rectangle. */
export function made(kind: Kind, points: readonly Point[], rect?: Rect): Shape | undefined {
  if (kind === "point") return points[0] && { kind, at: points[0] };
  if (kind === "line") return points.length >= 2 ? { kind, points: [...points] } : undefined;
  if (kind === "polygon") return points.length >= 3 ? { kind, points: [...points] } : undefined;
  if (!rect || rect.width <= 0 || rect.height <= 0) return undefined;
  if (kind === "rectangle") return { kind, rect };
  const centre = { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
  return { kind, centre, rx: rect.width / 2, ry: rect.height / 2 };
}
