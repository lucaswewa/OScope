// Checkpoint J (W6.4's notes): a scanned slide, found in the Gallery, read,
// viewed at every zoom, its look changed, a record beside it, a region of it
// saved and the whole exported, and a TIFF imported beside it. The scan and
// its stitch are Checkpoint I's, made here by the API, through the camera's
// tiny region over the default slide's tissue.
import { readFile } from "node:fs/promises";

import type { Locator, Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { finished } from "../../src/api/jobs.ts";
import { choose, expect, test } from "../e2e/fixtures.ts";
import { overTissueTiny } from "../e2e/sim.ts";
import { tiffOf } from "../e2e/tiff.ts";
import { said } from "./steps.ts";

/** A slide scanned, a snake of a few tiles over the tissue, and stitched: its project's name. */
async function scannedAndStitched(url: string): Promise<string> {
  await overTissueTiny(url);
  const on = new Api(url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const name = "Checkpoint J slide";
  const project = (await act("/api/v1/projects/create", { name }, { on })).id;
  const region = { rect: { origin: { x: 39_900, y: 11_900 }, size: { width: 300, height: 200 } } };
  const request = { pattern: "snake", region, positions: [] };
  const scan = await act("/api/v1/scan/run", { request, project, name: "the scan" }, { on });
  const scanned = await finished(scan, { on, timeout: 300_000 });
  const acquisition = scanned.results
    .map((r) => /acquisitions\/([^/]+)$/.exec(r)?.[1])
    .find((id) => id !== undefined)!;
  const params = { project, acquisition };
  const stitch = await act("/api/v1/jobs/submit", { kind: "stitch.run", params }, { on });
  await finished(stitch, { on, timeout: 300_000 });
  return name;
}

/** Drops `bytes` as the file `name` on `target`, as a drag from the desktop would. */
async function dropOn(target: Locator, name: string, bytes: Buffer): Promise<void> {
  const files = await target.page().evaluateHandle(
    ({ name, bytes }) => {
      const transfer = new DataTransfer();
      transfer.items.add(new File([new Uint8Array(bytes)], name, { type: "image/tiff" }));
      return transfer;
    },
    { name, bytes: [...bytes] },
  );
  for (const event of ["dragenter", "dragover", "drop"])
    await target.dispatchEvent(event, { dataTransfer: files });
}

/** The look the page's tiles are asked for, as their addresses say it. */
function looksAsked(page: Page): string[] {
  const asked: string[] = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (/\/image_files\/\d+\/\d+_\d+\.jpg$/.test(url.pathname)) asked.push(url.search);
  });
  return asked;
}

