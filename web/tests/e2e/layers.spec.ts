// The layers over the live image (W3.5): an ROI drawn, the region the server
// measures, moved from the keyboard and made the camera's region; a line's
// profile, its ends the line's; the layers switched and kept across a
// reload; axe; and each layer's baseline on a frozen frame, the frame
// itself hidden.
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { Server as Api, call } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { overTissue } from "./sim.ts";

const HIDDEN = fileURLToPath(new URL("./hide-frame.css", import.meta.url));
const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });
const tools = (page: Page) => page.getByRole("toolbar", { name: "What is drawn over the image" });
const tool = (page: Page, name: string) => tools(page).getByRole("button", { name, exact: true });
const readout = (page: Page, name: string) => said(page, name).locator("dd");

/** Control, the camera streaming over the tissue, its frames drawn. */
async function live(page: Page, url: string): Promise<void> {
  await overTissue(url);
  await page.goto(`${url}/#/control`);
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
}

/** A drag across the image, between two of its places, as fractions of its box. */
async function across(page: Page, from: [number, number], to: [number, number]): Promise<void> {
  const box = (await image(page).boundingBox())!;
  const at = (f: [number, number]) =>
    [box.x + box.width * f[0], box.y + box.height * f[1]] as const;
  await page.mouse.move(...at(from));
  await page.mouse.down();
  await page.mouse.move(...at(to), { steps: 6 });
  await page.mouse.up();
}

type Rect = { x: number; y: number; width: number; height: number };

/** A region read from what is said of it: `x 1, y 2, 3 × 4`. */
function rectIn(text: string): Rect | undefined {
  const found = /x (\d+), y (\d+), (\d+) × (\d+)/.exec(text);
  if (!found) return undefined;
  const [x, y, width, height] = found.slice(1).map(Number) as [number, number, number, number];
  return { x, y, width, height };
}

/** The ROI drawn, as its rectangle says it. */
async function drawnRoi(page: Page): Promise<Rect | undefined> {
  return rectIn((await said(page, "roi").getAttribute("aria-label")) ?? "");
}

/**
 * Whether the server measured `drawn`: the sensor's whole 2 × 2 cells, so
 * each edge rounded out by a pixel at most, as W3.1's regions are.
 */
async function measures(page: Page, drawn: Rect): Promise<boolean> {
  const measured = rectIn(await said(page, "roi-measured").innerText());
  if (!measured) return false;
  const edges = [
    drawn.x - measured.x,
    drawn.y - measured.y,
    measured.x + measured.width - (drawn.x + drawn.width),
    measured.y + measured.height - (drawn.y + drawn.height),
  ];
  return edges.every((edge) => edge >= 0 && edge <= 1);
}

test("an ROI drawn is the region the server measures, moved by its keys @main", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  await live(page, server.url);
  await tool(page, "Draw an ROI").click();
  await across(page, [0.4, 0.4], [0.6, 0.7]);
  await expect(tool(page, "Draw an ROI")).toHaveAttribute("aria-pressed", "false");
  const drawn = (await drawnRoi(page))!;
  expect(drawn).toBeDefined();
  await expect.poll(() => measures(page, drawn), { timeout: 30_000 }).toBe(true);
  await expect(said(page, "roi-stats").getByRole("row")).not.toHaveCount(0);
  // Ten pixels right, from the keyboard: the server measures the region moved.
  await said(page, "roi").focus();
  await page.keyboard.press("Shift+ArrowRight");
  const x = drawn.x;
  const shifted = { ...drawn, x: x + 10 };
  expect(await drawnRoi(page)).toEqual(shifted);
  await expect.poll(() => measures(page, shifted), { timeout: 30_000 }).toBe(true);
  // A small ROI, under the camera's least side of 48 pixels, made the camera's region:
  // enlarged to it, on the camera's grid, about the ROI.
  await tool(page, "Draw an ROI").click();
  await across(page, [0.5, 0.5], [0.52, 0.53]);
  const small = (await drawnRoi(page))!;
  expect(Math.max(small.width, small.height)).toBeLessThan(48);
  await said(page, "roi-stats").getByRole("button", { name: "Read out only this region" }).click();
  const on = new Api(server.url);
  await expect
    .poll(async () => {
      const roi = await call((signal) => on.api.GET("/api/v1/camera/roi", { signal }));
      if (!roi) return false;
      const holds = roi.x <= small.x && roi.x + roi.width >= small.x + small.width;
      // Not the region it had, which held it too: one of the least side, or a grid's step more.
      const least = [roi.width, roi.height].every((side) => side >= 48 && side <= 60);
      return holds && roi.x % 12 === 0 && least;
    })
    .toBe(true);
});

