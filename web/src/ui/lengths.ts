// Round lengths (W3.8): a scale bar's, 1, 2 or 5 times a power of ten, for
// the live viewer (W3.1) and the tile viewer alike.

/** The longest length of 1, 2 or 5 times a power of ten, in µm, that is at most `most`. */
export function roundLength(most: number): number {
  const power = 10 ** Math.floor(Math.log10(most));
  return [5, 2, 1].map((k) => k * power).find((length) => length <= most * (1 + 1e-9)) ?? power;
}
