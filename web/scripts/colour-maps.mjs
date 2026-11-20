// The false-colour maps' tables (W3.6), made from their published data and
// written as 256 sRGB colours each, as hex, for the renderer's texture:
//
// - viridis, magma, inferno and plasma: Nathaniel Smith and Stéfan van der
//   Walt's, for matplotlib, under CC0, from github.com/BIDS/colormap
//   (`colormaps.py`);
// - turbo: Anton Mikhailov's, Copyright 2019 Google LLC, under the Apache
//   License 2.0, from his gist `ee72ba4191942acecc03fe6da94fc73f`.
//
// The sources are read as text, their numbers taken by pattern: nothing in
// them is run. Grey and HiLo are the renderer's own.
//
//   node scripts/colour-maps.mjs BIDS-colormaps.py TURBO.py
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const OUT = fileURLToPath(new URL("../src/features/live/colour-maps.json", import.meta.url));
const [bids, turbo] = process.argv.slice(2);
if (!bids || !turbo) throw new Error("usage: colour-maps.mjs BIDS-colormaps.py TURBO.py");

/**
 * The 256 [r, g, b] triples assigned to `name` in `text`, each from 0 to 1,
 * as hex.
 *
 * @param {string} text
 * @param {string} name
 * @returns {string}
 */
function table(text, name) {
  const start = text.indexOf(`${name} = [`);
  if (start < 0) throw new Error(`${name} isn't there`);
  const end = text.indexOf("]]", start);
  const found = text.slice(start + name.length, end).match(/-?\d+(\.\d+)?(e-?\d+)?/g) ?? [];
  /** @type {number[]} */
  const numbers = found.map(Number);
  if (numbers.length !== 256 * 3 || numbers.some((n) => !(n >= 0 && n <= 1))) {
    throw new Error(`${name}: ${numbers.length} numbers, not 768 from 0 to 1`);
  }
  // Each as an 8-bit sRGB value, rounded: two hex digits a channel.
  return numbers
    .map((n) =>
      Math.round(n * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("");
}

const fromBids = readFileSync(bids, "utf8");
const fromTurbo = readFileSync(turbo, "utf8");
const maps = {
  viridis: table(fromBids, "_viridis_data"),
  magma: table(fromBids, "_magma_data"),
  inferno: table(fromBids, "_inferno_data"),
  plasma: table(fromBids, "_plasma_data"),
  turbo: table(fromTurbo, "turbo_colormap_data"),
};
const sources = {
  viridis: "Nathaniel Smith and Stéfan van der Walt, CC0 (github.com/BIDS/colormap)",
  magma: "Nathaniel Smith and Stéfan van der Walt, CC0 (github.com/BIDS/colormap)",
  inferno: "Nathaniel Smith and Stéfan van der Walt, CC0 (github.com/BIDS/colormap)",
  plasma: "Nathaniel Smith and Stéfan van der Walt, CC0 (github.com/BIDS/colormap)",
  turbo: "Anton Mikhailov, Copyright 2019 Google LLC, Apache License 2.0",
};
writeFileSync(OUT, `${JSON.stringify({ sources, maps }, null, 2)}\n`);
console.log(`colour-maps: wrote ${OUT}`);
