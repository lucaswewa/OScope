// A histo session, recorded for the unit tests of the focus map (W5.3), on
// a server of the script's own (tests/e2e/server.ts): the plan of a snake
// over a small part of the simulator's tissue, through the camera's tiny
// region, and the focus maps two runs of it measured at a 2 × 2 grid
// before their first tile, a plane and a thin plate. Nothing else moves.
//
//   node scripts/record-histo.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(
  new URL("../src/features/scan/histo/recorded-histo.json", import.meta.url),
);
const TISSUE = { x: 40_000, y: 12_000 };
const TINY = { x: 2592, y: 1716, width: 288, height: 216 };
const REGION = { rect: { origin: { x: 39_920, y: 11_940 }, size: { width: 160, height: 120 } } };

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
 * The map a run of `asked` measured, once its job has ended.
 * @param {string} project
 * @param {unknown} asked
 */
async function measured(project, asked) {
  const job = await act("scan/run", { request: asked, project });
  for (let tries = 0; tries < 6000; tries++) {
    const now = await request("GET", `jobs/job/${job.id}`);
    if (now.state === "completed") return (await request("GET", "scan/progress")).plan.z.map;
    if (["failed", "cancelled", "interrupted"].includes(now.state))
      throw new Error(`the scan ended ${now.state}: ${now.error}`);
    await new Promise((done) => setTimeout(done, 100));
  }
  throw new Error("the scan didn't end");
}

try {
  await request("PUT", "simulation/rate", 50);
  await act("stage/home", {});
  const z = await act("simulation/in_focus", TISSUE);
  await act("stage/move_to", { ...TISSUE, z, speed: 25_000 });
  await request("PUT", "simulation/rate", 1);
  await request("PUT", "camera/roi", TINY);
  await act("camera/start_streaming", {});
  const project = (await act("projects/create", { name: "Histo" })).id;
  /** @param {"plane" | "thin_plate"} kind */
  const asked = (kind) => ({
    pattern: "snake",
    region: REGION,
    positions: [],
    z: {
      kind: "measure",
      columns: 2,
      rows: 2,
      surface: kind === "plane" ? { kind } : { kind, smoothing: 0 },
    },
  });
  const preview = await act("scan/plan", { request: asked("plane") });
  const plane = await measured(project, asked("plane"));
  const thinPlate = await measured(project, asked("thin_plate"));
  // Generated, on one line: Prettier leaves it be (.prettierignore).
  writeFileSync(OUT, `${JSON.stringify({ preview, plane, thinPlate })}\n`);
  console.log(`record-histo: wrote ${OUT}, ${preview.plan.tiles.length} tiles`);
} finally {
  await server.dispose();
}
