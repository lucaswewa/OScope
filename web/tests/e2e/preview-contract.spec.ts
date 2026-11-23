// Covers: web/src/api
// The preview's stream (W1.5) against a real server, read as the worker
// reads it: frames arrive, numbered, each a JPEG of the size its header
// says; a region gets the pixels asked for; statistics come when asked;
// what the app asks for is what the server describes; and the worker's own
// stream, decoding slowly, gets the newest frames.
import { act } from "../../src/api/actions.ts";
import { call, Server } from "../../src/api/client.ts";
import { readMessage, type PreviewMessage } from "../../src/api/preview/message.ts";
import type { PreviewParams, StatsRequest } from "../../src/api/preview/params.ts";
import { PreviewStream, type StreamSocket } from "../../src/api/preview/stream.ts";
import { expect, test } from "./fixtures.ts";

/**
 * Messages from a stream of the test's own, each acknowledged as the page
 * does once it has drawn it, until `enough` says so.
 */
function messages(
  url: string,
  ask: Omit<PreviewParams, "ack">,
  enough: (got: PreviewMessage[]) => boolean,
): Promise<PreviewMessage[]> {
  const socket = new WebSocket(`${url.replace(/^http/, "ws")}/api/v1/preview/stream`);
  socket.binaryType = "arraybuffer";
  const got: PreviewMessage[] = [];
  return new Promise((done, failed) => {
    socket.onopen = () => socket.send(JSON.stringify({ ...ask, ack: 0 }));
    socket.onerror = () => failed(new Error("the stream failed"));
    socket.onmessage = (event) => {
      if (typeof event.data === "string") return failed(new Error(event.data));
      const message = readMessage(event.data as ArrayBuffer);
      got.push(message);
      if (enough(got)) {
        socket.close();
        return done(got);
      }
      socket.send(JSON.stringify({ ...ask, ack: message.header.sequence + 1 }));
    };
  });
}

/** A JPEG's width and height, from its frame's header. */
function jpegSize(jpeg: Uint8Array): [number, number] {
  for (let at = 2; at + 9 < jpeg.length; at += 2 + ((jpeg[at + 2]! << 8) | jpeg[at + 3]!)) {
    const marker = jpeg[at + 1]!;
    if (jpeg[at] === 0xff && marker >= 0xc0 && marker <= 0xc3) {
      return [(jpeg[at + 7]! << 8) | jpeg[at + 8]!, (jpeg[at + 5]! << 8) | jpeg[at + 6]!];
    }
  }
  throw new Error("no frame header in the JPEG");
}

/** The camera's sensor, and the camera started. */
async function streaming(on: Server): Promise<{ width: number; height: number }> {
  const capabilities = await call((signal) =>
    on.api.GET("/api/v1/camera/capabilities", { signal }),
  );
  await act("/api/v1/camera/start_streaming", {}, { on });
  return { width: capabilities.sensor_width, height: capabilities.sensor_height };
}

test("frames arrive numbered, each a JPEG of the size its header says", async ({ server }) => {
  const sensor = await streaming(new Server(server.url));
  const got = await messages(
    server.url,
    { width: 640, height: 480, quality: 70 },
    (m) => m.length === 3,
  );
  expect(got.map((m) => m.header.sequence)).toEqual([0, 1, 2]);
  for (const { header, jpeg } of got) {
    expect(header.roi).toEqual({ x: 0, y: 0, ...sensor });
    expect(jpegSize(jpeg)).toEqual([header.width, header.height]);
    expect(header.width <= 640 && header.height <= 480).toBe(true);
    expect(header.quality).toBe(70);
  }
  expect(got[2]!.header.frame > got[0]!.header.frame).toBe(true);
});

test("a region of the sensor gets the pixels asked for, at their own size", async ({ server }) => {
  await streaming(new Server(server.url));
  const roi = { x: 2000, y: 1000, width: 800, height: 600 };
  const [first] = await messages(
    server.url,
    { width: 1920, height: 1280, roi },
    (m) => m.length === 1,
  );
  expect(first!.header.roi).toEqual(roi);
  expect(jpegSize(first!.jpeg)).toEqual([800, 600]);
});

test("statistics come with a frame when asked for", async ({ server }) => {
  await streaming(new Server(server.url));
  const ask = { width: 320, height: 240, stats: { bins: 32 } };
  const got = await messages(server.url, ask, (m) => m.some((one) => one.stats !== undefined));
  const stats = got.at(-1)!.stats!;
  expect(stats.channels.length).toBeGreaterThan(0);
  expect(stats.histogram.map((bins) => bins.length)).toEqual(stats.channels.map(() => 32));
  expect(stats.counted.every((count) => count > 0)).toBe(true);
  expect(stats.focus).toBeGreaterThan(0);
});

