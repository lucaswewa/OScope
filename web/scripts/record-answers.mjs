// The server's answers to calls that fail, recorded for the error model's
// tests (src/api/errors.test.ts, ADR-0043): each the status, content type and
// body the server gives, or the invocation of an action that fails, from a
// server of the script's own (tests/e2e/server.ts), driven as a client would.
//
//   node scripts/record-answers.mjs    (after `cargo build -p oscope-server`
//                                       and `npm run build`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(new URL("../src/api/recorded-answers.json", import.meta.url));
const server = new Server();
await server.start();
const base = `${server.url}/api/v1/`;

/**
 * A call's answer, as the client sees it.
 * @param {string} method
 * @param {string} path
 * @param {unknown} [body] sent as JSON, or as it is if a string
 */
async function answer(method, path, body) {
  /** @type {RequestInit} */
  const request = { method };
  if (body !== undefined) {
    request.headers = { "content-type": "application/json" };
    request.body = typeof body === "string" ? body : JSON.stringify(body);
  }
  const reply = await fetch(base + path, request);
  const text = await reply.text();
  let parsed;
  try {
    parsed = JSON.parse(text);
  } catch {
    parsed = text;
  }
  return { status: reply.status, type: reply.headers.get("content-type"), body: parsed };
}

/**
 * An action started, and its invocation once it has ended; `then` runs
 * while it is under way.
 * @param {string} path
 * @param {unknown} input
 * @param {(id: string) => Promise<unknown>} [then]
 */
async function invoked(path, input, then) {
  const started = await answer("POST", path, input);
  if (started.status !== 201)
    throw new Error(`${path}: ${started.status} ${JSON.stringify(started.body)}`);
  const id = started.body.id;
  if (then) await then(id);
  for (;;) {
    const now = await answer("GET", `action_invocations/${id}`);
    if (["completed", "error", "cancelled"].includes(now.body.status)) return now.body;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

/**
 * A job followed to its end, from the `JobRef` its action gave.
 * @param {{ id: string }} ref
 */
async function finished(ref) {
  for (;;) {
    const job = await answer("GET", `jobs/job/${ref.id}`);
    if (["completed", "failed", "cancelled", "interrupted"].includes(job.body.state))
      return job.body;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

try {
  /** @type {Record<string, unknown>} */
  const recorded = {
    // The server's checks.
    invalid: await answer("POST", "stage/move_to", { x: "a" }),
    malformed: await answer("POST", "stage/move_to", "not json"),
    unreadable: await answer("POST", "jobs/cancel", { id: "not-a-uuid" }),
    missing: await answer("GET", "action_invocations/00000000-0000-0000-0000-000000000000"),
    // A device's refusals of a property's value (ADR-0034).
    outOfRange: await answer("PUT", "camera/gain", -5),
    // An action's errors, its kind and status at the head of its detail.
    alarm: await invoked("stage/move_to", { x: 0, y: 0, z: 0 }),
    homed: await invoked("stage/home", {}),
    beyond: await invoked("stage/move_to", { x: 1e9, y: 0, z: 0 }),
    cancelled: await invoked("stage/move_to", { x: 90000, y: 0, z: 0, speed: 50 }, (id) =>
      answer("DELETE", `action_invocations/${id}`),
    ),
  };
  await invoked("camera/start_streaming", {});
  recorded.busy = await answer("PUT", "camera/roi", { x: 0, y: 0, width: 960, height: 720 });
  // The media plane's leaner problem details, and a job's answer.
  const project = (await invoked("projects/create", { name: "answers" })).output;
  const capture = (await invoked("capture/single", { project: project.id })).output;
  const job = await finished(capture);
  const record = job.results[0].split("/").at(-1);
  recorded.job = job;
  // A job that fails: an export of a record that isn't there.
  const missing = "00000000-0000-0000-0000-000000000000";
  recorded.failedJob = await finished(
    (
      await invoked("jobs/submit", {
        kind: "projects.export",
        params: { project: project.id, record: missing, format: "ome-tiff" },
      })
    ).output,
  );
  recorded.noLevel = await answer(
    "GET",
    `volume/${project.id}/acquisitions/${record}/slice?axis=z&index=0&level=0&format=u8`,
  );
  writeFileSync(OUT, JSON.stringify(recorded, null, 2) + "\n");
  console.log(`recorded ${Object.keys(recorded).length} answers in ${OUT}`);
} finally {
  await server.stop();
}
