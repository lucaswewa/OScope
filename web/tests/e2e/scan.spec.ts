// Covers: web/src/features/scan
// Scan plans (W5.1): a region drawn on the view gives the server's plan of
// it; each built-in pattern's form submits, the tissue's refused without an
// overview; an overview is made and shown under the plan; axe; and a plan's
// baseline. Planning moves nothing: the stage only homes.
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { homed, overTissueTiny } from "./sim.ts";

const HIDE_TILES = fileURLToPath(new URL("hide-tiles.css", import.meta.url));
const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const plan = (page: Page) => said(page, "plan");
const estimate = (page: Page, term: string) =>
  said(page, "estimate").locator("dt", { hasText: term }).locator("xpath=following-sibling::dd[1]");

/** Types a region, in mm, into a pane's fields. */
async function typeRegion(
  page: Page,
  pane: string,
  [x, y, width, height]: number[],
): Promise<void> {
  const fields = said(page, pane);
  for (const [label, value] of [
    ["x", x],
    ["y", y],
    ["Width", width],
    ["Height", height],
  ] as const) {
    const field = fields.getByLabel(label, { exact: true });
    await field.fill(String(value));
    await field.press("Tab");
  }
}

/** The server's plan of a snake over `region`, in µm. */
async function serverPlan(
  url: string,
  region: { x: number; y: number; width: number; height: number },
) {
  const rect = {
    origin: { x: region.x, y: region.y },
    size: { width: region.width, height: region.height },
  };
  return act(
    "/api/v1/scan/plan",
    { request: { pattern: "snake", region: { rect }, positions: [] } },
    { on: new Api(url) },
  );
}

test("a region drawn on the view gives the server's plan of it @main", async ({ page, server }) => {
  test.setTimeout(120_000);
  await homed(server.url);
  await page.goto(`${server.url}/#/scan`);
  await expect(said(page, "view-said")).toContainText("No overview yet");
  await plan(page).getByRole("button", { name: "Draw it" }).click();
  await expect(said(page, "view-said")).toContainText("Drag on the image");
  // A drag across the middle of the view, over the stage's travel.
  const box = (await said(page, "tiles").boundingBox())!;
  const [x0, y0] = [box.x + box.width * 0.45, box.y + box.height * 0.45];
  await page.mouse.move(x0, y0);
  await page.mouse.down();
  await page.mouse.move(x0 + 40, y0 + 25, { steps: 5 });
  await page.mouse.up();
  await expect(estimate(page, "Tiles")).toHaveText(/^\d+$/, { timeout: 30_000 });
  // The region the drag drew, as the fields say it; the server's plan of it.
  const fields = said(page, "plan-region");
  const mm = async (label: string) =>
    Number(await fields.getByLabel(label, { exact: true }).inputValue());
  const region = {
    x: (await mm("x")) * 1000,
    y: (await mm("y")) * 1000,
    width: (await mm("Width")) * 1000,
    height: (await mm("Height")) * 1000,
  };
  expect(region.width).toBeGreaterThan(0);
  const planned = await serverPlan(server.url, region);
  await expect(estimate(page, "Tiles")).toHaveText(String(planned.estimate.tiles));
  await expect(estimate(page, "Grid")).toHaveText(
    `${planned.plan.rows} rows × ${planned.plan.columns} columns`,
  );
  // Each tile's frame drawn, then the path and the region.
  await expect(said(page, "shapes").locator("polygon")).toHaveCount(planned.plan.tiles.length + 1);
  // The overlap changed: the plan follows.
  const overlap = plan(page).getByLabel("Overlap", { exact: true });
  await overlap.fill("40");
  await overlap.press("Tab");
  await expect(estimate(page, "Overlap")).toHaveText("40 %");
});

test("each built-in pattern's form submits, and the tissue's is refused without an overview", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  await homed(server.url);
  await page.goto(`${server.url}/#/scan`);
  const pattern = plan(page).getByLabel("Pattern", { exact: true });
  const choose = async (name: string) => {
    await pattern.click();
    await page.getByRole("option", { name, exact: true }).click();
  };
  for (const name of ["Raster", "Snake", "Tiled z-stack"]) {
    await choose(name);
    await typeRegion(page, "plan-region", [40, 12, 3, 2]);
    await expect(estimate(page, "Tiles"), name).toHaveText(/^[1-9]\d*$/, { timeout: 30_000 });
  }
  await choose("Z-scan");
  await plan(page).getByRole("button", { name: "Here", exact: true }).click();
  // Ten below and ten above, two apart: one tile, eleven planes.
  await expect(estimate(page, "Tiles")).toHaveText("1", { timeout: 30_000 });
  await choose("Positions");
  await plan(page).getByRole("button", { name: "Add where the stage is" }).click();
  await plan(page).getByRole("button", { name: "Add where the stage is" }).click();
  await expect(estimate(page, "Tiles")).toHaveText("2", { timeout: 30_000 });
  await choose("The tissue found");
  await expect(said(page, "plan-said")).toContainText("no overview yet", { timeout: 30_000 });
});

test("an overview is made, and shown under the plan @main", async ({ page, server }) => {
  test.setTimeout(150_000);
  // The camera's tiny region: an overview of 0.3 × 0.2 mm over the tissue takes 20 frames.
  await overTissueTiny(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on: new Api(server.url) });
  await page.goto(`${server.url}/#/scan`);
  await typeRegion(page, "overview-region", [39.85, 11.9, 0.3, 0.2]);
  await expect(said(page, "overview-frames")).toContainText("It takes 20 frames");
  await said(page, "overview").getByRole("button", { name: "Make the overview" }).click();
  await expect(said(page, "overview-said")).toContainText("20 frames, 50 µm a pixel", {
    timeout: 120_000,
  });
  await expect(said(page, "view-said")).not.toContainText("No overview yet");
  const view = await call((signal) =>
    new Api(server.url).api.GET("/api/v1/scan/last_overview", { signal }),
  );
  expect(view?.frames).toBe(20);
});

test("the scan tab has no violation axe can find, in either theme", async ({ page, server }) => {
  test.setTimeout(120_000);
  await homed(server.url);
  await page.goto(`${server.url}/#/scan`);
  await typeRegion(page, "plan-region", [40, 12, 3, 2]);
  await expect(estimate(page, "Tiles")).toHaveText(/^\d+$/, { timeout: 30_000 });
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("a plan over the stage looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(120_000);
  await homed(server.url);
  await page.goto(`${server.url}/#/scan`);
  await typeRegion(page, "plan-region", [40, 12, 3, 2]);
  await expect(estimate(page, "Tiles")).toHaveText(/^\d+$/, { timeout: 30_000 });
  // The view fitted to the region, and still; no field focused, the pane at its top.
  await expect(said(page, "shapes").locator("polygon").first()).toBeVisible();
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page
    .getByRole("region", { name: "The overview and the plan" })
    .evaluate((pane) => pane.scrollTo(0, 0));
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The camera's field of view follows the stage: masked.
    await expect(page).toHaveScreenshot(`scan-${scheme}.png`, {
      stylePath: HIDE_TILES,
      mask: [said(page, "field"), said(page, "at")],
    });
    await shot(page, `W5.1-scan-${scheme}`);
  }
});
