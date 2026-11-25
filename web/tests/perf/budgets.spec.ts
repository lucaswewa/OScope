// The app's budgets (W10.3, ADR-0059), measured in Chromium against the
// server `cargo xtask perf --web` built in release: each test prints what it
// measured as `perf: KEY VALUE` lines, as the server's benchmarks do, and
// xtask judges them against their budgets. Run alone, against the debug
// server, the numbers say little.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { gzipSync } from "node:zlib";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { expect, test } from "../e2e/fixtures.ts";
import { imported, made } from "../e2e/records.ts";
import { web } from "../e2e/server.ts";
import { clockAt, FAST } from "../e2e/sim.ts";
import { tiffOf } from "../e2e/tiff.ts";
import { slopeOf, steady } from "./slope.ts";

/** A measure, as xtask reads it. */
const measured = (key: string, value: number) => console.log(`perf: ${key} ${value.toFixed(1)}`);

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const readout = (page: Page, name: string) => said(page, name).locator("dd");

/**
 * W1.5's measure: a region of 1920 × 1272 in the sensor's middle, in mode
 * `design`, full resolution in 16 bits at P29's 14.5 fps, over the tissue.
 */
async function streaming(url: string): Promise<void> {
  const on = new Api(url);
  await clockAt(url, FAST);
  try {
    await act("/api/v1/stage/home", {}, { on, timeout: 60_000 });
    await act("/api/v1/stage/move_to", { x: 40_000, y: 12_000, z: 5_000, speed: 25_000 }, { on });
  } finally {
    await clockAt(url, 1);
  }
  const design = {
    mode: "design",
    format: { kind: "bayer16", pattern: "RGGB", significant_bits: 12 },
  };
  await act("/api/v1/camera/set_mode", design as never, { on });
  const region = { x: 1776, y: 1188, width: 1920, height: 1272 };
  await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: region, signal }));
  await act("/api/v1/camera/start_streaming", {}, { on });
}

/** A number in a readout's words: `14.5 fps`, `40 ms`. */
const numberIn = async (page: Page, name: string) =>
  Number.parseFloat((await readout(page, name).innerText()).replace(/[^\d.]/g, ""));

test("the bundle: its entry, its largest chunk and the whole, gzipped", () => {
  const assets = join(web, "dist", "assets");
  const chunks = readdirSync(assets)
    .filter((name) => name.endsWith(".js"))
    .map((name) => ({ name, kb: gzipSync(readFileSync(join(assets, name))).length / 1024 }));
  expect(chunks.length).toBeGreaterThan(0);
  const entry = chunks.find((chunk) => chunk.name.startsWith("index-"))!;
  measured("web.bundle.entry_kb", entry.kb);
  measured("web.bundle.largest_kb", Math.max(...chunks.map((chunk) => chunk.kb)));
  measured(
    "web.bundle.total_kb",
    chunks.reduce((sum, chunk) => sum + chunk.kb, 0),
  );
});