test("Checkpoint J: a slide found, read, viewed at every zoom, exported, and a TIFF imported @main", async ({
  page,
  server,
}) => {
  test.setTimeout(600_000);
  const name = await scannedAndStitched(server.url);
  const looks = looksAsked(page);

  // Found by part of its name: its card, its picture the mosaic's.
  await page.goto(`${server.url}/#/gallery`);
  await page.getByRole("textbox", { name: "Find" }).fill("Checkpoint");
  const card = said(page, "card").filter({ hasText: name });
  await expect(card).toHaveCount(1);
  await expect(card.locator("img")).toHaveJSProperty("complete", true);
  await card.getByRole("link").first().click();

  // Its records: the acquisition's details, and the result's provenance.
  await said(page, "records-acquisition").getByRole("button").click();
  const details = said(page, "details");
  await expect(details).toContainText("Exposure");
  await expect(details).toContainText("The stage, at its first frame");
  await said(page, "records-result").getByRole("button").click();
  await expect(said(page, "provenance")).toContainText("the scan");
  await expect(said(page, "provenance")).toContainText("stitch.mosaic");

  // At every zoom: in to its pixels, sharp at each step; all of it again.
  await said(page, "view").click();
  const viewer = said(page, "viewers").locator('[data-said^="viewer-"]').first();
  const tiles = viewer.locator('[data-said="tiles"]');
  await expect(tiles).toHaveAttribute("data-sharp", "true", { timeout: 60_000 });
  const scale = viewer.locator('[data-said="tile-scale"]');
  const whole = await scale.innerText();
  for (let step = 0; step < 6; step++) {
    await viewer.getByRole("button", { name: "Zoom in" }).click();
    await expect(tiles).toHaveAttribute("data-sharp", "true", { timeout: 30_000 });
  }
  await expect(scale).not.toHaveText(whole);
  await viewer.getByRole("button", { name: "All of it" }).click();
  await expect(scale).toHaveText(whole);

  // Full screen, the views alone, then left.
  await page.getByRole("button", { name: "Full screen" }).click();
  await expect.poll(() => page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
  // The browser leaves full screen at Escape itself, which a test's keys don't reach.
  await page.evaluate(() => document.exitFullscreen());
  await expect(page.getByRole("button", { name: "Full screen" })).toBeVisible();

  // Its look: the white point at 60 % and gamma at 0.7, the tiles asked for again so.
  const look = said(page, "look");
  const white = look.getByRole("slider", { name: "White point" });
  await white.focus();
  for (let step = 0; step < 40; step++) await page.keyboard.press("ArrowLeft");
  const gamma = look.getByRole("slider", { name: "Gamma" });
  await gamma.focus();
  for (let step = 0; step < 6; step++) await page.keyboard.press("ArrowLeft");
  await expect
    .poll(() => looks.some((one) => one.includes("white=0.6") && one.includes("gamma=0.7")), {
      timeout: 30_000,
    })
    .toBe(true);

  // Beside it, the scan's acquisition: panned, the two views look at the same place.
  await choose(page, "Beside it", "the scan");
  const second = said(page, "viewers").locator('[data-said^="viewer-"]').nth(1);
  await expect(second.locator('[data-said="tiles"]')).toHaveAttribute("data-sharp", "true", {
    timeout: 60_000,
  });
  const box = (await tiles.boundingBox())!;
  const [x, y] = [box.x + box.width / 2, box.y + box.height / 2];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x - box.width / 5, y - box.height / 6, { steps: 4 });
  await page.mouse.up();
  const middleOf = async (view: Locator) => {
    const at = (await view.locator('[data-said="tiles"]').boundingBox())!;
    const [mx, my] = [at.x + at.width / 2, at.y + at.height / 2];
    await page.mouse.move(mx + 1, my);
    await page.mouse.move(mx, my);
    return view.locator('[data-said="at"]').innerText();
  };
  await expect
    .poll(async () => (await middleOf(viewer)) === (await middleOf(second)), { timeout: 15_000 })
    .toBe(true);
  // Its planes, its tiles: the last one asked for.
  const plane = look.getByRole("slider", { name: "Plane" });
  await plane.focus();
  await page.keyboard.press("End");
  await expect.poll(() => looks.some((one) => /plane=[1-9]/.test(one))).toBe(true);

  // The view saved, as a PNG; the whole mosaic exported as OME-TIFF and downloaded.
  const saving = page.waitForEvent("download");
  await said(page, "viewer").getByRole("button", { name: "Save the view" }).click();
  const view = await readFile(await (await saving).path());
  expect(view.subarray(1, 4).toString("latin1")).toBe("PNG");
  await said(page, "viewer").getByRole("link", { name: "Its project" }).click();
  await choose(page, "Format", "OME-TIFF");
  await said(page, "export").getByRole("button", { name: "Export", exact: true }).click();
  const exported = said(page, "exported").getByRole("listitem");
  await expect(exported).toBeVisible({ timeout: 120_000 });
  const file = (await exported.locator(".name").textContent())!;
  expect(file).toMatch(/\.ome\.tiff?$/);
  const downloading = page.waitForEvent("download");
  await exported.getByRole("button", { name: `Download ${file}` }).click();
  const tiff = await readFile(await (await downloading).path());
  expect(tiff.subarray(0, 2).toString("latin1")).toBe("II");

  // A TIFF dropped on the project's page: uploaded, imported, listed, and viewed.
  await dropOn(said(page, "project"), "beside.tif", tiffOf(2048, 1536));
  await expect(said(page, "import-beside.tif")).toContainText("imported", { timeout: 120_000 });
  await expect(said(page, "records-acquisition").getByRole("listitem")).toHaveCount(2);
  await said(page, "import-beside.tif")
    .getByRole("button", { name: "Open what beside.tif made" })
    .click();
  await said(page, "view").click();
  await expect(
    said(page, "viewers").locator('[data-said^="viewer-"] [data-said="tiles"]').first(),
  ).toHaveAttribute("data-sharp", "true", { timeout: 60_000 });
});
