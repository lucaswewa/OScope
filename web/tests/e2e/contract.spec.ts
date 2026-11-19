// Contract tests (W1.1, ADR-0043): the typed client, and the wrappers
// written by hand for what the OpenAPI document doesn't describe, against
// a real server, each test's own. No page: they run in Chromium's project
// only, as untagged tests do.
import { act, start, type Invocation } from "../../src/api/actions.ts";
import { call, Server } from "../../src/api/client.ts";
import { ApiError } from "../../src/api/errors.ts";
import { finished, JOB_ENDED, type Job, type JobRef } from "../../src/api/jobs.ts";
import {
  brick,
  download,
  logTail,
  slice,
  tile,
  volumeMeta,
  zoomDescriptor,
  zoomStatus,
  type StoredImage,
  type TailMessage,
} from "../../src/api/media.ts";
import { expect, test } from "./fixtures.ts";
import { fast } from "./sim.ts";

/** What a call threw. */
async function thrown(promise: Promise<unknown>): Promise<ApiError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(ApiError);
  return error as ApiError;
}

test("an action is followed to completion, to a failure, and cancelled", async ({ server }) => {
  const on = new Server(server.url);
  // Before homing, the stage refuses to move.
  const refused = await thrown(act("/api/v1/stage/move_to", { x: 0, y: 0, z: 0 }, { on }));
  expect([refused.failure, refused.refusal, refused.status]).toEqual(["refused", "alarm", 409]);
  const homed = await fast(server.url, () =>
    act("/api/v1/stage/home", {}, { on, timeout: 30_000 }),
  );
  expect(Object.keys(homed.position).sort()).toEqual(["x", "y", "z"]);
  // A slow move, stopped by its caller: cancelled on the server too.
  const caller = new AbortController();
  let moving: Invocation | undefined;
  const move = act(
    "/api/v1/stage/move_to",
    { x: 90_000, y: 0, z: 0, speed: 50 },
    {
      on,
      signal: caller.signal,
      seen: (now) => {
        moving = now;
        if (now.status === "running") caller.abort();
      },
    },
  );
  expect((await thrown(move)).failure).toBe("aborted");
  const id = moving!.id;
  await expect
    .poll(async () => {
      const now = await call((signal) =>
        on.api.GET("/api/v1/action_invocations/{id}", { params: { path: { id } }, signal }),
      );
      return now.status;
    })
    .toBe("cancelled");
});

test("the server's failures are read as the error model has them", async ({ server }) => {
  const on = new Server(server.url);
  const { api } = on;
  const range = await thrown(
    call((signal) => api.PUT("/api/v1/camera/gain", { body: -5, signal })),
  );
  expect([range.failure, range.refusal, range.status]).toEqual(["refused", "out of range", 400]);
  // A value the description doesn't allow, sent anyway.
  const wrong = { x: "a" } as unknown as { x: number; y: number; z: number };
  const invalid = await thrown(start("/api/v1/stage/move_to", wrong, { on }));
  expect(invalid.failure).toBe("invalid");
  expect(invalid.fields.map((field) => field.at.join("."))).toEqual(["x", "y", "z"]);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const roi = { x: 0, y: 0, width: 960, height: 720 };
  const busy = await thrown(call((signal) => api.PUT("/api/v1/camera/roi", { body: roi, signal })));
  expect([busy.failure, busy.refusal, busy.status]).toEqual(["refused", "busy", 409]);
  const id = "00000000-0000-0000-0000-000000000000";
  const missing = await thrown(
    call((signal) =>
      api.GET("/api/v1/action_invocations/{id}", { params: { path: { id } }, signal }),
    ),
  );
  expect([missing.failure, missing.status]).toEqual(["missing", 404]);
});

