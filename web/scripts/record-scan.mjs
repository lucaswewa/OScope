// A scan's messages, recorded for the unit tests of its progress (W5.2): a
// snake of a few tiles over the simulator's tissue, through the camera's
// tiny region, on a server of the script's own (tests/e2e/server.ts). The
// scan's `progress` and its job, as they are with a tile done, paused,
// completed, and as a second scan is after a cancel.
//
//   node scripts/record-scan.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(new URL("../src/features/scan/recorded-scan.json", import.meta.url));
const TISSUE = { x: 40_000, y: 12_000 };
const TINY = { x: 2592, y: 1716, width: 288, height: 216 };
/**
 * A region over the tissue, in µm: a few tiles' for the scan paused and
 * resumed, a larger for the scan cancelled before it ends.
 * @param {number} width
 * @param {number} height
 */
const region = (width, height) => ({
  rect: { origin: { x: 39_850, y: 11_900 }, size: { width, height } },
});

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
  const reply = await fetch(new URL(`/api/v1/${path}`, server.url), {
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
  for (let tries = 0; tries < 3000; tries++) {
    const now = await request("GET", `action_invocations/${started.id}`);
    if (now.status === "completed") return now.output;
    if (now.status === "error" || now.status === "cancelled")
      throw new Error(`${path}: ${JSON.stringify(now)}`);
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error(`${path} didn't end`);
}

/**
 * What the tests read of a job: not its parameters, which hold its plan again.
 * @param {any} job
 */
const kept = ({ id, kind, state, pausable, progress, results, error }) => ({
  id,
  kind,
  state,
  pausable,
  progress,
  results,
  error,
});

/**
 * The scan's progress and its job, once `ready` says they are as wanted.
 * @param {(progress: any, job: any) => boolean} ready
 * @param {string} what
 */
async function when(ready, what) {
  for (let tries = 0; tries < 3000; tries++) {
    const progress = await request("GET", "scan/progress");
    const job = progress && (await request("GET", `jobs/job/${progress.job}`));
    if (progress && ready(progress, job)) return { progress, job: kept(job) };
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error(`no scan ${what}`);
}

try {
  await request("PUT", "simulation/rate", 50);
  await act("stage/home", {});
  const z = await act("simulation/in_focus", TISSUE);
  await act("stage/move_to", { ...TISSUE, z, speed: 25_000 });
  await request("PUT", "simulation/rate", 1);
  await request("PUT", "camera/roi", TINY);
  await act("camera/start_streaming", {});
  const project = (await act("projects/create", { name: "Scans" })).id;
  /**
   * @param {number} width
   * @param {number} height
   */
  const scan = (width, height) => ({
    request: { pattern: "snake", region: region(width, height), positions: [] },
    project,
  });

  const first = await act("scan/run", { ...scan(180, 120), name: "a few tiles" });
  const running = await when((p, j) => p.done.length >= 1 && j.state === "running", "with a tile");
  await act("jobs/pause", { id: first.id });
  const paused = await when((_, j) => j.state === "paused", "paused");
  await act("jobs/resume", { id: first.id });
  const completed = await when((_, j) => j.state === "completed", "completed");

  await act("scan/run", { ...scan(200, 150), name: "cancelled" });
  const before = await when((p) => p.done.length >= 1, "of the second with a tile");
  await act("jobs/cancel", { id: before.progress.job });
  const cancelled = await when((_, j) => j.state === "cancelled", "cancelled");

  const recorded = { running, paused, completed, cancelled };
  // Generated, on one line: Prettier leaves it be (.prettierignore).
  writeFileSync(OUT, `${JSON.stringify(recorded)}\n`);
  console.log(`record-scan: wrote ${OUT}`);
} finally {
  await server.dispose();
}
