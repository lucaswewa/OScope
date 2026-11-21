// Covers: web/src/features/control, web/src/features/live
// The free-roam mosaic (W3.8): jogging adds tiles where the stage went, and
// none where it didn't; a double click on the mosaic sends the stage there;
// axe; and the tile viewer's chrome against its baseline. On the camera's
// tiny region, as W3.7's tests: a debug server draws each frame it keeps.
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { mosaicTile } from "../../src/api/media.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { overTissueTiny, TISSUE } from "./sim.ts";

const HIDE_TILES = fileURLToPath(new URL("hide-tiles.css", import.meta.url));
const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const readout = (page: Page, name: string) => said(page, name).locator("dd");

/** Control, the mosaic beside the live view, the camera streaming and the mosaic drawing. */
async function drawing(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/control`);
  await page.getByRole("button", { name: "Mosaic", exact: true }).click();
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  await page.getByRole("button", { name: "Start the mosaic" }).click();
  await expect(said(page, "mosaic-said")).toContainText("Drawing", { timeout: 30_000 });
}

/** The mosaic's state, as the server has it. */
async function mosaic(url: string) {
  const on = new Api(url);
  return (await call((signal) => on.api.GET("/api/v1/live_mosaic/state", { signal })))!;
}

/** The stage's x and y, as the server has it. */
async function stageAt(url: string): Promise<{ x: number; y: number; z: number }> {
  const on = new Api(url);
  return (await call((signal) => on.api.GET("/api/v1/stage/position", { signal }))).position;
}

/** The finest level's tile under a place of the stage. */
async function tileUnder(url: string, x: number, y: number): Promise<Blob | undefined> {
  const now = await mosaic(url);
  const canvas = now.canvas!;
  const col = Math.floor((x - canvas.x_um) / canvas.pixel_um / now.tile);
  const row = Math.floor((y - canvas.y_um) / canvas.pixel_um / now.tile);
  return mosaicTile(0, col, row, { on: new Api(url) });
}

test("jogging adds tiles where the stage went, and none where it didn't @main", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  await overTissueTiny(server.url);
  // A tile with nothing drawn is shown as nothing: none fails, as OpenSeadragon would log.
  const failed: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" && message.text().includes("failed to load")) {
      failed.push(message.text());
    }
  });
  // The tiles the viewer fetched that had something drawn, after the jogs as before.
  let jogged = false;
  const drawnTiles = { before: 0, after: 0 };
  page.on("response", (answer) => {
    if (answer.url().includes("/api/v1/mosaic/tiles/") && answer.status() === 200) {
      drawnTiles[jogged ? "after" : "before"] += 1;
    }
  });
  await drawing(page, server.url);
  await expect.poll(async () => (await mosaic(server.url)).counts.drawn).toBeGreaterThan(0);
  // Three jogs of 200 µm, each past the field of view, and a frame drawn after each.
  const on = new Api(server.url);
  const { z } = await stageAt(server.url);
  for (const step of [1, 2, 3]) {
    const drawn = (await mosaic(server.url)).counts.drawn;
    // Counted from the last jog on, before the frame it draws: the viewer's refresh can follow
    // the frame at once, and nothing changes once the stage is still.
    if (step === 3) jogged = true;
    await act("/api/v1/stage/move_to", { x: TISSUE.x + 200 * step, y: TISSUE.y, z }, { on });
    await expect
      .poll(async () => (await mosaic(server.url)).counts.drawn, { timeout: 30_000 })
      .toBeGreaterThan(drawn);
  }
  // Each place the stage went has its tile; where it never went has none.
  for (const step of [0, 1, 2, 3]) {
    expect(await tileUnder(server.url, TISSUE.x + 200 * step, TISSUE.y)).toBeDefined();
  }
  expect(await tileUnder(server.url, TISSUE.x + 20_000, TISSUE.y + 20_000)).toBeUndefined();
  // The viewer fetches the tiles again as the mosaic grows, and marks the field of view: an
  // eighth of the view across, as it opens on eight fields about it.
  await expect.poll(() => drawnTiles.after, { timeout: 30_000 }).toBeGreaterThan(0);
  const view = (await said(page, "tiles").boundingBox())!;
  const share = async () => ((await said(page, "field").boundingBox())?.width ?? 0) / view.width;
  await expect.poll(share).toBeGreaterThan(0.03);
  expect(await share()).toBeLessThan(0.2);
  await expect(said(page, "tile-scale")).toContainText("µm");
  expect(failed).toEqual([]);
});

test("a double click on the mosaic sends the stage there", async ({ page, server }) => {
  test.setTimeout(150_000);
  await overTissueTiny(server.url);
  await drawing(page, server.url);
  // A point of the viewer, away from its middle: where the pointer is, the read-out says.
  const viewer = said(page, "tiles");
  const box = (await viewer.boundingBox())!;
  const point = { x: box.x + box.width * 0.3, y: box.y + box.height * 0.7 };
  await page.mouse.move(point.x, point.y);
  const words = await said(page, "at").innerText();
  const [x, y] = [...words.matchAll(/-?[\d,]+\.\d/g)].map((m) => Number(m[0].replaceAll(",", "")));
  await page.mouse.dblclick(point.x, point.y);
  await expect
    .poll(
      async () => {
        const now = await stageAt(server.url);
        return Math.hypot(now.x - x!, now.y - y!);
      },
      { timeout: 30_000 },
    )
    // The stage's backlash, 2 µm, and the read-out's tenth, as W3.3's moves allow.
    .toBeLessThanOrEqual(3);
});

test("the mosaic has no violation axe can find, in either theme", async ({ page, server }) => {
  test.setTimeout(150_000);
  await overTissueTiny(server.url);
  await drawing(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the tile viewer's chrome looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(150_000);
  await page.setViewportSize({ width: 1280, height: 900 });
  await overTissueTiny(server.url);
  await drawing(page, server.url);
  await expect.poll(async () => (await mosaic(server.url)).counts.drawn).toBeGreaterThan(0);
  await page.getByRole("button", { name: "Stop drawing" }).click();
  await expect(said(page, "mosaic-said")).toContainText("Stopped");
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Not streaming");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The tiles, hidden, and the counts, masked, are the simulator's. The field marked, the
    // navigator's frame and the scale bar are the viewer's own.
    await expect(said(page, "mosaic")).toHaveScreenshot(`mosaic-${scheme}.png`, {
      stylePath: HIDE_TILES,
      mask: [said(page, "mosaic-said")],
    });
    await shot(page, `W3.8-mosaic-${scheme}`);
  }
});