test("a line's profile has the line's ends", async ({ page, server }) => {
  test.setTimeout(120_000);
  await live(page, server.url);
  await tool(page, "Draw a line").click();
  await across(page, [0.2, 0.5], [0.8, 0.5]);
  const line = said(page, "line");
  const ends = await line.evaluate((drawn) =>
    ["x1", "y1", "x2", "y2"].map((name) => Math.round(Number(drawn.getAttribute(name)))),
  );
  await expect(said(page, "line-measured")).toHaveText(
    `Measured from x ${ends[0]}, y ${ends[1]} to x ${ends[2]}, y ${ends[3]}`,
    { timeout: 30_000 },
  );
  await expect(said(page, "profile")).toContainText("The line's profile");
});

test("the layers are switched, and kept", async ({ page, server }) => {
  test.setTimeout(120_000);
  await live(page, server.url);
  await expect(said(page, "scale")).toBeVisible();
  await tool(page, "Crosshair").click();
  await tool(page, "Grid").click();
  await tool(page, "Scale bar").click();
  await expect(page.locator('[data-layer="crosshair"]')).toBeVisible();
  await expect(page.locator('[data-layer="grid"] line')).not.toHaveCount(0);
  await expect(said(page, "scale")).toHaveCount(0);
  // Kept: the page loaded again, still so.
  await page.reload();
  await expect(tool(page, "Crosshair")).toHaveAttribute("aria-pressed", "true");
  await expect(tool(page, "Grid")).toHaveAttribute("aria-pressed", "true");
  await expect(tool(page, "Scale bar")).toHaveAttribute("aria-pressed", "false");
});

test("the layers have no violation axe can find, in either theme", async ({ page, server }) => {
  test.setTimeout(120_000);
  await live(page, server.url);
  await tool(page, "Draw an ROI").click();
  await across(page, [0.3, 0.3], [0.5, 0.5]);
  await tool(page, "Draw a line").click();
  await across(page, [0.2, 0.7], [0.7, 0.8]);
  await expect(said(page, "line")).toBeVisible();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("each layer looks as its baseline does, on a frozen frame @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(120_000);
  await live(page, server.url);
  await tool(page, "Crosshair").click();
  await tool(page, "Grid").click();
  await tool(page, "Draw an ROI").click();
  await across(page, [0.25, 0.25], [0.45, 0.5]);
  // Each shape's numbers come before the next is drawn: the panel they fill takes room from the
  // image, which would move under a drag.
  await expect(said(page, "roi-stats").getByRole("row")).toHaveCount(4, { timeout: 30_000 });
  await tool(page, "Draw a line").click();
  await across(page, [0.55, 0.3], [0.85, 0.75]);
  await expect(said(page, "line-measured")).toBeVisible({ timeout: 30_000 });
  // Frozen: no frame comes, so the view and the shapes stay where they are.
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Not streaming");
  await page.mouse.move(5, 5);
  const stage = image(page).locator("..");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(stage).toHaveScreenshot(`layers-${scheme}.png`, { stylePath: HIDDEN });
    // The notes' picture, the frame shown.
    await shot(page, `W3.5-layers-${scheme}`);
  }
});
