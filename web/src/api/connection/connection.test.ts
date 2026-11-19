// The connection (W1.3): its states, from the server's `/health` and its
// version, on a stand-in network with the clock held by the test.
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { problem, standInNetwork } from "../../../tests/setup/stand-ins.ts";
import { Server } from "../client.ts";
import { APP_VERSION, knows, useConnection, WATCH_EVERY } from "./connection.ts";

let on: Server;
let up: boolean;
let version: unknown;

beforeEach(() => {
  setActivePinia(createPinia());
  on = new Server("http://127.0.0.1:8211");
  up = true;
  version = APP_VERSION;
  standInNetwork({
    "GET /health": () => (up ? { status: "ok" } : Promise.reject(new TypeError("Failed to fetch"))),
    "GET /api/v1/system/version": () => version,
  });
  vi.useFakeTimers({
    toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "performance"],
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

const later = (ms = 0) => vi.advanceTimersByTimeAsync(ms);

describe("a server's version", () => {
  it("is known when its major and minor numbers are the app's", () => {
    expect(knows("0.1.7", "0.1.1")).toBe(true);
    expect(knows("0.2.0", "0.1.1")).toBe(false);
    expect(knows("1.1.1", "0.1.1")).toBe(false);
    expect(knows(APP_VERSION)).toBe(true);
  });
});

describe("the connection", () => {
  it("is connecting until the server first answers, then connected", async () => {
    up = false;
    const connection = useConnection();
    const stop = connection.watch(on);
    await later();
    expect(connection.state).toBe("connecting");
    up = true;
    await later(WATCH_EVERY);
    expect([connection.state, connection.serverVersion]).toEqual(["connected", APP_VERSION]);
    stop();
  });

  it("is reconnecting once it had the server and lost it, and connected when it returns", async () => {
    const connection = useConnection();
    const stop = connection.watch(on);
    await later();
    expect(connection.state).toBe("connected");
    up = false;
    await later(WATCH_EVERY);
    expect(connection.state).toBe("reconnecting");
    up = true;
    await later(WATCH_EVERY);
    expect(connection.state).toBe("connected");
    stop();
  });

  it("is refused when the server answers, and isn't well", async () => {
    standInNetwork({
      "GET /health": () =>
        problem(403, "origin not allowed", "a page at … may not call this server"),
    });
    const connection = useConnection();
    const stop = connection.watch(on);
    await later();
    expect(connection.state).toBe("refused");
    stop();
  });

  it("is a version the app doesn't know when the server is another release's", async () => {
    version = "9.0.0";
    const connection = useConnection();
    const stop = connection.watch(on);
    await later();
    expect(connection.state).toBe("unknown version");
    stop();
  });

  it("reads the version once the server lets it, as after a login under security", async () => {
    version = problem(401, "Unauthorized", "a session's token is needed");
    const connection = useConnection();
    const stop = connection.watch(on);
    await later();
    expect([connection.state, connection.serverVersion]).toEqual(["connected", undefined]);
    version = APP_VERSION;
    await connection.readVersion();
    expect(connection.serverVersion).toBe(APP_VERSION);
    stop();
  });

  it("stops asking when told", async () => {
    const made = standInNetwork({ "GET /health": () => ({ status: "ok" }) });
    const connection = useConnection();
    const stop = connection.watch(on);
    await later(WATCH_EVERY * 2);
    stop();
    const asked = made.length;
    await later(WATCH_EVERY * 5);
    expect(made.length).toBe(asked);
  });
});
