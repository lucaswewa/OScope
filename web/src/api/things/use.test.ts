// Live Things in a component (W1.2): `useProperty`, `useEvent` and
// `useAction`, in an effect scope as a component's setup is, typed by their
// paths. On a stand-in socket and network.
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, expectTypeOf, it, vi } from "vitest";
import { effectScope } from "vue";

import {
  described,
  FakeSocket,
  problem,
  standInNetwork,
  standInSockets,
} from "../../../tests/setup/stand-ins.ts";
import { Server, type Schema } from "../client.ts";
import { partsOf, useAction, useEvent, useProperty } from "./use.ts";

let on: Server;
let made: Request[];
let looks: string[];

const invocation = (status: string, output: unknown = null) => ({
  status,
  id: "an-invocation",
  action: "/api/v1/stage/home",
  href: "",
  timeStarted: null,
  timeRequested: null,
  timeCompleted: null,
  links: [],
  input: {},
  output,
  log: [],
  error: null,
});

beforeEach(() => {
  setActivePinia(createPinia());
  on = new Server("http://127.0.0.1:8211");
  looks = ["running", "completed"];
  standInSockets();
  made = standInNetwork({
    "GET /api/v1/thing_descriptions/": () => ({
      stage: described({ position: {}, speed: { observable: true, writable: true } }, ["moved"]),
    }),
    "GET /api/v1/stage/position": () => ({ time: 1, position: { x: 1, y: 2, z: 3 } }),
    "GET /api/v1/stage/speed": () => 1000,
    "PUT /api/v1/stage/speed": () => problem(409, "busy", "the stage is moving"),
    "POST /api/v1/stage/home": () =>
      new Response(JSON.stringify(invocation("pending")), { status: 201 }),
    "GET /api/v1/action_invocations/an-invocation": () => {
      const status = looks.shift() ?? "completed";
      return invocation(status, status === "completed" ? { position: { x: 0 } } : null);
    },
    "DELETE /api/v1/action_invocations/an-invocation": () => null,
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

/** Waits for a condition, as an end-to-end test would. */
async function until(condition: () => boolean): Promise<void> {
  await vi.waitFor(() => expect(condition()).toBe(true), { timeout: 2000, interval: 5 });
}

describe("a property in a component", () => {
  it("is its value, live, until the component goes", async () => {
    const scope = effectScope();
    const speed = scope.run(() => useProperty("/api/v1/stage/speed", { on }))!;
    await until(() => speed.value.value === 1000);
    FakeSocket.last.opens();
    FakeSocket.last.says({ messageType: "propertyStatus", data: { speed: 3000 } });
    expect(speed.value.value).toBe(3000);
    expect(speed.stale.value).toBe(false);
    scope.stop();
    expect(FakeSocket.last.readyState).toBe(FakeSocket.CLOSED);
  });

  it("shows a write while it is under way, and its refusal's reason after", async () => {
    const scope = effectScope();
    const speed = scope.run(() => useProperty("/api/v1/stage/speed", { on }))!;
    await until(() => speed.value.value === 1000);
    const writing = speed.write(5000);
    expect([speed.value.value, speed.writing.value]).toEqual([5000, true]);
    await writing;
    expect([speed.value.value, speed.writing.value]).toEqual([1000, false]);
    expect(speed.refused.value).toMatchObject({ refusal: "busy", message: "the stage is moving" });
    scope.stop();
  });

  it("is typed by its path: its value, and what may be written", () => {
    const scope = effectScope();
    scope.run(() => {
      const position = useProperty("/api/v1/stage/position", { on });
      expectTypeOf(position.value.value).toEqualTypeOf<
        Schema<"stage_position_value"> | undefined
      >();
      // @ts-expect-error: the position can't be written.
      void (() => position.write({ time: 0, position: { x: 0, y: 0, z: 0 } }));
      const speed = useProperty("/api/v1/stage/speed", { on });
      void (() => speed.write(5000));
      // @ts-expect-error: a speed is a number.
      void (() => speed.write("fast"));
    });
    scope.stop();
  });
});

describe("an event in a component", () => {
  it("is each as its Thing tells it, in order, the newest `keep`", async () => {
    const scope = effectScope();
    const moved = scope.run(() =>
      useEvent<{ position: { x: number } }>("/api/v1/stage/moved", { on, keep: 2 }),
    )!;
    FakeSocket.last.opens();
    for (const x of [1, 2, 3]) {
      const data = { moved: { data: { position: { x } }, timestamp: `t${x}` } };
      FakeSocket.last.says({ messageType: "event", data });
    }
    expect(moved.told.value.map((one) => one.data.position.x)).toEqual([2, 3]);
    expect(moved.last.value?.time).toBe("t3");
    scope.stop();
  });
});

describe("an action in a component", () => {
  it("is run, its invocation told as it goes, and its output given", async () => {
    const scope = effectScope();
    const home = scope.run(() => useAction("/api/v1/stage/home", { on }))!;
    const output = await home.run({});
    expect(output).toEqual({ position: { x: 0 } });
    expect([home.invocation.value?.status, home.running.value, home.error.value]).toEqual([
      "completed",
      false,
      undefined,
    ]);
    scope.stop();
  });

  it("gives its error, not a throw", async () => {
    looks = ["running"];
    standInNetwork({
      "POST /api/v1/stage/home": () =>
        new Response(JSON.stringify(invocation("pending")), { status: 201 }),
      "GET /api/v1/action_invocations/an-invocation": () => ({
        ...invocation("error"),
        error: { detail: "alarm (409): the stage is in alarm" },
      }),
    });
    const scope = effectScope();
    const home = scope.run(() => useAction("/api/v1/stage/home", { on }))!;
    expect(await home.run({})).toBeUndefined();
    expect(home.error.value).toMatchObject({ refusal: "alarm", status: 409 });
    scope.stop();
  });

  it("isn't cancelled when its component goes: it is only no longer followed", async () => {
    looks = Array(100).fill("running");
    const scope = effectScope();
    const home = scope.run(() => useAction("/api/v1/stage/home", { on }))!;
    const running = home.run({});
    await until(() => home.running.value);
    scope.stop();
    expect(await running).toBeUndefined();
    expect(home.error.value?.failure).toBe("aborted");
    expect(made.some((request) => request.method === "DELETE")).toBe(false);
  });

  it("is cancelled on the server when asked", async () => {
    looks = Array(100).fill("running");
    const scope = effectScope();
    const home = scope.run(() => useAction("/api/v1/stage/home", { on }))!;
    void home.run({});
    await until(() => home.running.value);
    await home.cancel();
    expect(made.filter((request) => request.method === "DELETE")).toHaveLength(1);
    scope.stop();
  });
});

describe("a path", () => {
  it("names its Thing and its affordance, or isn't one", () => {
    expect(partsOf("/api/v1/stage/position")).toEqual(["stage", "position"]);
    expect(() => partsOf("/api/v1/stage")).toThrow("isn't one of a Thing's affordances");
  });
});
