// The built-in simulated samples' pictures (W8.4), as the simulated camera
// sees each through the 10×, on a server of the script's own
// (tests/e2e/server.ts): loaded, lit as it is meant to be seen, in focus,
// exposed by auto exposure, and captured, 240 × 160, its display rendition
// kept as a JPEG beside the samples' view. The stage moves on the simulator's fast
// clock, as the end-to-end tests do.
//
//   node scripts/record-samples.mjs    (after `cargo build -p oscope-server`)
import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(
  new URL("../src/features/settings/simulation/pictures/", import.meta.url),
);
/** 720 × 480 of the sensor's middle, binned by three: a picture of 240 × 160. */
const MIDDLE = { x: 2_376, y: 1_584, width: 720, height: 480 };
const BINNED = { mode: "bin3", format: { kind: "bayer16", pattern: "RGGB", significant_bits: 12 } };
/** Each built-in sample: where it is seen, and in which light. */
const SAMPLES = [
  { name: "H&E tissue", at: { x: 40_000, y: 12_000 }, light: "transmitted" },
  { name: "H-DAB tissue", at: { x: 40_000, y: 12_000 }, light: "transmitted" },
  { name: "test target", at: { x: 47_000, y: 12_500 }, light: "transmitted" },
  { name: "reflective surface", at: { x: 47_000, y: 12_500 }, light: "reflected" },
  { name: "fluorescent cells", at: { x: 47_000, y: 12_500 }, light: "blue 488" },
  { name: "sub-diffraction structures", at: { x: 47_000, y: 12_500 }, light: "blue 488" },
  { name: "colour target", at: { x: 47_000, y: 12_500 }, light: "transmitted" },
];
const LIGHTS = ["transmitted", "reflected", "violet 405", "blue 488", "green 561", "red 640"];

/** A sample's picture's file name: its name in lower case, words joined by `-`. */
export const fileOf = (/** @type {string} */ name) =>
  `${name.toLowerCase().replace(/[^a-z0-9]+/g, "-")}.jpg`;

const server = new Server();
await server.start();

/**
 * What the server answers: JSON, or the bytes when `bytes`; anything but a success fails.
 * @param {string} method
 * @param {string} path
 * @param {unknown} [body]
 * @param {boolean} [bytes]
 * @returns {Promise<any>}
 */
async function request(method, path, body, bytes = false) {
  const reply = await fetch(new URL(path.startsWith("/") ? path : `/api/v1/${path}`, server.url), {
    method,
    headers: { "content-type": "application/json" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  if (!reply.ok) throw new Error(`${method} ${path}: ${reply.status} ${await reply.text()}`);
  if (bytes) return Buffer.from(await reply.arrayBuffer());
  const text = await reply.text();
  return text ? JSON.parse(text) : null;
}

/**
 * An action, run to its end: its output; a job it started, followed to its end.
 * @param {string} path
 * @param {unknown} [input]
 */
async function act(path, input = {}) {
  let invocation = await request("POST", path, input);
  while (["pending", "running"].includes(invocation.status)) {
    await new Promise((done) => setTimeout(done, 50));
    invocation = await request("GET", `action_invocations/${invocation.id}`);
  }
  if (invocation.status !== "completed") {
    throw new Error(`${path}: ${JSON.stringify(invocation.error)}`);
  }
  const href = invocation.output?.href;
  if (typeof href !== "string" || !href.includes("/jobs/")) return invocation.output;
  for (const deadline = Date.now() + 300_000; ;) {
    const job = await request("GET", href);
    if (["completed", "failed", "cancelled", "interrupted"].includes(job.state)) {
      if (job.state !== "completed") throw new Error(`${path}: ${JSON.stringify(job)}`);
      return job;
    }
    if (Date.now() > deadline) throw new Error(`${path}: its job didn't end`);
    await new Promise((done) => setTimeout(done, 200));
  }
}

try {
  mkdirSync(OUT, { recursive: true });
  await act("camera/set_mode", BINNED);
  await request("PUT", "camera/roi", MIDDLE);
  await request("PUT", "simulation/rate", 50);
  await act("stage/home");
  for (const { name, at, light } of SAMPLES) {
    await act("simulation/load", { name });
    for (const channel of LIGHTS) {
      await act("illumination/set_light", { channel, on: channel === light, intensity: 1 });
    }
    const z = await act("simulation/in_focus", at);
    await request("PUT", "simulation/rate", 50);
    await act("stage/move_to", { ...at, z, speed: 25_000 });
    await request("PUT", "simulation/rate", 1);
    await act("camera/start_streaming");
    await act("exposure/auto");
    const project = (await act("projects/create", { name })).id;
    const captured = await act("capture/single", { project, name, rendition: "jpeg" });
    await act("camera/stop_streaming");
    // Its display rendition, in colour, from the data route: `data/<project>/<its path>`.
    const picture = await request("GET", captured.results[1], undefined, true);
    writeFileSync(`${OUT}${fileOf(name)}`, picture);
    console.log(`record-samples: ${name}, ${picture.length} bytes`);
  }
} finally {
  await server.dispose();
}
