// A series' trend, for the memory a page keeps (W10.3): Theil–Sen's
// estimate of its slope, the median of the slopes between every pair of
// its points (H. Theil, "A rank-invariant method of linear and polynomial
// regression analysis", 1950; P. K. Sen, "Estimates of the regression
// coefficient based on Kendall's tau", JASA 63:1379–1389, 1968). A few
// samples taken in a collection's wake move a mean of the slopes, not
// their median: as P53's soak reads the server's memory.

/** The median of `values`, NaN for none. */
function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  if (sorted.length === 0) return Number.NaN;
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle]! : (sorted[middle - 1]! + sorted[middle]!) / 2;
}

/** Theil–Sen's slope of `points`, each `[x, y]`: NaN for fewer than two at distinct x. */
export function slopeOf(points: readonly (readonly [number, number])[]): number {
  const slopes: number[] = [];
  for (let i = 0; i < points.length; i++) {
    for (let j = i + 1; j < points.length; j++) {
      const [[x1, y1], [x2, y2]] = [points[i]!, points[j]!];
      if (x2 !== x1) slopes.push((y2 - y1) / (x2 - x1));
    }
  }
  return median(slopes);
}

/**
 * The series after its first quarter: a page's memory grows as it starts,
 * its caches filling, before it is steady.
 */
export function steady<T>(series: readonly T[]): T[] {
  return series.slice(Math.floor(series.length / 4));
}
