// The calibration wizard's states, recorded for the unit test of its display
// (W8.3): a wizard of every step on the simulator's tissue, on a server of
// the script's own (tests/e2e/server.ts), prompting, running, its objective
// check and geometry done, its flat field failed over the tissue, retried,
// skipped, and cancelled; and a wizard of one step, finished. The stage
// moves on the simulator's fast clock, as the end-to-end tests do.
//
//   node scripts/record-wizards.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(
  new URL("../src/features/settings/calibration/wizard/recorded-wizards.json", import.meta.url),
);

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
  const reply = await fetch(new URL(path.startsWith("/") ? path : `/api/v1/${path}`, server.url), {
    method,
    headers: { "content-type": "application/json" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const text = await reply.text();
  if (!reply.ok) throw new Error(`${method} ${path}: ${reply.status} ${text}`);
  return text ? JSON.parse(text) : null;
}

/**
 * An action, run to its end: its output.
 * @param {string} path
 * @param {unknown} [input]
 */
async function act(path, input = {}) {
  let invocation = await request("POST", path, input);
  while (["pending", "running"].includes(invocation.status)) {
    await new Promise((done) => setTimeout(done, 50));
    invocation = await request("GET", invocation.href);
  }
  if (invocation.status !== "completed") {
    throw new Error(`${path}: ${JSON.stringify(invocation.error)}`);
  }
  return invocation.output;
}

/**
 * The wizard once `ended` takes it, read every 200 ms, for at most 2 min.
 * @param {(wizard: any) => boolean} ended
 */
async function until(ended) {
  const deadline = Date.now() + 120_000;
  for (;;) {
    const wizard = await request("GET", "calibration/wizard");
    if (ended(wizard)) return wizard;
    if (Date.now() > deadline) throw new Error(`the wizard didn't move: ${JSON.stringify(wizard)}`);
    await new Promise((done) => setTimeout(done, 200));
  }
}

/** @type {Record<string, unknown>} */
const recorded = {};
const settled = (/** @type {number} */ i) => (/** @type {any} */ w) =>
  w.steps[i].state.state !== "running";
try {
  await request("PUT", "simulation/rate", 50);
  await act("stage/home");
  await act("stage/move_to", { x: 40_000, y: 12_000, z: 5_000, speed: 25_000 });
  await request("PUT", "simulation/rate", 1);
  await request("PUT", "camera/roi", { x: 2256, y: 1500, width: 960, height: 648 });
  await act("camera/start_streaming");
  recorded.prompting = await act("calibration/wizard_start");
  recorded.running = await act("calibration/wizard_confirm");
  recorded.objectiveDone = await until(settled(0));
  await act("calibration/wizard_confirm");
  recorded.geometryDone = await until(settled(1));
  await act("calibration/wizard_confirm");
  await until(settled(2));
  // The flat field, over the tissue, isn't blank.
  await act("calibration/wizard_confirm");
  recorded.flatFailed = await until(settled(3));
  recorded.retried = await act("calibration/wizard_retry");
  recorded.skipped = await act("calibration/wizard_skip");
  recorded.cancelled = await act("calibration/wizard_cancel");
  await act("calibration/wizard_start", { steps: ["dark"] });
  await act("calibration/wizard_confirm");
  recorded.finished = await until((w) => w.finished);
  await act("camera/stop_streaming");
  writeFileSync(OUT, `${JSON.stringify(recorded, null, 2)}\n`);
  console.log(`recorded ${Object.keys(recorded).join(", ")} in ${OUT}`);
} finally {
  await server.dispose();
}
