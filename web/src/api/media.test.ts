// The media plane (ADR-0006), typed by hand: what each route is sent, and
// how its answers are read. The contract tests (tests/e2e/contract.spec.ts)
// check each against a real server.
import { afterEach, describe, expect, it, vi } from "vitest";

import { Server } from "./client.ts";
import {
  brick,
  descriptorOf,
  download,
  fileAddress,
  slice,
  tileAddress,
  voxelsOf,
  zoomDescriptor,
  type StoredImage,
} from "./media.ts";

const on = new Server("http://127.0.0.1:8211");
const image: StoredImage = { project: "p 1", kind: "acquisitions", id: "r/2" };
const DZI =
  '<?xml version="1.0" encoding="UTF-8"?>\n<Image xmlns="http://schemas.microsoft.com/deepzoom/2008" ' +
  'TileSize="256" Overlap="0" Format="jpg"><Size Width="5472" Height="3648"/></Image>';

/** A slice's or a brick's bytes, as the server makes them (ADR-0029). */
function voxelBytes(bytes: 1 | 2 | 4, samples: number[], voxel = [0, 0, 0]): ArrayBuffer {
  const size = [samples.length / 2, 2, 1];
  const buffer = new ArrayBuffer(64 + samples.length * bytes);
  const view = new DataView(buffer);
  [..."OSV1"].forEach((letter, n) => view.setUint8(n, letter.charCodeAt(0)));
  view.setUint16(4, 64, true);
  view.setUint8(6, bytes);
  view.setUint8(7, 1);
  view.setUint32(8, 2, true);
  size.forEach((part, n) => view.setUint32(12 + 4 * n, part, true));
  [4, 5, 6].forEach((part, n) => view.setUint32(24 + 4 * n, part, true));
  voxel.forEach((part, n) => view.setFloat32(36 + 4 * n, part, true));
  samples.forEach((sample, n) => {
    const at = 64 + n * bytes;
    if (bytes === 1) view.setUint8(at, sample);
    else if (bytes === 2) view.setUint16(at, sample, true);
    else view.setFloat32(at, sample, true);
  });
  return buffer;
}

/** Stands in for the network, each request answered by `answer`. */
function network(answer: () => Response): Request[] {
  const made: Request[] = [];
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    made.push(input instanceof Request ? input : new Request(input, init));
    return answer();
  });
  return made;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("voxels", () => {
  it.each([
    [1, Uint8Array, [0, 1, 254, 255]],
    [2, Uint16Array, [0, 1, 65534, 65535]],
    [4, Float32Array, [0, 0.5, 1.5, -2]],
  ] as const)("of %i bytes each are read with where they are", (bytes, Samples, values) => {
    const voxels = voxelsOf(voxelBytes(bytes, [...values]));
    expect(voxels.samples).toBeInstanceOf(Samples);
    expect([...voxels.samples]).toEqual(values);
    expect(voxels).toMatchObject({ level: 2, size: [2, 2, 1], origin: [4, 5, 6], channels: 1 });
  });

  it("say a voxel's size in µm when it is known, and none when it is 0", () => {
    expect(voxelsOf(voxelBytes(1, [1, 2, 3, 4], [0.25, 0.25, 2])).voxelUm).toEqual([0.25, 0.25, 2]);
    expect(voxelsOf(voxelBytes(1, [1, 2, 3, 4])).voxelUm).toBeNull();
  });

  it("are refused when they aren't: another start, cut short, or an unknown sample", () => {
    const other = voxelBytes(1, [1, 2, 3, 4]);
    new DataView(other).setUint8(3, "2".charCodeAt(0));
    expect(() => voxelsOf(other)).toThrow("these aren't voxels: they start OSV2");
    expect(() => voxelsOf(voxelBytes(2, [1, 2, 3, 4]).slice(0, 70))).toThrow("cut short");
    const odd = voxelBytes(1, [1, 2, 3, 4]);
    new DataView(odd).setUint8(6, 3);
    expect(() => voxelsOf(odd)).toThrow("a sample of 3 bytes isn't known");
  });
});

describe("deep zoom", () => {
  it("reads a descriptor's size, tiles and format", () => {
    expect(descriptorOf(DZI)).toEqual({
      width: 5472,
      height: 3648,
      tileSize: 256,
      overlap: 0,
      format: "jpg",
    });
    expect(() => descriptorOf("<Image/>")).toThrow("the descriptor has no Size Width");
  });

  it("gives a tile's address, each part escaped, its look in its query", () => {
    const address = tileAddress(
      image,
      { level: 12, col: 3, row: 4 },
      { black: 0.1, white: 0.9 },
      on,
    );
    expect(address).toBe(
      "http://127.0.0.1:8211/api/v1/zoom/p%201/acquisitions/r%2F2/image_files/12/3_4.jpg?black=0.1&white=0.9",
    );
    expect(tileAddress(image, { level: 0, col: 0, row: 0 }, {}, on)).not.toContain("?");
  });

  it("fetches an image's descriptor", async () => {
    const made = network(
      () => new Response(DZI, { headers: { "content-type": "application/xml" } }),
    );
    expect((await zoomDescriptor(image, { on })).width).toBe(5472);
    expect(new URL(made[0]!.url).pathname).toBe("/api/v1/zoom/p%201/acquisitions/r%2F2/image.dzi");
  });
});

describe("volumes", () => {
  it("are asked for a slice, or a brick, with what is given, and their voxels read", async () => {
    const made = network(() => new Response(voxelBytes(1, [1, 2, 3, 4])));
    const one = await slice(image, { axis: "z", index: 0, level: 4, format: "u8" }, { on });
    expect([...one.samples]).toEqual([1, 2, 3, 4]);
    await brick(image, { level: 4 }, { on });
    await brick(image, {}, { on });
    const asked = made.map((request) => {
      const url = new URL(request.url);
      return url.pathname.split("/").at(-1) + url.search;
    });
    expect(asked).toEqual(["slice?axis=z&index=0&level=4&format=u8", "brick?level=4", "brick"]);
  });
});

describe("a project's files", () => {
  it("are addressed by their path, each part escaped, empty parts dropped", () => {
    expect(fileAddress("p 1", "/acquisitions/r 2//thumbnail.jpg", on)).toBe(
      "http://127.0.0.1:8211/api/v1/data/p%201/acquisitions/r%202/thumbnail.jpg",
    );
  });

  it("are downloaded whole, or a range of their bytes, its place read from the answer", async () => {
    const part = () =>
      new Response(new Uint8Array(16), {
        status: 206,
        headers: { "content-type": "image/jpeg", "content-range": "bytes 0-15/1793" },
      });
    const made = network(part);
    const got = await download("p", "a/thumbnail.jpg", { on, range: { from: 0, to: 15 } });
    expect(got.range).toEqual({ from: 0, to: 15, of: 1793 });
    expect([got.bytes.byteLength, got.type]).toEqual([16, "image/jpeg"]);
    await download("p", "a/thumbnail.jpg", { on, range: { from: 16 } });
    expect(made.map((request) => request.headers.get("range"))).toEqual([
      "bytes=0-15",
      "bytes=16-",
    ]);
    network(() => new Response(new Uint8Array(4)));
    expect((await download("p", "a/thumbnail.jpg", { on })).range).toBeUndefined();
  });
});
