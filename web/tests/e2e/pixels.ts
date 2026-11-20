// What screenshots of the live image show (W3.3, W3.4): a square of one as
// greys, read in the page, which decodes the PNG; a patch found again in
// another by normalised cross-correlation; and the greys' mean.
import type { Locator, Page } from "@playwright/test";

/** A square of the screenshot `png`, as greys, `side` CSS pixels about `at`. */
export async function greys(
  page: Page,
  png: Buffer,
  at: [number, number],
  side: number,
): Promise<number[]> {
  return page.evaluate(
    async ({ base64, x, y, side }) => {
      const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
      const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
      // An ordinary canvas: WebKit's build has no OffscreenCanvas.
      const canvas = document.createElement("canvas");
      [canvas.width, canvas.height] = [bitmap.width, bitmap.height];
      const drawn = canvas.getContext("2d")!;
      drawn.drawImage(bitmap, 0, 0);
      const { data } = drawn.getImageData(x, y, side, side);
      return Array.from(
        { length: side * side },
        (_, i) => data[4 * i]! + data[4 * i + 1]! + data[4 * i + 2]!,
      );
    },
    {
      base64: png.toString("base64"),
      x: Math.round(at[0] - side / 2),
      y: Math.round(at[1] - side / 2),
      side,
    },
  );
}

/**
 * Where the patch `wanted` (`side` square) is best found in `area` (`side + 2 · reach`
 * square), by normalised cross-correlation: its offset from the area's middle, and how well.
 */
export function found(wanted: number[], area: number[], side: number, reach: number) {
  const span = side + 2 * reach;
  const w = mean(wanted);
  const wd = wanted.map((v) => v - w);
  const wn = Math.sqrt(wd.reduce((sum, v) => sum + v * v, 0));
  let best = { dx: 0, dy: 0, score: -Infinity };
  for (let dy = -reach; dy <= reach; dy++) {
    for (let dx = -reach; dx <= reach; dx++) {
      const patch: number[] = [];
      for (let row = 0; row < side; row++) {
        const start = (row + reach + dy) * span + reach + dx;
        patch.push(...area.slice(start, start + side));
      }
      const p = mean(patch);
      let dot = 0;
      let pn = 0;
      patch.forEach((v, i) => {
        dot += (v - p) * wd[i]!;
        pn += (v - p) ** 2;
      });
      const score = dot / (wn * Math.sqrt(pn) || 1);
      if (score > best.score) best = { dx, dy, score };
    }
  }
  return best;
}

/** The greys' mean. */
export function mean(values: number[]): number {
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

/**
 * An element's pixels, every seventh, each [r, g, b], from a screenshot of it in CSS
 * pixels: a quarter of WebKit's own, which took it 12 to 18 s a sample on two cores (W3.6).
 */
export async function rgbOf(page: Page, element: Locator): Promise<number[][]> {
  const png = await element.screenshot({ scale: "css" });
  return page.evaluate(async (base64) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
    const canvas = document.createElement("canvas");
    [canvas.width, canvas.height] = [bitmap.width, bitmap.height];
    const drawn = canvas.getContext("2d")!;
    drawn.drawImage(bitmap, 0, 0);
    const { data } = drawn.getImageData(0, 0, bitmap.width, bitmap.height);
    const out: number[][] = [];
    for (let i = 0; i < data.length; i += 4 * 7) out.push([data[i]!, data[i + 1]!, data[i + 2]!]);
    return out;
  }, png.toString("base64"));
}

/** How coloured pixels are: the mean of each one's highest channel less its lowest. */
export const colourOf = (pixels: number[][]): number =>
  mean(pixels.map(([r, g, b]) => Math.max(r!, g!, b!) - Math.min(r!, g!, b!)));
