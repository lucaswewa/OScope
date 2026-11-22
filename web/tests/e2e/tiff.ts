// TIFFs for the tests (W6.3, W6.4), written by hand from the TIFF 6.0
// specification's baseline fields: 16-bit greyscale, uncompressed, one strip
// a page; a stack is its pages, each with its own directory.

/** A pixel's grey, from 0 to 65 535, at `(x, y)` of page `page`. */
export type Pattern = (x: number, y: number, page: number) => number;

/** A ramp, and a page's own offset: no two pages alike. */
export const RAMP: Pattern = (x, y, page) => ((x + y) * 37 + page * 9_001) % 65_536;

/** A 16-bit greyscale TIFF of `width` × `height` pixels, of `pages` pages. */
export function tiffOf(width: number, height: number, pages = 1, pattern: Pattern = RAMP): Buffer {
  const fields = 9;
  const directory = 2 + fields * 12 + 4;
  const bytes = width * height * 2;
  const file = Buffer.alloc(8 + pages * (directory + bytes));
  file.write("II", 0, "latin1");
  file.writeUInt16LE(42, 2);
  file.writeUInt32LE(8, 4);
  for (let page = 0; page < pages; page++) {
    const at = 8 + page * (directory + bytes);
    const pixels = at + directory;
    const entries: [tag: number, type: 3 | 4, value: number][] = [
      [256, 3, width],
      [257, 3, height],
      [258, 3, 16],
      [259, 3, 1],
      [262, 3, 1],
      [273, 4, pixels],
      [277, 3, 1],
      [278, 3, height],
      [279, 4, bytes],
    ];
    file.writeUInt16LE(fields, at);
    entries.forEach(([tag, type, value], i) => {
      const entry = at + 2 + i * 12;
      file.writeUInt16LE(tag, entry);
      file.writeUInt16LE(type, entry + 2);
      file.writeUInt32LE(1, entry + 4);
      if (type === 3) file.writeUInt16LE(value, entry + 8);
      else file.writeUInt32LE(value, entry + 8);
    });
    // The next page's directory, or none.
    const next = page + 1 < pages ? pixels + bytes : 0;
    file.writeUInt32LE(next, at + 2 + fields * 12);
    for (let y = 0; y < height; y++)
      for (let x = 0; x < width; x++)
        file.writeUInt16LE(pattern(x, y, page), pixels + (y * width + x) * 2);
  }
  return file;
}
