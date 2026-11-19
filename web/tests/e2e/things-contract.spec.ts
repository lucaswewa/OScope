// Contract tests (W1.2, ADR-0044): live Things against a real server, each
// test's own. No page: they run in Chromium's project only, as untagged
// tests do.
import { createPinia, setActivePinia } from "pinia";

import { act } from "../../src/api/actions.ts";
import { call, Server } from "../../src/api/client.ts";
import { thingStore, type Told } from "../../src/api/things/store.ts";
import { expect, test } from "./fixtures.ts";
import { fast } from "./sim.ts";

test.beforeEach(() => {
  setActivePinia(createPinia());
});

/** Another client's write of the stage's speed. */
const writeSpeed = (url: string, speed: number) =>
  call((signal) => new Server(url).api.PUT("/api/v1/stage/speed", { body: speed, signal }));

test("a property is followed through another client's writes, by its Thing's socket", async ({
  server,
}) => {
  const stage = thingStore("stage", new Server(server.url));
  const stop = stage.follow("speed");
  await expect.poll(() => stage.open).toBe(true);
  for (const speed of [4000, 6000, 8000]) {
    await writeSpeed(server.url, speed);
    await expect.poll(() => stage.kept.get("speed")?.value).toBe(speed);
  }
  expect(stage.kept.get("speed")?.observed).toBe(true);
  stop();
});

test("a property its socket can't keep is followed by reading it", async ({ server }) => {
  test.setTimeout(60_000);
  const on = new Server(server.url);
  const stage = thingStore("stage", on);
  const stop = stage.follow("position", 200);
  await fast(server.url, async () => {
    await act("/api/v1/stage/home", {}, { on: new Server(server.url), timeout: 30_000 });
    await act(
      "/api/v1/stage/move_to",
      { x: 3000, y: 2000, z: 100 },
      { on: new Server(server.url) },
    );
  });
  await expect
    .poll(() => {
      const kept = stage.kept.get("position")?.value as { position: { x: number } } | undefined;
      // The simulated stage stops within a few µm of its target.
      return Math.abs((kept?.position.x ?? -1) - 3000) < 5;
    })
    .toBe(true);
  expect(stage.kept.get("position")?.observed).toBe(false);
  stop();
});

test("events come in the order they happened", async ({ server }) => {
  test.setTimeout(60_000);
  const on = new Server(server.url);
  const stage = thingStore("stage", on);
  const told: Told[] = [];
  const stop = stage.on("moved", (one) => told.push(one));
  await expect.poll(() => stage.open).toBe(true);
  const other = new Server(server.url);
  const targets = [1000, 2000, 3000];
  await fast(server.url, async () => {
    await act("/api/v1/stage/home", {}, { on: other, timeout: 30_000 });
    for (const x of targets) await act("/api/v1/stage/move_to", { x, y: 0, z: 0 }, { on: other });
  });
  await expect.poll(() => told.length).toBeGreaterThanOrEqual(targets.length + 1);
  const xs = told.slice(-3).map((one) => (one.data as { position: { x: number } }).position.x);
  // Each within a few µm of its target, as the simulated stage stops.
  expect(xs.map((x, n) => Math.abs(x - targets[n]!) < 5)).toEqual([true, true, true]);
  const times = told.map((one) => Date.parse(one.time));
  expect([...times].sort((a, b) => a - b)).toEqual(times);
  stop();
});

test("the socket opens again after the server restarts, and keeps what it kept", async ({
  server,
}) => {
  test.setTimeout(60_000);
  const stage = thingStore("stage", new Server(server.url));
  const stop = stage.follow("speed");
  await expect.poll(() => stage.open).toBe(true);
  await server.stop();
  await expect.poll(() => stage.open).toBe(false);
  await server.start();
  await expect.poll(() => stage.open, { timeout: 30_000 }).toBe(true);
  await writeSpeed(server.url, 7000);
  await expect.poll(() => stage.kept.get("speed")?.value).toBe(7000);
  stop();
});