test("the live view's rate and latency in the page", async ({ page, server }) => {
  test.setTimeout(300_000);
  await streaming(server.url);
  await page.goto(`${server.url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  // Its rate and latency, as the readout says them over its own window: ten
  // readings a second apart once frames come, their medians.
  await expect.poll(() => numberIn(page, "rate"), { timeout: 60_000 }).toBeGreaterThan(0);
  const rates: number[] = [];
  const latencies: number[] = [];
  for (let reading = 0; reading < 10; reading++) {
    await page.waitForTimeout(1_000);
    rates.push(await numberIn(page, "rate"));
    latencies.push(await numberIn(page, "latency"));
  }
  const middle = (values: number[]) => [...values].sort((a, b) => a - b)[values.length >> 1]!;
  measured("web.live.fps", middle(rates));
  measured("web.live.latency_ms", middle(latencies));
});

test("the time to the first frame after opening the app", async ({ page, server }) => {
  test.setTimeout(300_000);
  await streaming(server.url);
  // Read where it happens: the moment the readout first says the image is live.
  await page.addInitScript(() => {
    const kept = globalThis as unknown as { firstFrame?: number };
    const observer = new MutationObserver(() => {
      const state = document.querySelector('[data-said="state"] dd');
      if (state?.textContent?.trim() === "Live" && kept.firstFrame === undefined) {
        kept.firstFrame = performance.now();
        observer.disconnect();
      }
    });
    observer.observe(document, { subtree: true, childList: true, characterData: true });
  });
  await page.goto(`${server.url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  const ms = await page.evaluate(
    () => (globalThis as unknown as { firstFrame?: number }).firstFrame ?? Number.NaN,
  );
  expect(Number.isFinite(ms)).toBe(true);
  measured("web.first_frame_ms", ms);
});

test("a full-screen view of a region never seen, deep-zoomed", async ({ page, server }) => {
  test.setTimeout(600_000);
  await page.setViewportSize({ width: 1920, height: 1080 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  const project = await made(server.url, "Measured");
  const record = await imported(server.url, project, "large.tif", tiffOf(8192, 6144));
  const query = new URLSearchParams({ project, record, view: "1" });
  await page.goto(`${server.url}/#/gallery?${query.toString()}`);
  const viewer = said(page, `viewer-${record}`);
  const tiles = viewer.locator('[data-said="tiles"]');
  await expect(tiles).toHaveAttribute("data-sharp", "true", { timeout: 60_000 });
  await page.getByRole("button", { name: "Full screen" }).click();
  // Near one pixel of the image to one of the screen.
  for (let step = 0; step < 5; step++) {
    await viewer.getByRole("button", { name: "Zoom in" }).click();
    await expect(tiles).toHaveAttribute("data-sharp", "true", { timeout: 60_000 });
  }
  // Jumps by the navigator, a fifth of the image apart, more than a screen,
  // each to tiles never made: timed in the page, from the click until every
  // tile in view has come, OpenSeadragon's fully loaded; 10 s at most.
  const navigator = (await viewer.locator(".navigator").boundingBox())!;
  const shown = Math.min(navigator.width / 8192, navigator.height / 6144);
  const [across, down] = [8192 * shown, 6144 * shown];
  const [left, top] = [
    navigator.x + (navigator.width - across) / 2,
    navigator.y + (navigator.height - down) / 2,
  ];
  const times: number[] = [];
  for (let trial = 0; trial < 20; trial++) {
    await tiles.evaluate((element) => {
      (globalThis as unknown as { measured: Promise<number> }).measured = new Promise<number>(
        (done) => {
          let start: number | undefined;
          let blurred = false;
          const clicked = () => (start ??= performance.now());
          addEventListener("pointerup", clicked, { capture: true });
          const observer = new MutationObserver(() => {
            const sharp = element.getAttribute("data-sharp") === "true";
            if (!sharp) blurred = true;
            else if (blurred && start !== undefined) end(performance.now() - start);
          });
          const end = (ms: number) => {
            observer.disconnect();
            removeEventListener("pointerup", clicked, { capture: true });
            done(ms);
          };
          observer.observe(element, { attributes: true, attributeFilter: ["data-sharp"] });
          setTimeout(() => end(Number.NaN), 10_000);
        },
      );
    });
    const [column, row] = [trial % 5, Math.floor(trial / 5)];
    await page.mouse.click(left + across * (0.1 + 0.2 * column), top + down * (0.125 + 0.25 * row));
    times.push(
      await page.evaluate(() => (globalThis as unknown as { measured: Promise<number> }).measured),
    );
  }
  // A view that never blurred, or never sharpened within 10 s, is a NaN: none may be.
  expect(times.filter((ms) => !Number.isFinite(ms))).toEqual([]);
  const sorted = [...times].sort((p, q) => p - q);
  measured("web.deep_zoom.median_ms", sorted[sorted.length >> 1]!);
  measured("web.deep_zoom.slowest_ms", sorted.at(-1)!);
});

/** How long the memory is watched, in minutes: OSCOPE_PERF_MINUTES, 2 by default. */
const minutes = Number(process.env.OSCOPE_PERF_MINUTES ?? "2");

test("the memory the page keeps over a long live view", async ({ page, server }) => {
  test.setTimeout((minutes + 5) * 60_000);
  await streaming(server.url);
  await page.goto(`${server.url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  // The heap after a collection, a sample every 15 s: Chromium's own count.
  const cdp = await page.context().newCDPSession(page);
  const samples: [number, number][] = [];
  const started = Date.now();
  while (Date.now() - started < minutes * 60_000) {
    await cdp.send("HeapProfiler.collectGarbage");
    const { usedSize } = await cdp.send("Runtime.getHeapUsage");
    samples.push([(Date.now() - started) / 3_600_000, usedSize / 1_048_576]);
    await expect(readout(page, "state")).toHaveText("Live");
    await page.waitForTimeout(15_000);
  }
  const perHour = slopeOf(steady(samples));
  console.log(`heap, MB: ${samples.map(([, mb]) => mb.toFixed(1)).join(", ")}`);
  expect(Number.isFinite(perHour)).toBe(true);
  measured("web.memory.mb_per_hour", perHour);
  measured("web.memory.minutes", minutes);
});
