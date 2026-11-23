// Annotations' shapes and drawing (W7.5; the behaviour spec in the phase's
// notes): lengths and areas against known shapes, drawn at several zooms;
// the ellipse's bounds; each shape as GeoJSON and back; and each edit.
import { describe, expect, it } from "vitest";

import { drafted, nudge, pressed, type Draft } from "./drawing";
import {
  areaOf,
  featureOf,
  handlesOf,
  KINDS,
  lengthOf,
  made,
  measureOf,
  moved,
  reshaped,
  shapeOf,
  type Shape,
} from "./shapes";

const p = (x: number, y: number) => ({ x, y });

/** `points`, in µm, pressed on the screen at `scale` screen pixels to a µm: as the page reads them back. */
function drawn(kind: Draft["kind"], points: { x: number; y: number }[], scale: number): Shape {
  const near = 6 / scale;
  let draft: Draft = { kind, points: [] };
  let done: Shape | undefined;
  for (const point of points) {
    const screen = { x: point.x * scale, y: point.y * scale };
    const result = pressed(draft, { x: screen.x / scale, y: screen.y / scale }, near);
    draft = result.draft;
    done = result.done ?? done;
  }
  return done!;
}

describe("what a shape measures", () => {
  it("is the same at every zoom it is drawn at", () => {
    for (const scale of [0.25, 1, 4, 16]) {
      // A 3-4-5 triangle of 30, 40 and 50 µm, closed on its first point.
      const triangle = drawn("polygon", [p(0, 0), p(40, 0), p(40, 30), p(0, 0)], scale);
      expect(measureOf(triangle).area).toBeCloseTo(600, 9);
      expect(measureOf(triangle).length).toBeCloseTo(120, 9);
      // A line of two segments, ended by a press on its last point.
      const line = drawn("line", [p(0, 0), p(30, 40), p(30, 100), p(30, 100)], scale);
      expect(measureOf(line)).toEqual({ length: 110, area: 0 });
    }
  });

  it("is a rectangle's w × h, and a concave polygon's by the shoelace formula", () => {
    const rect = made("rectangle", [], { x: 10, y: 20, width: 30, height: 5 })!;
    expect(measureOf(rect)).toEqual({ length: 70, area: 150 });
    // An L of three 10 µm squares, either way round.
    const ell = [p(0, 0), p(20, 0), p(20, 10), p(10, 10), p(10, 20), p(0, 20)];
    expect(areaOf(ell)).toBe(300);
    expect(areaOf([...ell].reverse())).toBe(300);
    expect(lengthOf(ell, true)).toBe(80);
  });

  it("is an ellipse's within the spec's bounds, of every shape", () => {
    for (const [rx, ry] of [
      [10, 10],
      [40, 10],
      [200, 10],
    ] as const) {
      const ellipse = made("ellipse", [], { x: 0, y: 0, width: 2 * rx, height: 2 * ry })!;
      const { area, length } = measureOf(ellipse);
      expect(area / (Math.PI * rx * ry)).toBeGreaterThan(0.9995);
      // Ramanujan's approximation of its perimeter.
      const h = ((rx - ry) / (rx + ry)) ** 2;
      const ramanujan = Math.PI * (rx + ry) * (1 + (3 * h) / (10 + Math.sqrt(4 - 3 * h)));
      expect(length / ramanujan).toBeGreaterThan(0.9998);
      expect(length / ramanujan).toBeLessThan(1);
    }
  });
});

describe("a shape kept", () => {
  it("is read back as itself, each kind", () => {
    const shapes: Shape[] = [
      { kind: "point", at: p(1, 2), name: "a" },
      { kind: "line", points: [p(0, 0), p(3, 4)] },
      { kind: "rectangle", rect: { x: 1, y: 2, width: 3, height: 4 }, id: "7" },
      { kind: "ellipse", centre: p(5, 5), rx: 3, ry: 2 },
      { kind: "polygon", points: [p(0, 0), p(4, 0), p(0, 3)] },
    ];
    expect(shapes.map((shape) => shape.kind)).toEqual(KINDS);
    for (const shape of shapes)
      expect(shapeOf(featureOf(shape))).toEqual({ id: undefined, name: undefined, ...shape });
    const ring = featureOf(shapes[3]!).geometry.coordinates as number[][][];
    expect(ring[0]).toHaveLength(129);
    expect(ring[0]![0]).toEqual(ring[0]![128]);
  });
});

describe("an edit", () => {
  const rect: Shape = { kind: "rectangle", rect: { x: 0, y: 0, width: 10, height: 20 } };

  it("moves a shape, and reshapes it by a handle, the opposite one staying", () => {
    expect(moved(rect, p(5, -5))).toEqual({
      kind: "rectangle",
      rect: { x: 5, y: -5, width: 10, height: 20 },
    });
    // The bottom right corner dragged past the top left: the rectangle turned over, square still.
    expect(reshaped(rect, 2, p(-10, -10))).toEqual({
      kind: "rectangle",
      rect: { x: -10, y: -10, width: 10, height: 10 },
    });
    const ellipse: Shape = { kind: "ellipse", centre: p(0, 0), rx: 10, ry: 5 };
    expect(handlesOf(ellipse)).toEqual([p(10, 0), p(0, 5), p(-10, 0), p(0, -5)]);
    expect(reshaped(ellipse, 0, p(30, 3))).toEqual({
      kind: "ellipse",
      centre: p(10, 0),
      rx: 20,
      ry: 5,
    });
    const line: Shape = { kind: "line", points: [p(0, 0), p(1, 1)] };
    expect(reshaped(line, 1, p(2, 2))).toEqual({ kind: "line", points: [p(0, 0), p(2, 2)] });
  });

  it("is drawn: two corners from the keyboard, and the shape through the pointer", () => {
    let draft: Draft = { kind: "ellipse", points: [] };
    draft = pressed(draft, p(0, 0), 1).draft;
    expect(drafted(draft, p(20, 10))).toEqual({ kind: "ellipse", centre: p(10, 5), rx: 10, ry: 5 });
    expect(pressed(draft, p(20, 10), 1).done).toEqual({
      kind: "ellipse",
      centre: p(10, 5),
      rx: 10,
      ry: 5,
    });
    // A polygon of two points so far shows as a line; of three, closed through the pointer.
    const two: Draft = { kind: "polygon", points: [p(0, 0), p(10, 0)] };
    expect(drafted(two)).toEqual({ kind: "line", points: [p(0, 0), p(10, 0)] });
    expect(measureOf(drafted(two, p(10, 10))!).area).toBe(50);
    // A line's one point pressed twice isn't a line yet.
    expect(pressed({ kind: "line", points: [p(0, 0)] }, p(0, 0), 1).done).toBeUndefined();
  });

  it("moves by the arrows a screen pixel, or ten with Shift", () => {
    expect(nudge("ArrowRight", false, 2)).toEqual(p(0.5, 0));
    expect(nudge("ArrowUp", true, 2)).toEqual(p(0, -5));
    expect(nudge("Enter", false, 2)).toBeUndefined();
  });
});
