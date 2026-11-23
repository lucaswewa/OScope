// The false-colour maps (W3.6): seven, each 256 sRGB colours for the
// renderer's texture, applied on the image's luminance. Grey and HiLo are
// made here; viridis, magma, inferno, plasma and turbo come from their
// published tables, as `scripts/colour-maps.mjs` wrote them.
import tables from "./colour-maps.json";

/** The image's own colours, then the seven maps. */
export const MAPS = [
  "none",
  "grey",
  "viridis",
  "magma",
  "inferno",
  "plasma",
  "turbo",
  "hilo",
] as const;
export type MapName = (typeof MAPS)[number];

const isTabled = (name: MapName): name is keyof typeof tables.maps => name in tables.maps;

/** A map's 256 colours, three bytes each: none for the image's own colours. */
export function tableOf(name: MapName): Uint8Array | undefined {
  if (name === "none") return undefined;
  const table = new Uint8Array(256 * 3);
  if (isTabled(name)) {
    const hex = tables.maps[name];
    for (let i = 0; i < table.length; i++) table[i] = parseInt(hex.slice(2 * i, 2 * i + 2), 16);
    return table;
  }
  for (let level = 0; level < 256; level++) table.fill(level, level * 3, level * 3 + 3);
  // HiLo: grey, but the darkest blue and the brightest red, to set the levels by.
  if (name === "hilo") {
    table.set([0, 0, 255], 0);
    table.set([255, 0, 0], 255 * 3);
  }
  return table;
}

/** Where a map's table comes from, for the notes and the About tab. */
export function sourceOf(name: MapName): string | undefined {
  return isTabled(name) ? tables.sources[name] : undefined;
}
