// Covers: web/src/features/scan
// The histo scan (W5.3), on the simulator: the regions proposed cover the
// tissue the overview found; an edited region is what is scanned, its
// focus map measured before its first tile and shown; axe; and the
// regions' baseline. The overview is made over the H&E slide's tissue's
// edge and glass, through the camera's small region, 25 frames, light on a
// computer the other specs share; the scan through its tiny region.
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { finished } from "../../src/api/jobs.ts";
import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { said, typeRegion } from "./scanning.ts";
import { aSmallRegion, fast, TINY } from "./sim.ts";

const HIDE_TILES = fileURLToPath(new URL("hide-tiles.css", import.meta.url));
/** The overview's region, in µm: the tissue's edge, with its islands, and glass. */
const OVERVIEW = { x: 41_300, y: 10_900, width: 1_000, height: 750 };
/** Where it is in focus, and an edited region in it, in mm, a few of the tiny region's tiles. */
const AT = { x: 41_400, y: 11_200 };
const EDITED = [41.35, 11.15, 0.15, 0.1];

/** The stage homed, at the tissue's edge in focus; the overview made, by the server. */
async function overviewed(url: string): Promise<void> {
  const on = new Api(url);
  await fast(url, async () => {
    await act("/api/v1/stage/home", {}, { on, timeout: 30_000 });
    const z = await act("/api/v1/simulation/in_focus", AT, { on });
    await act("/api/v1/stage/move_to", { ...AT, z: z as number, speed: 25_000 }, { on });
  });
  await aSmallRegion(url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const region = {
    rect: {
      origin: { x: OVERVIEW.x, y: OVERVIEW.y },
      size: { width: OVERVIEW.width, height: OVERVIEW.height },
    },
  };
  const job = await act("/api/v1/scan/overview", { region }, { on });
  await finished(job, { on, timeout: 120_000 });
}

/** The histo scan's flow, at its regions. */
async function atRegions(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/scan`);
  await page.getByRole("tab", { name: "Histo scan" }).click();
  const steps = said(page, "histo");
  await steps.getByRole("button", { name: "3. The regions" }).click();
  await expect(said(page, "regions")).toBeVisible();
}

/** Region `n`'s rectangle, in µm, as its fields say. */
async function regionOf(page: Page, n: number) {
  const fields = said(page, `region-${n}`);
  const mm = async (label: string) =>
    Number(await fields.getByLabel(label, { exact: true }).inputValue()) * 1000;
  return {
    x: await mm("x"),
    y: await mm("y"),
    width: await mm("Width"),
    height: await mm("Height"),
  };
}

test("the regions proposed cover the tissue the overview found @main", async ({ page, server }) => {
  test.setTimeout(180_000);
  await overviewed(server.url);
  await atRegions(page, server.url);
  const view = (await call((signal) =>
    new Api(server.url).api.GET("/api/v1/scan/last_overview", { signal }),
  ))!;
  expect(view.kept).toBeGreaterThan(0);
  const count = await said(page, "regions")
    .getByRole("group", { name: /^Region \d+$/ })
    .count();
  const regions = [];
  for (let n = 1; n <= count; n++) regions.push(await regionOf(page, n));
  // Every pixel of tissue, its middle, in a region proposed; a thousandth of a mm, as the fields round.
  const p = view.pixel_um;
  for (const [row = 0, from = 0, to = 0] of view.tissue) {
    for (let column = from; column < to; column++) {
      const x = view.origin.x + (column + 0.5) * p;
      const y = view.origin.y + (row + 0.5) * p;
      const inside = regions.some(
        (r) => x >= r.x - 1 && x <= r.x + r.width + 1 && y >= r.y - 1 && y <= r.y + r.height + 1,
      );
      expect(inside, `${x}, ${y}`).toBe(true);
    }
  }
});

test("an edited region is what is scanned, its focus map measured and shown", async ({
  page,
  server,
}) => {
  test.setTimeout(240_000);
  await overviewed(server.url);
  // The camera's tiny region for the scan, set while it doesn't stream.
  const on = new Api(server.url);
  await act("/api/v1/camera/stop_streaming", {}, { on });
  await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: TINY, signal }));
  await act("/api/v1/camera/start_streaming", {}, { on });
  await act("/api/v1/projects/create", { name: "Histo" }, { on });
  await atRegions(page, server.url);
  // The first kept and narrowed; the others dropped.
  const count = await said(page, "regions")
    .getByRole("group", { name: /^Region \d+$/ })
    .count();
  for (let n = 2; n <= count; n++) {
    await said(page, `region-${n}`).getByRole("checkbox", { name: "Keep it" }).click();
  }
  await typeRegion(page, "region-1", EDITED);
  await said(page, "histo").getByRole("button", { name: "Next" }).click();
  const focus = said(page, "focus");
  for (const [label, value] of [
    ["The map's columns", "2"],
    ["Its rows", "2"],
  ] as const) {
    await focus.getByLabel(label, { exact: true }).fill(value);
    await focus.getByLabel(label, { exact: true }).press("Tab");
  }
  await said(page, "histo").getByRole("button", { name: "Next" }).click();
  await choose(page, "Project", "Histo");
  await said(page, "run").getByRole("button", { name: "Run the scan" }).click();
  await expect(said(page, "scan-tiles")).toHaveText(/^Completed: (\d+) of \1 tiles$/, {
    timeout: 200_000,
  });
  // The map the run measured, before its first tile: shown.
  await expect(said(page, "map-said")).toContainText(/A plane through \d+ points/);
  // Each tile the scan took, in the region as edited.
  const progress = (await call((signal) => on.api.GET("/api/v1/scan/progress", { signal })))!;
  const [x, y, width, height] = EDITED.map((mm) => mm * 1000) as [number, number, number, number];
  const half = progress.plan.field.cell_um;
  for (const tile of progress.plan.tiles) {
    expect(tile.x_um).toBeGreaterThanOrEqual(x - half.width);
    expect(tile.x_um).toBeLessThanOrEqual(x + width + half.width);
    expect(tile.y_um).toBeGreaterThanOrEqual(y - half.height);
    expect(tile.y_um).toBeLessThanOrEqual(y + height + half.height);
  }
  expect(progress.done).toHaveLength(progress.plan.tiles.length);
});

test("the histo scan's regions have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  test.setTimeout(180_000);
  await overviewed(server.url);
  await atRegions(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the regions proposed look as their baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(180_000);
  await overviewed(server.url);
  await atRegions(page, server.url);
  await expect(said(page, "shapes").locator("polygon.region").first()).toBeVisible();
  // The plan of the regions kept, come: its frames drawn.
  await expect(said(page, "shapes").locator("polygon.frame").first()).toBeVisible();
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`scan-histo-${scheme}.png`, {
      stylePath: HIDE_TILES,
      mask: [said(page, "field"), said(page, "at")],
    });
    await shot(page, `W5.3-histo-${scheme}`);
  }
});
