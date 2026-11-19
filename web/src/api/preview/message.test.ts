// The preview's messages (W1.5), read: three recorded from a server
// (scripts/record-preview.mjs), and others made by hand to break each rule.
import { describe, expect, it } from "vitest";

import { previewMessage } from "../../../tests/setup/stand-ins.ts";
import {
  isSaturated,
  KIND,
  PreviewError,
  readMessage,
  saturatedPixels,
  type PreviewMessage,
} from "./message.ts";
import recorded from "./recorded-messages.json";

/** A recorded message's bytes. */
function bytes(name: keyof typeof recorded.messages): ArrayBuffer {
  const binary = atob(recorded.messages[name]);
  return Uint8Array.from(binary, (letter) => letter.charCodeAt(0)).buffer;
}

/** Whether the bytes are a whole JPEG: its start of image, and its end. */
const isJpeg = ({ jpeg }: PreviewMessage) =>
  jpeg[0] === 0xff && jpeg[1] === 0xd8 && jpeg.at(-2) === 0xff && jpeg.at(-1) === 0xd9;

const text = (value: unknown) => new TextEncoder().encode(JSON.stringify(value));

describe("a preview message from the server", () => {
  it("is read: its header, and its JPEG whole", () => {
    const message = readMessage(bytes("plain"));
    const { header } = message;
    expect(isJpeg(message)).toBe(true);
    expect([header.roi.x, header.roi.y]).toEqual([0, 0]);
    // Fitted to the 96 × 64 asked for, in the sensor's shape.
    expect(Math.max(header.width / 96, header.height / 64)).toBeCloseTo(1, 1);
    expect(header.width / header.height).toBeCloseTo(header.roi.width / header.roi.height, 1);
    expect(header.exposureStart < header.exposureEnd && header.exposureEnd <= header.ready).toBe(
      true,
    );
    expect([message.stats, message.mask]).toEqual([undefined, undefined]);
  });

  it("carries the statistics asked for, and the saturation mask", () => {
    const { header, stats, mask } = readMessage(bytes("stats"));
    expect(stats?.channels.length).toBe(stats?.histogram.length);
    expect(stats?.histogram.every((bins) => bins.length === 16)).toBe(true);
    expect(stats?.percentiles.every((values) => values.length === 5)).toBe(true);
    expect(stats?.saturated.some((share) => share > 0)).toBe(true);
    expect(stats?.focus).toBeGreaterThan(0);
    expect([mask?.width, mask?.height]).toEqual([header.width, header.height]);
    expect(saturatedPixels(mask!)).toBeGreaterThan(0);
  });

  it("shows the region asked for", () => {
    const { header } = readMessage(bytes("region"));
    expect(header.roi).toEqual(recorded.region);
    expect(header.width).toBeLessThanOrEqual(96);
  });
});

describe("a preview message made by hand", () => {
  it("refuses a version this app doesn't know", () => {
    expect(() => readMessage(previewMessage({ version: 2 }))).toThrow(
      new PreviewError("a preview of version 2: this app reads version 1"),
    );
  });

  it("refuses what isn't a preview, or is cut short", () => {
    const message = previewMessage();
    new Uint8Array(message)[0] = 0x58;
    expect(() => readMessage(message)).toThrow("not a preview message");
    expect(() => readMessage(previewMessage().slice(0, 40))).toThrow("shorter than a header");
    expect(() => readMessage(previewMessage().slice(0, 78))).toThrow(
      "shorter than its header says",
    );
    expect(() => readMessage(previewMessage({ kind: KIND.mask }))).toThrow("kind 3");
  });

  it("reads ADR-0014's first header, whose JPEG runs to the end", () => {
    const { header, jpeg } = readMessage(previewMessage({ length: 72, sequence: 9 }));
    expect([header.sequence, header.frame, header.quality, jpeg.length]).toEqual([9, 42n, 80, 4]);
    expect([header.width, header.height, header.roi]).toEqual([
      320,
      240,
      { x: 10, y: 20, width: 640, height: 480 },
    ]);
  });

  it("passes over a field added to the header, and a kind of block it doesn't know", () => {
    const tiles = text({ tiles: [[0, 1, 2]] });
    const stats = text({ focus: 3 });
    const message = readMessage(
      previewMessage({
        length: 84,
        blocks: [
          [KIND.tiles, tiles],
          [9, new Uint8Array(3)],
          [KIND.stats, stats],
        ],
      }),
    );
    expect(isJpeg(message)).toBe(true);
    expect(message.stats).toEqual({ focus: 3 });
  });

  it("refuses a block that runs past its message", () => {
    const cut = previewMessage({ blocks: [[KIND.stats, text({ focus: 3 })]] }).slice(0, -2);
    expect(() => readMessage(cut)).toThrow("a block runs past its message");
  });

  it("reads the mask's rows from each byte's highest bit down", () => {
    // 10 × 2: two bytes a row; (0, 0), (9, 0) and (8, 1) saturated.
    const mask = new Uint8Array([
      10, 0, 0, 0, 2, 0, 0, 0, 0b1000_0000, 0b0100_0000, 0, 0b1000_0000,
    ]);
    const read = readMessage(previewMessage({ blocks: [[KIND.mask, mask]] })).mask!;
    expect([read.width, read.height, read.stride]).toEqual([10, 2, 2]);
    const on = [];
    for (let y = 0; y < 2; y++)
      for (let x = 0; x < 10; x++) if (isSaturated(read, x, y)) on.push([x, y]);
    expect(on).toEqual([
      [0, 0],
      [9, 0],
      [8, 1],
    ]);
    expect(saturatedPixels(read)).toBe(3);
    expect(() => readMessage(previewMessage({ blocks: [[KIND.mask, mask.slice(0, 10)]] }))).toThrow(
      "shorter than its size",
    );
  });
});
