// A depth-from-focus result as a surface (W7.7, ADR-0054): the mesh made
// from its heights, two triangles a square of them, its points' texture
// coordinates on the all-in-focus image; and a profile along a line.

/** Heights in µm, a row at a time from the top, each `pixelUm` across. */
export interface HeightMap {
  width: number;
  height: number;
  heights: Float32Array;
  pixelUm: number;
}

/** A mesh as VTK.js takes it: points x, y, z; triangles, each `3, a, b, c`; and texture coordinates u, v. */
export interface Mesh {
  points: Float32Array;
  polys: Uint32Array;
  tcoords: Float32Array;
  /** The heights' least and greatest, in µm, for colouring. */
  range: [number, number];
}

/**
 * `map` as a mesh, its heights stretched `exaggeration` times. Its first
 * row is at the top, as the image's is: y goes up. A square with a height
 * that isn't a number is left out.
 */
export function meshOf(map: HeightMap, exaggeration = 1): Mesh {
  const { width: w, height: h, heights, pixelUm } = map;
  const points = new Float32Array(w * h * 3);
  const tcoords = new Float32Array(w * h * 2);
  let [least, most] = [Infinity, -Infinity];
  for (let j = 0; j < h; j++)
    for (let i = 0; i < w; i++) {
      const at = j * w + i;
      const z = heights[at]!;
      if (Number.isFinite(z)) [least, most] = [Math.min(least, z), Math.max(most, z)];
      points.set(
        [i * pixelUm, (h - 1 - j) * pixelUm, (Number.isFinite(z) ? z : 0) * exaggeration],
        at * 3,
      );
      tcoords.set([w > 1 ? i / (w - 1) : 0, h > 1 ? j / (h - 1) : 0], at * 2);
    }
  const triangles: number[] = [];
  const whole = (...at: number[]) => at.every((one) => Number.isFinite(heights[one]!));
  for (let j = 0; j + 1 < h; j++)
    for (let i = 0; i + 1 < w; i++) {
      const [a, b, c, d] = [j * w + i, j * w + i + 1, (j + 1) * w + i + 1, (j + 1) * w + i];
      if (whole(a, b, c, d)) triangles.push(3, a, b, c, 3, a, c, d);
    }
  const range: [number, number] = least <= most ? [least, most] : [0, 0];
  return { points, polys: Uint32Array.from(triangles), tcoords, range };
}

/** A point of the map, in µm from its top-left corner. */
export interface At {
  x: number;
  y: number;
}

/** The height at `at`, between the four nearest heights; not a number off the map. */
export function heightAt(map: HeightMap, at: At): number {
  const [x, y] = [at.x / map.pixelUm, at.y / map.pixelUm];
  const [i, j] = [Math.floor(x), Math.floor(y)];
  if (i < 0 || j < 0 || i >= map.width || j >= map.height) return NaN;
  const [i1, j1] = [Math.min(i + 1, map.width - 1), Math.min(j + 1, map.height - 1)];
  const [fx, fy] = [x - i, y - j];
  const v = (a: number, b: number) => map.heights[b * map.width + a]!;
  const top = v(i, j) * (1 - fx) + v(i1, j) * fx;
  const bottom = v(i, j1) * (1 - fx) + v(i1, j1) * fx;
  return top * (1 - fy) + bottom * fy;
}

/**
 * The heights along `from` to `to`, at `samples` points evenly apart: each
 * one's distance from `from`, in µm, and its height, the mean of those a
 * band `across` µm wide finds there, square with the line: a single row of
 * heights is as noisy as depth from focus leaves it.
 */
export function profileOf(
  map: HeightMap,
  from: At,
  to: At,
  samples = 200,
  across = 0,
): { distance: number[]; height: number[] } {
  const length = Math.hypot(to.x - from.x, to.y - from.y);
  const n = Math.max(2, samples);
  // The band's lines: nine, its middle one among them, or the line alone.
  const normal =
    length > 0 ? { x: -(to.y - from.y) / length, y: (to.x - from.x) / length } : { x: 0, y: 0 };
  const offsets = across > 0 ? Array.from({ length: 9 }, (_, k) => ((k - 4) / 8) * across) : [0];
  const distance: number[] = [];
  const height: number[] = [];
  for (let k = 0; k < n; k++) {
    const t = k / (n - 1);
    const at = { x: from.x + t * (to.x - from.x), y: from.y + t * (to.y - from.y) };
    const found = offsets
      .map((d) => heightAt(map, { x: at.x + d * normal.x, y: at.y + d * normal.y }))
      .filter(Number.isFinite);
    distance.push(t * length);
    height.push(found.length ? found.reduce((sum, one) => sum + one, 0) / found.length : NaN);
  }
  return { distance, height };
}

/** A profile's ends: the mean of its first and of its last tenth's heights, those that are numbers. */
export function endsOf(height: readonly number[]): [number, number] {
  const tenth = Math.max(1, Math.floor(height.length / 10));
  const mean = (part: readonly number[]) => {
    const kept = part.filter(Number.isFinite);
    return kept.length ? kept.reduce((sum, one) => sum + one, 0) / kept.length : NaN;
  };
  return [mean(height.slice(0, tenth)), mean(height.slice(-tenth))];
}