test("a captured image: its job, its tiles, its volume and its files", async ({ server }) => {
  test.setTimeout(120_000);
  const on = new Server(server.url);
  const { api } = on;
  // A part of the sensor, to keep the image small.
  const roi = { x: 2256, y: 1464, width: 960, height: 720 };
  await call((signal) => api.PUT("/api/v1/camera/roi", { body: roi, signal }));
  await act("/api/v1/camera/start_streaming", {}, { on });
  const project = await act("/api/v1/projects/create", { name: "contract" }, { on });
  const ref: JobRef = await act("/api/v1/capture/single", { project: project.id }, { on });
  const states = new Set<string>();
  const job: Job = await finished(ref, { on, seen: (now) => states.add(now.state) });
  // The job's shape, as jobs.ts has it.
  expect(JOB_ENDED.has(job.state)).toBe(true);
  expect(states.has("completed")).toBe(true);
  expect(typeof job.kind === "string" && typeof job.title === "string").toBe(true);
  expect(Object.keys(job.progress).sort()).toEqual(["counters", "eta_s", "fraction", "stage"]);
  expect(job.results).toHaveLength(1);
  const image: StoredImage = {
    project: project.id,
    kind: "acquisitions",
    id: job.results[0]!.split("/").at(-1)!,
  };

  // Deep zoom.
  const descriptor = await zoomDescriptor(image, { on });
  expect(descriptor).toEqual({ width: 960, height: 720, tileSize: 256, overlap: 0, format: "jpg" });
  const status = await zoomStatus(image, { on });
  expect(status.size).toEqual([960, 720]);
  const finest = status.levels - 1;
  const picture = await tile(image, { level: finest, col: 0, row: 0 }, { on, timeout: 30_000 });
  expect(picture.type).toBe("image/jpeg");
  expect([...new Uint8Array(await picture.arrayBuffer()).slice(0, 2)]).toEqual([0xff, 0xd8]);

  // Its planes as a volume.
  const meta = await volumeMeta(image, { on });
  expect(meta).toMatchObject({ size: [960, 720, 1], channels: 1, header: 64 });
  const coarsest = meta.levels.at(-1)!;
  const across = await slice(
    image,
    { axis: "z", index: 0, level: coarsest.level, format: "u8" },
    { on },
  );
  expect([across.level, across.size, across.channels]).toEqual([coarsest.level, coarsest.size, 1]);
  expect(across.samples).toHaveLength(coarsest.size[0] * coarsest.size[1]);
  const box = await brick(image, { level: coarsest.level, format: "u16" }, { on });
  expect(box.samples).toBeInstanceOf(Uint16Array);
  expect(box.size).toEqual(coarsest.size);

  // Its files, whole and in part.
  const thumbnail = `acquisitions/${image.id}/thumbnail.jpg`;
  const whole = await download(project.id, thumbnail, { on });
  expect(whole.type).toBe("image/jpeg");
  const part = await download(project.id, thumbnail, { on, range: { from: 0, to: 15 } });
  expect(part.range).toEqual({ from: 0, to: 15, of: whole.bytes.byteLength });
  expect([...new Uint8Array(part.bytes)]).toEqual([...new Uint8Array(whole.bytes).slice(0, 16)]);

  // A job that fails: an export of a record that isn't there.
  const params = { project: project.id, record: "00000000-0000-0000-0000-000000000000" };
  const exported = await act(
    "/api/v1/jobs/submit",
    { kind: "projects.export", params: { ...params, format: "ome-tiff" } },
    { on },
  );
  const failed = await thrown(finished(exported as JobRef, { on }));
  expect([failed.failure, failed.message]).toEqual(["failed", `no acquisition ${params.record}`]);
});

test("the logs' tail sends its query, the records that match, and catches up", async ({
  server,
}) => {
  const on = new Server(server.url);
  const told: TailMessage[] = [];
  const caller = new AbortController();
  const tail = logTail((message) => told.push(message), { on, signal: caller.signal });
  await expect.poll(() => told.filter((message) => "caught_up" in message).length).toBe(1);
  // The query it follows comes first: every condition unset.
  const asked = (message: TailMessage | undefined) =>
    message && "query" in message
      ? Object.fromEntries(Object.entries(message.query).filter(([, value]) => value !== null))
      : undefined;
  expect(asked(told[0])).toEqual({});
  const records = told.flatMap((message) => ("record" in message ? [message.record] : []));
  expect(told.at(-1)).toEqual({ caught_up: records.length });
  for (const record of records) {
    expect(typeof record.seq === "number" && typeof record.message === "string").toBe(true);
    expect(["trace", "debug", "info", "warn", "error", "critical"]).toContain(record.level);
  }
  // Another query: the tail starts again from the records that match it.
  tail.ask({ level: "error" });
  await expect.poll(() => told.filter((message) => "caught_up" in message).length).toBe(2);
  expect(told.map(asked).filter(Boolean)).toEqual([{}, { level: "error" }]);
  caller.abort();
  await tail.ended;
});