test("what the app asks for is what the server describes", async ({ server }) => {
  // Each key of the app's parameters: a key added there must be added here.
  const sent: Required<PreviewParams> = {
    width: 1,
    height: 1,
    roi: { x: 0, y: 0, width: 1, height: 1 },
    quality: 1,
    max_fps: 1,
    ack: 0,
    stats: {},
  };
  const measured: Required<StatsRequest> = {
    roi: sent.roi,
    profile: { from: [0, 0], to: [1, 1] },
    bins: 1,
    mask: true,
  };
  const described = (await (await fetch(`${server.url}/api/v1/preview/`)).json()) as {
    links: Array<{ subprotocol?: string; parameters?: { properties: Record<string, unknown> } }>;
  };
  const schema = described.links.find((link) => link.subprotocol === "websocket")?.parameters;
  expect(Object.keys(schema?.properties ?? {})).toEqual(expect.arrayContaining(Object.keys(sent)));
  // The statistics' request, wherever the schema keeps it.
  const text = JSON.stringify(schema);
  for (const key of Object.keys(measured)) expect(text).toContain(`"${key}"`);
});

test("the worker's stream, decoding slowly, gets the newest frames, not a backlog", async ({
  server,
}) => {
  test.setTimeout(120_000);
  // A region of the camera, which keeps its rate, for an eighth of the
  // simulator's work: the pace below follows the camera.
  const on = new Server(server.url);
  const roi = { x: 1776, y: 1176, width: 1920, height: 1272 };
  await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: roi, signal }));
  await streaming(on);
  // A decoder that takes three of the camera's frame intervals for each
  // frame, one at a time: slower than the camera, however busy the computer
  // is. The interval is read from the first two frames' exposures.
  let pace = 0;
  let previous: bigint | undefined;
  let queue = Promise.resolve();
  const frames: bigint[] = [];
  let enough = () => {};
  const done = new Promise<void>((resolve) => (enough = resolve));
  const stream = new PreviewStream({
    socket: (url) => new WebSocket(url) as unknown as StreamSocket,
    decode: () => {
      const decoded = queue.then(
        () =>
          new Promise<ImageBitmap>((resolve) => {
            setTimeout(() => resolve({ close() {} } as ImageBitmap), pace);
          }),
      );
      queue = decoded.then(() => undefined);
      return decoded;
    },
    post: (message) => {
      if (message.kind !== "frame") return;
      const { exposureEnd } = message.header;
      if (previous !== undefined && pace === 0) {
        pace = Math.min(5_000, Math.max(100, (3 * Number(exposureEnd - previous)) / 1e6));
      }
      previous = exposureEnd;
      // The page draws it at once, and says so.
      frames.push(message.header.frame);
      stream.handle({ kind: "drawn", sequence: message.header.sequence });
      if (frames.length === 6) enough();
    },
    now: () => Date.now(),
  });
  const url = `${server.url.replace(/^http/, "ws")}/api/v1/preview/stream`;
  stream.handle({ kind: "open", url, ask: { width: 320, height: 240, quality: 50 } });
  await done;
  const [session] = await (await fetch(`${server.url}/api/v1/preview/sessions`)).json();
  stream.handle({ kind: "close" });
  // Acknowledged, the server waited, and skipped the frames that came
  // between: each decoded is the newest, and the frames' numbers run ahead
  // of the count decoded. Without, they would queue behind the decoder.
  expect(session.dropped).toBeGreaterThan(0);
  expect(Number(frames.at(-1)! - frames[0]!)).toBeGreaterThan(frames.length);
});
// A separate required-device case; the launcher rejects CPU fallback before tests.
test("a required CUDA preview reaches the client with GPU session evidence @gpu", async ({
  server,
}) => {
  test.skip(process.env.OSCOPE_E2E_GPU !== "1", "run the explicit GPU integration gate");
  const on = new Server(server.url);
  await call((signal) =>
    on.api.PUT("/api/v1/camera/roi", {
      body: { x: 1776, y: 1176, width: 1920, height: 1280 },
      signal,
    }),
  );
  await streaming(on);
  const socket = new WebSocket(server.url.replace(/^http/, "ws") + "/api/v1/preview/stream");
  socket.binaryType = "arraybuffer";
  try {
    const received = new Promise<PreviewMessage>((done, failed) => {
      socket.onopen = () =>
        socket.send(JSON.stringify({ width: 320, height: 240, quality: 70, ack: 0 }));
      socket.onerror = () => failed(new Error("GPU preview connection failed"));
      socket.onmessage = (event) => done(readMessage(event.data as ArrayBuffer));
    });
    const got = await received;
    expect(jpegSize(got.jpeg)).toEqual([got.header.width, got.header.height]);
    await expect
      .poll(async () => {
        const sessions = await (await fetch(server.url + "/api/v1/preview/sessions")).json();
        return sessions.some((s: { gpu: boolean; sent: number }) => s.gpu && s.sent > 0);
      })
      .toBe(true);
  } finally {
    socket.close();
  }
});
