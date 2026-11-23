// A surface from its heights (W7.7): the mesh from a small height map, and
// a profile's sampling along a line, in a band, and its ends.
import { describe, expect, it } from "vitest";

import { endsOf, heightAt, meshOf, profileOf, type HeightMap } from "./surface";

/** A map of `width` × `height`, each height `of(i, j)`, 2 µm apart. */
const mapOf = (width: number, height: number, of: (i: number, j: number) => number): HeightMap => ({
  width,
  height,
  pixelUm: 2,
  heights: Float32Array.from({ length: width * height }, (_, k) =>
    of(k % width, Math.floor(k / width)),
  ),
});

describe("the mesh", () => {
  it("has a point a height, its first row at the top, stretched up", () => {
    const mesh = meshOf(
      mapOf(3, 2, (i, j) => i + 10 * j),
      2,
    );
    expect(Array.from(mesh.points)).toEqual([
      0, 2, 0, 2, 2, 2, 4, 2, 4, 0, 0, 20, 2, 0, 22, 4, 0, 24,
    ]);
    expect(Array.from(mesh.tcoords)).toEqual([0, 0, 0.5, 0, 1, 0, 0, 1, 0.5, 1, 1, 1]);
    expect(mesh.range).toEqual([0, 12]);
  });

  it("has two triangles a square, and none for a square with a height that isn't a number", () => {
    expect(Array.from(meshOf(mapOf(3, 2, () => 1)).polys)).toEqual([
      3, 0, 1, 4, 3, 0, 4, 3, 3, 1, 2, 5, 3, 1, 5, 4,
    ]);
    const holed = meshOf(mapOf(3, 2, (i, j) => (i === 0 && j === 0 ? NaN : 1)));
    expect(Array.from(holed.polys)).toEqual([3, 1, 2, 5, 3, 1, 5, 4]);
    expect(meshOf(mapOf(2, 2, () => NaN)).range).toEqual([0, 0]);
  });
});

describe("a profile", () => {
  // A ramp: a height of x in µm, everywhere.
  const ramp = mapOf(11, 5, (i) => 2 * i);

  it("samples between the heights, and none off the map", () => {
    expect(heightAt(ramp, { x: 3, y: 1 })).toBeCloseTo(3, 9);
    expect(heightAt(ramp, { x: -1, y: 1 })).toBeNaN();
    const { distance, height } = profileOf(ramp, { x: 0, y: 4 }, { x: 16, y: 4 }, 5);
    expect(distance).toEqual([0, 4, 8, 12, 16]);
    expect(height.map((h) => Math.round(h * 1e9) / 1e9)).toEqual([0, 4, 8, 12, 16]);
  });

  it("takes a band's mean across the line, its heights off the map left out", () => {
    // A ridge along y = 4 µm: 10 on it, 0 off it; a band 4 µm across finds both.
    const ridge = mapOf(11, 5, (_, j) => (j === 2 ? 10 : 0));
    const along = profileOf(ridge, { x: 4, y: 4 }, { x: 12, y: 4 }, 3);
    expect(along.height).toEqual([10, 10, 10]);
    const band = profileOf(ridge, { x: 4, y: 4 }, { x: 12, y: 4 }, 3, 4);
    expect(band.height[0]).toBeGreaterThan(0);
    expect(band.height[0]).toBeLessThan(10);
    // At the map's edge, only the band's half on it counts.
    const edge = profileOf(ramp, { x: 0, y: 0 }, { x: 20, y: 0 }, 2, 4);
    expect(edge.height.map(Math.round)).toEqual([0, 20]);
  });

  it("has ends: the means of its first and last tenths, numbers only", () => {
    expect(endsOf([1, 1, NaN, 5, 5, 5, 5, 5, 5, 5, 9, 9, 9, 9, 9, 9, 9, 9, 3, 3])).toEqual([1, 3]);
    expect(endsOf([NaN, NaN])).toEqual([NaN, NaN]);
  });
});
