// Records the charts gallery's data from the simulated instrument, through
// its API, as a client would (W0.7): a server of its own puts the test
// target on the stage, focuses it from 15 µm off, as ex-autofocus does, and
// captures a burst through the sensor's middle; the burst is exported as
// OME-TIFF and read back. It keeps each frame's histogram, a
// profile across the middle with the spread of the rows around it, and the
// focus run's curve, in src/features/dev/charts/recorded.ts.
//
//   npm run build && node scripts/record-charts.mjs     (from web/, with oscope-server built)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const FRAMES = 10;
const REGION = { x: 2256, y: 1464, width: 960, height: 720 };
/** The coverslip's middle, in stage µm: the simulator's slide is 50 × 24 mm, its corner at (22, 0.5) mm. */
const MIDDLE = { x: 47_000, y: 12_500 };
const BINS = 4096;
const BAND_ROWS = 16;
const out = fileURLToPath(new URL("../src/features/dev/charts/recorded.ts", import.meta.url));
/** The data file's first lines: what it is, and its shape. */
const HEADER = `// Recorded from the simulated instrument by scripts/record-charts.mjs: don't edit.

export interface Recorded {
  recorded: string;
  sample: string;
  frame: { width: number; height: number; umPerPixel: number };
  histograms: { binWidth: number; counts: number[][] };
  profile: { um: number[]; mean: number[]; low: number[]; high: number[] };
  focus: { measure: string; z: number; coarse: [number, number][]; fine: [number, number][] };
}

`;

const server = new Server();
await server.start();
try {
  /**
   * A call to the server, answered in JSON.
   * @param {string} method
   * @param {string} path
   * @param {unknown} [body]
   * @returns {Promise<any>}
   */
  const call = async (method, path, body) => {
    const answer = await fetch(`${server.url}/api/v1/${path}`, {
      method,
      headers: { "content-type": "application/json" },
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    });
    const text = await answer.text();
    if (!answer.ok) throw new Error(`${method} ${path}: ${answer.status} ${text}`);
    return text ? JSON.parse(text) : null;
  };
  /**
   * Invokes an action, waits for it to finish, and gives its output.
   * @param {string} path
   * @param {unknown} body
   * @returns {Promise<any>}
   */
  const act = async (path, body) => {
    let invocation = await call("POST", path, body);
    while (!["completed", "error", "cancelled"].includes(invocation.status)) {
      await new Promise((done) => setTimeout(done, 100));
      invocation = await call("GET", `action_invocations/${invocation.id}`);
    }
    if (invocation.status !== "completed")
      throw new Error(
        `${path}: ${invocation.status}: ${JSON.stringify(invocation.error ?? invocation)}`,
      );
    return invocation.output;
  };
  /**
   * Waits for a job to end, and gives it as it ended.
   * @param {{ id: string }} job
   * @returns {Promise<any>}
   */
  const ended = async (job) => {
    for (;;) {
      const now = await call("GET", `jobs/job/${job.id}`);
      if (["completed", "failed", "cancelled", "interrupted"].includes(now.state)) {
        if (now.state !== "completed") throw new Error(`job ${job.id}: ${now.state}: ${now.error}`);
        return now;
      }
      await new Promise((done) => setTimeout(done, 200));
    }
  };

  await call("PUT", "camera/roi", REGION);
  await act("stage/home", {});
  await act("camera/start_streaming", {});
  const samples = await call("GET", "simulation/samples");
  const target = samples.find((/** @type {{ kind: string }} */ one) => one.kind === "test-target");
  await act("simulation/load", { name: target.name });
  for (const channel of ["transmitted", "reflected", "blue 488"]) {
    await act("illumination/set_light", { channel, on: channel === "transmitted" });
  }
  const umPerPixel = await call("GET", "objective/pixel_at_sample");

  // The focus run, from 15 µm above the focus, and its curve.
  const truth = await act("simulation/in_focus", MIDDLE);
  await act("stage/move_to", { ...MIDDLE, z: truth + 15 });
  await ended(await act("autofocus/run", {}));
  const report = await call("GET", "autofocus/last");

  // A burst, exported, and read back a frame at a time.
  const project = await act("projects/create", { name: "charts" });
  const burst = await ended(
    await act("capture/burst", { project: project.id, frames: FRAMES, name: "frames" }),
  );
  // The burst's result is its record's path in the project: the export wants its ID, the path's end.
  const record = String(burst.results[0]).split("/").at(-1);
  const exported = await ended(
    await act("jobs/submit", {
      kind: "projects.export",
      params: { project: project.id, record, format: "ome-tiff" },
    }),
  );
  // The export's result is its file's path under the API: data/{project}/exports/….
  const file = await fetch(`${server.url}/api/v1/${exported.results[0]}`);
  if (!file.ok)
    throw new Error(`the export, ${exported.results[0]}: ${file.status} ${await file.text()}`);
  const frames = pages(Buffer.from(await file.arrayBuffer()));

  const histograms = frames.map((frame) => {
    const counts = new Array(BINS).fill(0);
    for (const value of frame.samples) {
      const bin = Math.floor((value * BINS) / 65536);
      counts[bin] = (counts[bin] ?? 0) + 1;
    }
    return counts;
  });
  const first = frames[0];
  if (!first) throw new Error("the export holds no frame");
  const data = {
    recorded: new Date().toISOString().slice(0, 10),
    sample: target.name,
    frame: { width: first.width, height: first.height, umPerPixel },
    histograms: { binWidth: 65536 / BINS, counts: histograms },
    profile: profile(first, umPerPixel),
    focus: {
      measure: report.measure,
      z: report.z_um,
      coarse: report.coarse.map((/** @type {{ z_um: number; value: number }} */ one) => [
        one.z_um,
        one.value,
      ]),
      fine: report.fine.map((/** @type {{ z_um: number; value: number }} */ one) => [
        one.z_um,
        one.value,
      ]),
    },
  };
  writeFileSync(
    out,
    `${HEADER}const recorded: Recorded = ${JSON.stringify(data)};

export default recorded;
`,
  );
  console.log(
    `recorded ${frames.length} frames of ${first.width} × ${first.height}, and a focus run of ${report.coarse.length + report.fine.length} frames, to ${out}`,
  );
} finally {
  await server.dispose();
}

