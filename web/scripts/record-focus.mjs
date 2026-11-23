// Autofocus's report, recorded for the unit test of its curve (W3.7): a
// stepped run over the simulator's tissue, from 12 µm above its focus, on a
// server of the script's own (tests/e2e/server.ts). As the end-to-end tests
// do (tests/e2e/sim.ts), the stage moves on the simulator's fast clock, and
// the run is on its own. The API is called as it is: the app's client is
// more TypeScript than Node runs.
//
//   node scripts/record-focus.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(
  new URL("../src/features/control/focus/recorded-last.json", import.meta.url),
);
const TISSUE = { x: 40_000, y: 12_000 };
const REGION = { x: 2256, y: 1500, width: 960, height: 648 };

const server = new Server();
await server.start();

/**
 * What the server answers, as JSON: anything but a success fails.
 * @param {string} method
 * @param {string} path
 * @param {unknown} [body]
 * @returns {Promise<any>}
 */
async function request(method, path, body) {
  const address = new URL(path.startsWith("/") ? path : `/api/v1/${path}`, server.url);
  const reply = await fetch(address, {
    method,
    headers: { "content-type": "application/json" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await reply.text();
  if (!reply.ok) throw new Error(`${method} ${path}: ${reply.status} ${text}`);
  return text ? JSON.parse(text) : null;
}

/**
 * What `path` gives once `ended` takes it, read every 100 ms, for at most `ms`.
 * @param {string} path
 * @param {(now: any) => boolean} ended
 * @param {number} ms
 */
async function until(path, ended, ms) {
  const deadline = Date.now() + ms;
  for (;;) {
    const now = await request("GET", path);
    if (ended(now)) return now;
    if (Date.now() > deadline) throw new Error(`${path} didn't end: ${JSON.stringify(now)}`);
    await new Promise((done) => setTimeout(done, 100));
  }
}

/**
 * An action, run to its end: its output.
 * @param {string} path
 * @param {unknown} input
 */
async function act(path, input) {
  const started = await request("POST", path, input);
  const href = new URL(started.href, server.url).pathname;
  const ended = ["completed", "error", "cancelled"];
  const done = await until(href, (now) => ended.includes(now.status), 120_000);
  if (done.status !== "completed") throw new Error(`${path}: ${JSON.stringify(done)}`);
  return done.output;
}

try {
  await request("PUT", "camera/roi", REGION);
  await request("PUT", "simulation/rate", 50);
  await act("stage/home", {});
  const truth = await act("simulation/in_focus", TISSUE);
  await act("stage/move_to", { ...TISSUE, z: truth + 12, speed: 25_000 });
  await request("PUT", "simulation/rate", 1);
  await act("camera/start_streaming", {});
  const job = await act("autofocus/run", { sweep: "stepped" });
  const ended = ["completed", "failed", "cancelled", "interrupted"];
  const run = await until(`jobs/job/${job.id}`, (now) => ended.includes(now.state), 240_000);
  const last = await request("GET", "autofocus/last");
  if (run.state !== "completed" || !last?.found) {
    throw new Error(`no focus found: ${JSON.stringify(run)} ${JSON.stringify(last)}`);
  }
  writeFileSync(OUT, `${JSON.stringify(last, null, 2)}\n`);
  console.log(`record-focus: wrote ${OUT}, ${(last.z_um - truth).toFixed(2)} µm from the truth`);
} finally {
  await server.dispose();
}
