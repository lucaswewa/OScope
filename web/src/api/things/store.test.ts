// A Thing's store (W1.2): its properties kept live, by its socket or by
// reading them; writes shown at once and taken back if refused; staleness.
// On a stand-in socket and network, with the clock held by the test.
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  described,
  FakeSocket,
  problem,
  standInNetwork,
  standInSockets,
} from "../../../tests/setup/stand-ins.ts";
import { Server } from "../client.ts";
import { STALE_AFTER, thingStore } from "./store.ts";

let on: Server;
let reads: { position: number; speed: number; status: number };
let position: number;
let made: Request[];

const STAGE = described({ position: {}, speed: { observable: true, writable: true }, status: {} }, [
  "moved",
  "alarm",
]);

beforeEach(() => {
  setActivePinia(createPinia());
  on = new Server("http://127.0.0.1:8211");
  reads = { position: 0, speed: 0, status: 0 };
  position = 1;
  standInSockets();
  made = standInNetwork({
    "GET /api/v1/thing_descriptions/": () => ({ stage: STAGE }),
    "GET /api/v1/stage/position": () => (reads.position++, { x: position }),
    "GET /api/v1/stage/speed": () => (reads.speed++, 1000),
    "PUT /api/v1/stage/speed": async (request) => {
      const value = (await request.json()) as number;
      return value > 25_000
        ? problem(400, "out of range", `the speed must be at most 25000, and was ${value}`)
        : new Response(null, { status: 204 });
    },
  });
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "performance"],
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

/** Lets the stand-ins answer, and the clock move on by `ms`. */
const later = (ms = 0) => vi.advanceTimersByTimeAsync(ms);

describe("a Thing's store", () => {
  it("reads a property its socket can't keep at once, then every `every` ms", async () => {
    const store = thingStore("stage", on);
    const stop = store.follow("position", 250);
    await later();
    expect(store.kept.get("position")?.value).toEqual({ x: 1 });
    position = 2;
    await later(250);
    expect(store.kept.get("position")?.value).toEqual({ x: 2 });
    await later(750);
    expect(reads.position).toBe(5);
    stop();
    await later(1000);
    expect(reads.position).toBe(5);
  });

  it("keeps an observable property by its socket, read at first and as the socket opens", async () => {
    const store = thingStore("stage", on);
    const stop = store.follow("speed");
    await later();
    FakeSocket.last.opens();
    await later();
    expect(FakeSocket.last.sent).toContainEqual({
      messageType: "addPropertyObservation",
      data: { speed: {} },
    });
    expect(store.kept.get("speed")?.value).toBe(1000);
    FakeSocket.last.says({ messageType: "propertyStatus", data: { speed: 4000 } });
    expect(store.kept.get("speed")?.value).toBe(4000);
    await later(5_000);
    expect([store.kept.get("speed")?.value, reads.speed]).toEqual([4000, 2]);
    stop();
  });

  it("reads again what its socket keeps each time it opens: what changed meanwhile isn't told", async () => {
    let speed = 1000;
    standInNetwork({
      "GET /api/v1/thing_descriptions/": () => ({ stage: STAGE }),
      "GET /api/v1/stage/speed": () => (reads.speed++, speed),
    });
    const store = thingStore("stage", on);
    const stop = store.follow("speed");
    await later();
    // Another client's write, before the server keeps the speed for this one.
    speed = 4000;
    FakeSocket.last.opens();
    await later();
    expect(store.kept.get("speed")?.value).toBe(4000);
    // The socket drops, and the speed changes while it is closed.
    FakeSocket.last.drops();
    speed = 6000;
    await later(10_000);
    FakeSocket.last.opens();
    await later();
    expect([store.kept.get("speed")?.value, reads.speed]).toEqual([6000, 3]);
    stop();
  });

  it("reads again a property it starts to keep on a socket already open", async () => {
    let speed = 1000;
    standInNetwork({
      "GET /api/v1/thing_descriptions/": () => ({ stage: STAGE }),
      "GET /api/v1/stage/position": () => ({ x: 1 }),
      // Another client writes the speed just after it is first read.
      "GET /api/v1/stage/speed": () => {
        reads.speed++;
        const now = speed;
        speed = 4000;
        return now;
      },
    });
    const store = thingStore("stage", on);
    const stops = [store.follow("position")];
    await later();
    FakeSocket.last.opens();
    await later();
    stops.push(store.follow("speed"));
    await later();
    expect([store.kept.get("speed")?.value, reads.speed]).toEqual([4000, 2]);
    for (const stop of stops) stop();
  });

  it("reads again what its socket doesn't keep when the Thing tells an event, and passes the event on", async () => {
    const store = thingStore("stage", on);
    const told: unknown[] = [];
    const stops = [store.follow("position", 60_000), store.on("moved", (one) => told.push(one))];
    await later();
    FakeSocket.last.opens();
    expect(FakeSocket.last.sent).toContainEqual({
      messageType: "addEventSubscription",
      data: { moved: {}, alarm: {} },
    });
    position = 7;
    const event = { moved: { data: { position: { x: 7 } }, timestamp: "2026-11-18T12:00:00Z" } };
    FakeSocket.last.says({ messageType: "event", data: event });
    await later();
    expect(store.kept.get("position")?.value).toEqual({ x: 7 });
    expect(told).toEqual([{ data: { position: { x: 7 } }, time: "2026-11-18T12:00:00Z" }]);
    for (const stop of stops) stop();
  });

  it("shows a write at once, and keeps it once the server takes it", async () => {
    const store = thingStore("stage", on);
    const stop = store.follow("speed");
    await later();
    const writing = store.write("speed", 5000);
    expect(store.kept.get("speed")?.writing).toEqual({ value: 5000 });
    await writing;
    expect(store.kept.get("speed")).toMatchObject({
      value: 5000,
      writing: undefined,
      refused: undefined,
    });
    const put = made.find((request) => request.method === "PUT")!;
    expect(new URL(put.url).pathname).toBe("/api/v1/stage/speed");
    stop();
  });

  it("takes a refused write back, with the server's reason", async () => {
    const store = thingStore("stage", on);
    const stop = store.follow("speed");
    await later();
    await store.write("speed", 99_999);
    const speed = store.kept.get("speed")!;
    expect([speed.value, speed.writing]).toEqual([1000, undefined]);
    expect(speed.refused).toMatchObject({
      failure: "refused",
      refusal: "out of range",
      message: "the speed must be at most 25000, and was 99999",
    });
    // The next write starts afresh.
    await store.write("speed", 2000);
    expect(store.kept.get("speed")?.refused).toBeUndefined();
    stop();
  });

  it("marks stale a value the server hasn't confirmed for 5 s, and live again once it does", async () => {
    const store = thingStore("stage", on);
    const stops = [store.follow("position", 1000), store.follow("speed")];
    await later();
    FakeSocket.last.opens();
    await later(STALE_AFTER + 2000);
    expect([store.isStale("position"), store.isStale("speed")]).toEqual([false, false]);
    // The server goes: its reads fail, and its socket closes.
    vi.stubGlobal("fetch", () => Promise.reject(new TypeError("Failed to fetch")));
    FakeSocket.last.drops();
    await later(STALE_AFTER - 1000);
    expect([store.isStale("position"), store.isStale("speed")]).toEqual([false, false]);
    await later(2000);
    expect([store.isStale("position"), store.isStale("speed")]).toEqual([true, true]);
    expect(store.kept.get("position")?.failed?.failure).toBe("unreachable");
    // It comes back.
    standInNetwork({ "GET /api/v1/stage/position": () => ({ x: 9 }) });
    await later(10_000);
    FakeSocket.last.opens();
    await later(1000);
    expect([store.isStale("position"), store.isStale("speed")]).toEqual([false, false]);
    expect(store.kept.get("position")?.value).toEqual({ x: 9 });
    for (const stop of stops) stop();
  });

  it("has one reader of a property for all who follow it, at the quickest asked, and stops after the last", async () => {
    const store = thingStore("stage", on);
    const slow = store.follow("position", 1000);
    const quick = store.follow("position", 250);
    await later();
    const from = reads.position;
    await later(1000);
    expect(reads.position - from).toBe(4);
    quick();
    slow();
    expect(FakeSocket.last.readyState).toBe(FakeSocket.CLOSED);
    const stopped = reads.position;
    await later(5000);
    expect(reads.position).toBe(stopped);
  });

  it("is one store for each Thing on a server", () => {
    expect(thingStore("stage", on)).toBe(thingStore("stage", on));
    expect(thingStore("camera", on)).not.toBe(thingStore("stage", on));
  });
});