/**
 * The pages of an uncompressed TIFF of 16-bit samples, little-endian: each
 * page's size and samples.
 * @param {Buffer} bytes
 * @returns {{ width: number; height: number; samples: Uint16Array }[]}
 */
function pages(bytes) {
  if (bytes.toString("latin1", 0, 4) !== "II*\0") throw new Error("not a little-endian TIFF");
  const found = [];
  for (let at = bytes.readUInt32LE(4); at !== 0;) {
    /** @type {Map<number, number[]>} */
    const tags = new Map();
    const count = bytes.readUInt16LE(at);
    for (let n = 0; n < count; n++) {
      const entry = at + 2 + n * 12;
      const [tag, type, many] = [
        bytes.readUInt16LE(entry),
        bytes.readUInt16LE(entry + 2),
        bytes.readUInt32LE(entry + 4),
      ];
      const size = type === 3 ? 2 : 4;
      const where = many * size <= 4 ? entry + 8 : bytes.readUInt32LE(entry + 8);
      tags.set(
        tag,
        Array.from({ length: many }, (_, k) =>
          size === 2 ? bytes.readUInt16LE(where + 2 * k) : bytes.readUInt32LE(where + 4 * k),
        ),
      );
    }
    const one = (/** @type {number} */ tag) => tags.get(tag)?.[0] ?? 0;
    if (one(258) !== 16 || (tags.has(259) && one(259) !== 1))
      throw new Error("not uncompressed 16-bit samples");
    const [width, height] = [one(256), one(257)];
    const samples = new Uint16Array(width * height);
    let k = 0;
    const offsets = tags.get(273) ?? [];
    const lengths = tags.get(279) ?? [];
    offsets.forEach((offset, strip) => {
      for (let b = 0; b < (lengths[strip] ?? 0); b += 2)
        samples[k++] = bytes.readUInt16LE(offset + b);
    });
    found.push({ width, height, samples });
    at = bytes.readUInt32LE(at + 2 + count * 12);
  }
  return found;
}

/**
 * A profile across the frame's middle: each 2 × 2 cell of the mosaic as the
 * mean of its four, the mean of the rows of cells around the middle, and
 * their 10th and 90th percentiles, by position in µm.
 * @param {{ width: number; height: number; samples: Uint16Array }} frame
 * @param {number} umPerPixel
 */
function profile(frame, umPerPixel) {
  const cells = Math.floor(frame.width / 2);
  const middle = Math.floor(frame.height / 4);
  const um = [];
  const mean = [];
  const low = [];
  const high = [];
  for (let c = 0; c < cells; c++) {
    const values = [];
    for (let r = middle - BAND_ROWS / 2; r < middle + BAND_ROWS / 2; r++) {
      const at = 2 * r * frame.width + 2 * c;
      const s = (/** @type {number} */ k) => frame.samples[k] ?? 0;
      values.push((s(at) + s(at + 1) + s(at + frame.width) + s(at + frame.width + 1)) / 4);
    }
    values.sort((a, b) => a - b);
    um.push(Number((c * 2 * umPerPixel).toFixed(3)));
    mean.push(Math.round(values.reduce((sum, value) => sum + value, 0) / values.length));
    low.push(Math.round(values[Math.floor(0.1 * (values.length - 1))] ?? 0));
    high.push(Math.round(values[Math.ceil(0.9 * (values.length - 1))] ?? 0));
  }
  return { um, mean, low, high };
}
