// A scan's plan, recorded for the unit tests of its drawing and warnings
// (W5.1): a snake over 3.5 × 2.5 mm of the simulator's tissue, through the
// 10×, on a server of the script's own (tests/e2e/server.ts), with the
// stage's capabilities beside it. Nothing moves but the homing.
//
//   node scripts/record-plan.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(new URL("../src/features/scan/recorded-plan.json", import.meta.url));
const REGION = {
  rect: { origin: { x: 38_250, y: 10_750 }, size: { width: 3_500, height: 2_500 } },
};

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
  const address = new URL(`/api/v1/${path}`, server.url);
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
 * An action, run to its end: its output.
 * @param {string} path
 * @param {unknown} input
 */
async function act(path, input) {
  const started = await request("POST", path, input);
  for (let tries = 0; tries < 1200; tries++) {
    const now = await request("GET", `action_invocations/${started.id}`);
    if (now.status === "completed") return now.output;
    if (now.status === "error" || now.status === "cancelled")
      throw new Error(`${path}: ${JSON.stringify(now)}`);
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error(`${path} didn't end`);
}

try {
  await request("PUT", "simulation/rate", 50);
  await act("stage/home", {});
  await request("PUT", "simulation/rate", 1);
  const preview = await act("scan/plan", {
    request: { pattern: "snake", region: REGION, positions: [] },
  });
  const capabilities = await request("GET", "stage/capabilities");
  writeFileSync(OUT, `${JSON.stringify({ region: REGION, preview, capabilities }, null, 2)}\n`);
  console.log(`record-plan: wrote ${OUT}, ${preview.plan.tiles.length} tiles`);
} finally {
  await server.dispose();
}
