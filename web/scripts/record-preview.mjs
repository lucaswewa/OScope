// Messages from the preview's stream, recorded for the reader's tests
// (src/api/preview/message.test.ts): from a server of the script's own
// (tests/e2e/server.ts), small, read as a client reads them. One plain; one
// of a region; and one with statistics and a saturation mask, the camera's
// exposure doubled until part of the frame saturates.
//
//   node scripts/record-preview.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { readMessage } from "../src/api/preview/message.ts";
import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(new URL("../src/api/preview/recorded-messages.json", import.meta.url));
/** A region of the sensor, in whole cells. */
const REGION = { x: 1024, y: 768, width: 256, height: 192 };

const server = new Server();
await server.start();

/**
 * Calls the server, and fails on any answer but `status`.
 * @param {string} method
 * @param {string} path
 * @param {unknown} body
 * @param {number} status
 */
async function call(method, path, body, status) {
  const reply = await fetch(`${server.url}/api/v1/${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (reply.status !== status) throw new Error(`${path}: ${reply.status} ${await reply.text()}`);
}

/**
 * The first message on a stream of its own that `wanted` takes, each
 * message acknowledged.
 * @param {Record<string, unknown>} ask
 * @param {(message: ReturnType<typeof readMessage>) => boolean} wanted
 * @returns {Promise<ArrayBuffer>}
 */
function first(ask, wanted) {
  const socket = new WebSocket(`${server.url.replace(/^http/, "ws")}/api/v1/preview/stream`);
  socket.binaryType = "arraybuffer";
  return new Promise((done, failed) => {
    const timer = setTimeout(
      () => failed(new Error(`nothing wanted came: ${JSON.stringify(ask)}`)),
      60_000,
    );
    socket.onopen = () => socket.send(JSON.stringify({ ...ask, ack: 0 }));
    socket.onerror = () => failed(new Error("the stream failed"));
    socket.onmessage = (event) => {
      if (typeof event.data === "string") return failed(new Error(event.data));
      const message = readMessage(event.data);
      if (wanted(message)) {
        clearTimeout(timer);
        socket.close();
        return done(event.data);
      }
      socket.send(JSON.stringify({ ...ask, ack: message.header.sequence + 1 }));
    };
  });
}

try {
  const reply = await fetch(`${server.url}/api/v1/camera/exposure`);
  const normal = /** @type {number} */ (await reply.json());
  await call("POST", "camera/start_streaming", {}, 201);
  const small = { width: 96, height: 64, quality: 50 };
  const plain = await first(small, () => true);
  const region = await first({ ...small, roi: REGION }, (m) => m.header.roi.x === REGION.x);
  // An exposure at which part of the frame saturates, not all of it: a frame
  // all at full scale has no focus to measure. The simulated brightfield is
  // nearly even, so it lies between the camera's own and twice that, found
  // by halving the range.
  let [low, high] = [normal, normal * 2];
  let [exposure, stats] = [normal, /** @type {ArrayBuffer | undefined} */ (undefined)];
  for (let step = 0; !stats; step++) {
    if (step === 16) throw new Error("no exposure saturated part of the frame");
    exposure = (low + high) / 2;
    await call("POST", "camera/stop_streaming", {}, 201);
    await call("PUT", "camera/exposure", exposure, 201);
    await call("POST", "camera/start_streaming", {}, 201);
    const asked = { ...small, stats: { bins: 16, mask: true } };
    const got = await first(asked, (m) => m.stats !== undefined);
    const most = Math.max(...(readMessage(got).stats?.saturated ?? [0]));
    if (most === 0) low = exposure;
    else if (most >= 0.5) high = exposure;
    else stats = got;
  }
  /** @param {ArrayBuffer} buffer */
  const encoded = (buffer) => Buffer.from(buffer).toString("base64");
  const messages = { plain: encoded(plain), stats: encoded(stats), region: encoded(region) };
  writeFileSync(
    OUT,
    `${JSON.stringify({ exposure_ms: exposure, region: REGION, messages }, null, 2)}\n`,
  );
  console.log(`recorded ${Object.keys(messages).length} messages in ${OUT}`);
} finally {
  await server.dispose();
}
