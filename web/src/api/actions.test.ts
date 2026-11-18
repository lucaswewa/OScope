// Actions (ADR-0043): started, followed to their end through
// `action_invocations/{id}`, or cancelled there; typed by their paths. The
// server is stood in for here, with its recorded answers; the contract
// tests (tests/e2e/contract.spec.ts) follow a real one.
import { afterEach, describe, expect, expectTypeOf, it, vi } from "vitest";

import { act, cancel, start, type Invocation } from "./actions.ts";
import { Server, type Schema } from "./client.ts";
import recorded from "./recorded-answers.json";

const on = new Server("http://127.0.0.1:8211");
const ID = "a1b2c3d4-0000-4000-8000-000000000001";

const invocation = (status: Invocation["status"], more: Partial<Invocation> = {}): Invocation => ({
  status,
  id: ID,
  action: "/api/v1/stage/move_to",
  href: `http://127.0.0.1:8211/api/v1/action_invocations/${ID}`,
  timeStarted: null,
  timeRequested: "2026-11-18T12:00:00",
  timeCompleted: null,
  links: [],
  input: {},
  output: null,
  log: [],
  error: null,
  ...more,
});

const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });

/**
 * A server with one action: its start answers `pending`, each look the next
 * of `looks`, the last again and again. Each request is kept, with its time.
 */
function serving(looks: Invocation[]) {
  const made: { request: Request; at: number }[] = [];
  let seen = 0;
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const request = input instanceof Request ? input : new Request(input, init);
    made.push({ request, at: performance.now() });
    if (request.method === "POST") return json(invocation("pending"), 201);
    if (request.method === "DELETE") return json(null);
    return json(looks[Math.min(seen++, looks.length - 1)]);
  });
  return made;
}

const where = { position: { x: 1, y: 2, z: 3 }, time: 12.5 };
const ended = (name: "alarm" | "cancelled") =>
  recorded[name] as unknown as Pick<Invocation, "status" | "error">;

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("an action", () => {
  it("is started with its input, followed to its end, and gives its output", async () => {
    const made = serving([invocation("running"), invocation("completed", { output: where })]);
    const told: string[] = [];
    const output = await act(
      "/api/v1/stage/move_to",
      { x: 1, y: 2, z: 3 },
      { on, seen: (now) => told.push(now.status) },
    );
    expect(output).toEqual(where);
    expect(told).toEqual(["pending", "running", "completed"]);
    const first = made[0]!.request;
    expect([first.method, new URL(first.url).pathname]).toEqual(["POST", "/api/v1/stage/move_to"]);
    expect(await first.json()).toEqual({ x: 1, y: 2, z: 3 });
    const looks = made.slice(1).map(({ request }) => new URL(request.url).pathname);
    expect(new Set(looks)).toEqual(new Set([`/api/v1/action_invocations/${ID}`]));
  });

  it("throws a device's refusal from its invocation's error", async () => {
    serving([invocation("running"), invocation("error", { error: ended("alarm").error })]);
    await expect(act("/api/v1/stage/move_to", { x: 0, y: 0, z: 0 }, { on })).rejects.toMatchObject({
      failure: "refused",
      refusal: "alarm",
      status: 409,
      message: "the stage is in alarm: homing required",
    });
  });

  it("throws its cancellation", async () => {
    serving([invocation("cancelled", { error: ended("cancelled").error })]);
    await expect(act("/api/v1/stage/home", {}, { on })).rejects.toMatchObject({
      failure: "cancelled",
    });
  });

  it("is cancelled on the server when its caller stops waiting", async () => {
    const made = serving([invocation("running")]);
    const caller = new AbortController();
    const running = act("/api/v1/stage/home", null, {
      on,
      signal: caller.signal,
      seen: (now) => {
        if (now.status === "running") caller.abort();
      },
    });
    await expect(running).rejects.toMatchObject({ failure: "aborted" });
    const last = made.at(-1)!.request;
    expect([last.method, new URL(last.url).pathname]).toEqual([
      "DELETE",
      `/api/v1/action_invocations/${ID}`,
    ]);
  });

  it("is looked at 50 ms after its start, each wait then half as long again", async () => {
    const made = serving([
      invocation("running"),
      invocation("running"),
      invocation("completed", { output: where }),
    ]);
    await act("/api/v1/stage/home", {}, { on });
    const times = made.map(({ at }) => at);
    const waits = times.slice(1).map((at, n) => at - times[n]!);
    // At least: a busy computer only waits longer.
    expect(waits[0]).toBeGreaterThanOrEqual(49);
    expect(waits[1]).toBeGreaterThanOrEqual(74);
    expect(waits[2]).toBeGreaterThanOrEqual(111);
  });

  it("can be started, and cancelled, apart", async () => {
    const made = serving([invocation("running")]);
    const started = await start("/api/v1/stage/home", {}, { on });
    expect(started.status).toBe("pending");
    await cancel(started.id, { on });
    expect(made.map(({ request }) => request.method)).toEqual(["POST", "DELETE"]);
  });

  it("is typed by its path: its input and its output", () => {
    // Only called for their types.
    const typed = () => act("/api/v1/stage/move_to", { x: 1, y: 2, z: 3 });
    expectTypeOf(typed).returns.resolves.toEqualTypeOf<Schema<"stage_move_to_output">>();
    // @ts-expect-error: x is a number.
    void (() => act("/api/v1/stage/move_to", { x: "1", y: 2, z: 3 }));
    // @ts-expect-error: z is wanted.
    void (() => act("/api/v1/stage/move_to", { x: 1, y: 2 }));
    // @ts-expect-error: the version is a property, not an action.
    void (() => act("/api/v1/system/version", {}));
    // An action whose input is optional takes nothing, too.
    void (() => act("/api/v1/stage/home", undefined));
  });
});
