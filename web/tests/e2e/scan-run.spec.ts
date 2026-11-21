// Covers: web/src/features/scan, web/src/features/projects
// Running a scan (W5.2), on the simulator, through the camera's tiny
// region, over the default sample's tissue: a scan runs to its end, each
// tile marked as it is taken, and its stitch job is queued; a pause holds
// the stage until it is resumed; a cancel leaves what was taken in
// scratch; axe; and a scan's end's baseline.
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { jobList } from "../../src/api/jobs.ts";
import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { estimate, said, typeRegion } from "./scanning.ts";
import { overTissueTiny } from "./sim.ts";

const HIDE_TILES = fileURLToPath(new URL("hide-tiles.css", import.meta.url));
/** Over the tissue, in mm: a few tiles; and many, for a scan stopped on its way. */
const FEW = [39.85, 11.9, 0.15, 0.1];
const MANY = [39.85, 11.9, 0.3, 0.2];

const progressOf = (url: string) =>
  call((signal) => new Api(url).api.GET("/api/v1/scan/progress", { signal }));
const where = (url: string) =>
  call((signal) => new Api(url).api.GET("/api/v1/stage/position", { signal }));

/**
 * The stage over the tissue in focus, the camera's tiny region streaming, a
 * project, and the scan tab's snake over `region`, in mm, run into it: the
 * tiles it plans.
 */
async function scanned(page: Page, url: string, region: number[]): Promise<number> {
  await overTissueTiny(url);
  const on = new Api(url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  await act("/api/v1/projects/create", { name: "Scans" }, { on });
  await page.goto(`${url}/#/scan`);
  await typeRegion(page, "plan-region", region);
  await expect(estimate(page, "Tiles")).toHaveText(/^[1-9]\d*$/, { timeout: 30_000 });
  const tiles = Number(await estimate(page, "Tiles").textContent());
  await expect(estimate(page, "Data")).toHaveText(/^\d+(\.\d)? (B|kB|MB|GB)$/);
  await choose(page, "Project", "Scans");
  await said(page, "run").getByRole("button", { name: "Run the scan" }).click();
  return tiles;
}

test("a scan runs to its end, each tile marked as taken, and its stitch is queued @main", async ({
  page,
  server,
}) => {
  test.setTimeout(180_000);
  const tiles = await scanned(page, server.url, FEW);
  await expect(said(page, "scan-tiles")).toHaveText(`Completed: ${tiles} of ${tiles} tiles`, {
    timeout: 120_000,
  });
  await expect(said(page, "shapes").locator("polygon.done")).toHaveCount(tiles);
  expect((await progressOf(server.url))?.done).toHaveLength(tiles);
  const made = said(page, "made");
  await expect(made.getByRole("link", { name: "Open it in the Gallery" })).toBeVisible();
  await made.getByRole("button", { name: "Stitch it" }).click();
  await expect(said(page, "stitch")).toContainText(/^Stitching: (Queued|Running|Completed)/);
  const stitches = await jobList({ kind: "stitch.run" }, { on: new Api(server.url) });
  expect(stitches).toHaveLength(1);
});

test("a pause holds the stage until the scan is resumed", async ({ page, server }) => {
  test.setTimeout(180_000);
  await scanned(page, server.url, MANY);
  await expect(said(page, "scan-tiles")).toHaveText(/^Running: [1-9]\d* of \d+ tiles$/, {
    timeout: 60_000,
  });
  await said(page, "scan").getByRole("button", { name: "Pause" }).click();
  await expect(said(page, "scan-tiles")).toHaveText(/^Paused: /, { timeout: 30_000 });
  const before = (await where(server.url))!;
  const done = (await progressOf(server.url))!.done.length;
  // Three seconds of the simulator's time, many tiles' worth: the stage
  // stays where it is, and no tile is taken.
  await expect
    .poll(async () => (await where(server.url))!.time - before.time, { timeout: 30_000 })
    .toBeGreaterThanOrEqual(3e9);
  const after = (await where(server.url))!;
  for (const axis of ["x", "y", "z"] as const) {
    expect(Math.abs(after.position[axis] - before.position[axis]), axis).toBeLessThan(0.5);
  }
  expect((await progressOf(server.url))!.done).toHaveLength(done);
  await expect(said(page, "scan-tiles")).toHaveText(/^Paused: /);
  await said(page, "scan").getByRole("button", { name: "Resume" }).click();
  await expect(said(page, "scan-tiles")).toHaveText(/^Completed: (\d+) of \1 tiles$/, {
    timeout: 120_000,
  });
});

test("a cancel leaves what was taken in scratch", async ({ page, server }) => {
  test.setTimeout(180_000);
  const tiles = await scanned(page, server.url, MANY);
  await expect(said(page, "scan-tiles")).toHaveText(/^Running: [1-9]\d* of \d+ tiles$/, {
    timeout: 60_000,
  });
  await said(page, "scan").getByRole("button", { name: "Cancel…" }).click();
  await page.getByRole("button", { name: "Cancel the scan" }).click();
  await expect(said(page, "scan-tiles")).toHaveText(/^Cancelled: /, { timeout: 60_000 });
  await expect(said(page, "kept")).toContainText(/recording \S+, kept in scratch/);
  const done = (await progressOf(server.url))!.done.length;
  expect(done).toBeGreaterThan(0);
  expect(done).toBeLessThan(tiles);
  await expect(said(page, "made")).toHaveCount(0);
});

test("a scan's end has no violation axe can find, in either theme", async ({ page, server }) => {
  test.setTimeout(180_000);
  const tiles = await scanned(page, server.url, FEW);
  await expect(said(page, "scan-tiles")).toHaveText(`Completed: ${tiles} of ${tiles} tiles`, {
    timeout: 120_000,
  });
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("a scan's end looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(180_000);
  const tiles = await scanned(page, server.url, FEW);
  await expect(said(page, "scan-tiles")).toHaveText(`Completed: ${tiles} of ${tiles} tiles`, {
    timeout: 120_000,
  });
  await expect(said(page, "shapes").locator("polygon.done")).toHaveCount(tiles);
  // The rail's indicator says the instrument is idle again, as in W5.1's baseline.
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  // The scan's end's toast, once it has come, dismissed: it leaves when it will.
  const toasts = page.getByRole("region", { name: "Notifications (F8)" });
  await expect(toasts.getByText(`scan ${tiles} tiles completed.`, { exact: true })).toBeVisible();
  await toasts.getByRole("button", { name: "Dismiss" }).click();
  await expect(toasts.getByRole("listitem")).toHaveCount(0);
  // No field focused, the pane at its end, where the scan is said.
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await said(page, "scan").scrollIntoViewIfNeeded();
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The camera's field of view follows the stage: masked.
    await expect(page).toHaveScreenshot(`scan-run-${scheme}.png`, {
      stylePath: HIDE_TILES,
      mask: [said(page, "field"), said(page, "at")],
    });
    await shot(page, `W5.2-scan-${scheme}`);
  }
});
