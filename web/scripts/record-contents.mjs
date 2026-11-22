// A project's contents, recorded for the unit tests of its page (W6.2), on
// a server of the script's own (tests/e2e/server.ts): an acquisition, a
// snake of a few tiles over the simulator's tissue through the camera's
// tiny region; the result stitched from it; and a report of the project.
//
//   node scripts/record-contents.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(
  new URL("../src/features/gallery/project/recorded-contents.json", import.meta.url),
);
const TISSUE = { x: 40_000, y: 12_000 };
const TINY = { x: 2592, y: 1716, width: 288, height: 216 };

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
 * A job, followed to its end: its record.
 * @param {{ id: string }} job
 */
async function ended(job) {
  for (let tries = 0; tries < 6000; tries++) {
    const now = await request("GET", `jobs/job/${job.id}`);
    if (now.state === "completed") return now;
    if (["failed", "cancelled", "interrupted"].includes(now.state))
      throw new Error(`the job ended ${now.state}: ${now.error}`);
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error("the job didn't end");
}

try {
  await request("PUT", "simulation/rate", 50);
  await act("stage/home", {});
  const z = await act("simulation/in_focus", TISSUE);
  await act("stage/move_to", { ...TISSUE, z, speed: 25_000 });
  await request("PUT", "simulation/rate", 1);
  await request("PUT", "camera/roi", TINY);
  await act("camera/start_streaming", {});
  const project = (await act("projects/create", { name: "Records" })).id;
  const region = { rect: { origin: { x: 39_920, y: 11_940 }, size: { width: 160, height: 120 } } };
  const scanned = await ended(
    await act("scan/run", {
      request: { pattern: "snake", region, positions: [] },
      project,
      name: "a scan",
    }),
  );
  const acquisition = scanned.results
    .map((/** @type {string} */ r) => /acquisitions\/([^/]+)$/.exec(r)?.[1])
    .find(Boolean);
  await ended(await act("jobs/submit", { kind: "stitch.run", params: { project, acquisition } }));
  await ended(await act("jobs/submit", { kind: "report.make", params: { project } }));
  const contents = await request("GET", `projects/project/${project}`);
  // Generated, on one line: Prettier leaves it be (.prettierignore).
  writeFileSync(OUT, `${JSON.stringify(contents)}\n`);
  console.log(
    `record-contents: wrote ${OUT}: ${contents.acquisitions.length} acquisitions, ` +
      `${contents.results.length} results, ${contents.reports.length} reports`,
  );
} finally {
  await server.dispose();
}
