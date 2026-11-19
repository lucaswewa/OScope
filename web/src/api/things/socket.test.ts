// A Thing's WebSocket (W1.2): what it asks, what it hears, and how it opens
// again, on a stand-in socket and the clock held by the test.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { FakeSocket, standInSockets } from "../../../tests/setup/stand-ins.ts";
import { Server, session } from "../client.ts";
import { heardIn, REOPEN, ThingSocket } from "./socket.ts";

const on = new Server("http://127.0.0.1:8211");

beforeEach(() => {
  standInSockets();
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "performance"],
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  delete session.token;
});

describe("a message", () => {
  it("tells a property's value, an event, or a refusal", () => {
    expect(heardIn('{"messageType":"propertyStatus","data":{"speed":5000}}')).toEqual([
      { kind: "property", name: "speed", value: 5000 },
    ]);
    const event = { moved: { data: { position: { x: 1 } }, timestamp: "2026-11-18T12:00:00Z" } };
    expect(heardIn(JSON.stringify({ messageType: "event", data: event }))).toEqual([
      { kind: "event", name: "moved", data: { position: { x: 1 } }, time: "2026-11-18T12:00:00Z" },
    ]);
    const refused = {
      messageType: "response",
      operation: "observeproperty",
      name: "nonsense",
      error: { status: "404", detail: "No property found with the name 'nonsense'." },
    };
    expect(heardIn(JSON.stringify(refused))).toEqual([
      {
        kind: "refused",
        name: "nonsense",
        operation: "observeproperty",
        status: 404,
        detail: "No property found with the name 'nonsense'.",
      },
    ]);
  });

  it("tells nothing when it isn't one", () => {
    for (const text of ["not JSON", "[1]", '{"messageType":"actionStatus","data":{}}', "{}"]) {
      expect(heardIn(text)).toEqual([]);
    }
  });
});

describe("a Thing's socket", () => {
  it("opens at the first listener, at the Thing's address, and asks for what it was asked", () => {
    const socket = new ThingSocket("stage", on);
    socket.observe("speed");
    const stop = socket.listen(() => {});
    expect(FakeSocket.last.url).toBe("ws://127.0.0.1:8211/api/v1/stage/ws");
    FakeSocket.last.opens();
    socket.subscribe("moved");
    expect(FakeSocket.last.sent).toEqual([
      { messageType: "addPropertyObservation", data: { speed: {} } },
      { messageType: "addEventSubscription", data: { moved: {} } },
    ]);
    expect(socket.open.value).toBe(true);
    stop();
  });

  it("asks for each name once, however often it is asked", () => {
    const socket = new ThingSocket("stage", on);
    const stop = socket.listen(() => {});
    FakeSocket.last.opens();
    socket.observe("speed");
    socket.observe("speed");
    expect(FakeSocket.last.sent).toHaveLength(1);
    stop();
  });

  it("sends the session's token first, under security (ADR-0031)", () => {
    session.token = "a token";
    const socket = new ThingSocket("stage", on);
    socket.observe("speed");
    const stop = socket.listen(() => {});
    FakeSocket.last.opens();
    expect(FakeSocket.last.sent[0]).toEqual({ authorization: "Bearer a token" });
    stop();
  });

  it("tells each listener what it hears", () => {
    const socket = new ThingSocket("stage", on);
    const heard: unknown[] = [];
    const stops = [
      socket.listen((one) => heard.push(["a", one])),
      socket.listen((one) => heard.push(["b", one])),
    ];
    FakeSocket.last.opens();
    FakeSocket.last.says({ messageType: "propertyStatus", data: { speed: 7 } });
    const one = { kind: "property", name: "speed", value: 7 };
    expect(heard).toEqual([
      ["a", one],
      ["b", one],
    ]);
    for (const stop of stops) stop();
  });

  it("closes after its last listener, and stays closed", () => {
    const socket = new ThingSocket("stage", on);
    const first = socket.listen(() => {});
    const second = socket.listen(() => {});
    FakeSocket.last.opens();
    first();
    expect(FakeSocket.last.readyState).toBe(FakeSocket.OPEN);
    second();
    second();
    expect(FakeSocket.last.readyState).toBe(FakeSocket.CLOSED);
    vi.advanceTimersByTime(REOPEN.most * 2);
    expect(FakeSocket.made).toHaveLength(1);
  });

  it("opens again when the server drops it, waiting twice as long each time, to 10 s", () => {
    const socket = new ThingSocket("stage", on);
    const stop = socket.listen(() => {});
    const waits: number[] = [];
    for (let n = 0; n < 8; n++) {
      FakeSocket.last.drops();
      expect(socket.open.value).toBe(false);
      const from = performance.now();
      const made = FakeSocket.made.length;
      // At most a minute of the held clock: a socket that never opens again
      // fails the test, rather than spinning it for ever.
      while (FakeSocket.made.length === made && performance.now() - from < 60_000) {
        vi.advanceTimersByTime(10);
      }
      waits.push(performance.now() - from);
    }
    expect(waits).toEqual([250, 500, 1000, 2000, 4000, 8000, 10_000, 10_000]);
    stop();
  });

  it("waits 250 ms again once it had stayed open 5 s, and asks again for all it was asked", () => {
    const socket = new ThingSocket("stage", on);
    socket.observe("speed");
    socket.subscribe("moved");
    const stop = socket.listen(() => {});
    FakeSocket.last.drops();
    vi.advanceTimersByTime(250);
    FakeSocket.last.drops();
    vi.advanceTimersByTime(500);
    FakeSocket.last.opens();
    vi.advanceTimersByTime(REOPEN.settled);
    FakeSocket.last.drops();
    const made = FakeSocket.made.length;
    vi.advanceTimersByTime(250);
    expect(FakeSocket.made).toHaveLength(made + 1);
    FakeSocket.last.opens();
    expect(FakeSocket.last.sent).toEqual([
      { messageType: "addPropertyObservation", data: { speed: {} } },
      { messageType: "addEventSubscription", data: { moved: {} } },
    ]);
    stop();
  });

  it("knows when it last heard from the server", () => {
    const socket = new ThingSocket("stage", on);
    const stop = socket.listen(() => {});
    FakeSocket.last.opens();
    const opened = socket.heard.value;
    vi.advanceTimersByTime(3_000);
    FakeSocket.last.says({ messageType: "propertyStatus", data: { speed: 1 } });
    expect(socket.heard.value - opened).toBe(3_000);
    stop();
  });
});
