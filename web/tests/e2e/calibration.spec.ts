// Covers: web/src/features/settings web/src/features/live
// The calibration (W8.2): the flat field run from the Settings tab over
// clear glass, its map showing the simulator's vignetting, and the live
// image evened out by the look's switch; the geometry run over the tissue,
// its residuals charted; that set exported, edited, imported and chosen,
// and the live view's scale bar following it; axe; and the view's baseline.
import { readFileSync, writeFileSync } from "node:fs";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { greys, mean } from "./pixels.ts";
import { aSmallRegion, fast, homed, overTissue, SMALL } from "./sim.ts";

// The optics' falloff, strong enough to see over the small region about the sensor's middle.
test.use({ simulator: "vignetting = 8.0" });

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });
const tools = (page: Page) => page.getByRole("toolbar", { name: "What is drawn over the image" });
const readout = (page: Page, name: string) => said(page, name).locator("dd");

/** Settings' calibration view. */
async function calibration(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/settings`);
  await page.getByRole("tab", { name: "Calibration", exact: true }).click();
}

/** Runs the routine `name` from the view, and waits for its set to be in use. */
async function run(page: Page, name: string): Promise<void> {
  const routines = said(page, "routines");
  await expect(routines.getByRole("button", { name, exact: true })).toBeEnabled({
    timeout: 30_000,
  });
  await routines.getByRole("button", { name, exact: true }).click();
  await expect(said(page, "routine-said")).toHaveText(`${name}: measured, and in use.`, {
    timeout: 120_000,
  });
}

/** The µm at the sample a CSS pixel of the live view spans, from its scale bar. */
async function perPixel(page: Page): Promise<number> {
  const bar = said(page, "scale");
  const [length, unit] = (await bar.innerText()).trim().split(/\s+/);
  const um = parseFloat(length!) * (unit === "mm" ? 1000 : 1);
  const line = bar.locator(".line");
  return um / (await line.evaluate((one) => parseFloat((one as HTMLElement).style.inlineSize)));
}

/** The stage over clear glass beside the coverslip, the small region streaming. */
async function overGlass(url: string): Promise<void> {
  const on = new Api(url);
  await homed(url);
  await fast(url, () =>
    act("/api/v1/stage/move_to", { x: 10_000, y: 12_500, z: 5_000, speed: 25_000 }, { on }),
  );
  await aSmallRegion(url);
  await act("/api/v1/camera/start_streaming", {}, { on });
}

/**
 * Control, its values panel open, the live frames drawn. The panel is opened
 * and the page loaded again, which keeps it open: Playwright's WebKit on
 * Windows draws a WebGL canvas black once it is resized, as opening the
 * panel resizes it (W3.6).
 */
async function withValues(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/control`);
  await tools(page).getByRole("button", { name: "Histogram", exact: true }).click();
  await expect(said(page, "values")).toBeVisible();
  await page.reload();
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  await expect(said(page, "corrections")).toBeVisible();
}

/**
 * The live image's middle over its edges, as drawn, the frame fitted in the canvas: about
 * 1.07 through the vignetting, about 1 without it.
 */
async function falloff(page: Page): Promise<number> {
  const box = (await image(page).boundingBox())!;
  const scale = Math.min(box.width / SMALL.width, box.height / SMALL.height);
  const [width, height] = [SMALL.width * scale, SMALL.height * scale];
  const png = await image(page).screenshot({ scale: "css" });
  const at = async (dx: number, dy: number) =>
    mean(await greys(page, png, [box.width / 2 + dx * width, box.height / 2 + dy * height], 12));
  const edges =
    (await at(-0.42, 0)) + (await at(0.42, 0)) + (await at(0, -0.4)) + (await at(0, 0.4));
  return (await at(0, 0)) / (edges / 4);
}

test("the flat field's map shows the vignetting, and the look evens the live image out @main", async ({
  page,
  server,
}) => {
  test.setTimeout(180_000);
  await overGlass(server.url);
  await calibration(page, server.url);
  await expect(said(page, "result-flat")).toContainText("None in use");
  await run(page, "Flat field");
  const result = said(page, "result-flat");
  await expect(result).toContainText(/For the 10x in transmitted: \d+\.\d % uneven\./);
  await expect(result.getByRole("img", { name: /^The flat field/ })).toBeVisible();
  // Brighter in the middle than at the corner: from the map's own pixels.
  const [middle, corner] = await result.locator("canvas").evaluate((canvas: HTMLCanvasElement) => {
    const { data } = canvas.getContext("2d")!.getImageData(0, 0, canvas.width, canvas.height);
    const grey = (x: number, y: number) => {
      const i = 4 * (y * canvas.width + x);
      return data[i]! + data[i + 1]! + data[i + 2]!;
    };
    return [grey(canvas.width >> 1, canvas.height >> 1), grey(0, 0)];
  });
  expect(middle).toBeGreaterThan(corner! + 150);
  await expect(said(page, "flat-range")).toContainText(/From 0\.\d+ to 1\.\d+ of its mean/);
  await expect(said(page, "sets-10x")).toContainText("flat for the 10x in transmitted");

  // The live image: vignetted, then even with the flat field divided out.
  await withValues(page, server.url);
  await expect.poll(() => falloff(page), { timeout: 30_000 }).toBeGreaterThan(1.05);
  const corrections = said(page, "corrections");
  await corrections.getByRole("switch", { name: "Divide by the flat field" }).click();
  await expect
    .poll(
      async () => (await (await page.request.get(`${server.url}/api/v1/preview/look`)).json()).flat,
    )
    .toBe(true);
  await expect.poll(() => falloff(page), { timeout: 30_000 }).toBeLessThan(1.015);
  await corrections.getByRole("switch", { name: "Divide by the flat field" }).click();
  await expect.poll(() => falloff(page), { timeout: 30_000 }).toBeGreaterThan(1.05);
});

test("a geometry's set is exported, imported and chosen, and the live view's scale follows it", async ({
  page,
  server,
}) => {
  test.setTimeout(240_000);
  const on = new Api(server.url);
  await overTissue(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  await calibration(page, server.url);
  await run(page, "Geometry");
  const result = said(page, "result-geometry");
  await expect(result).toContainText(/A pixel\s*0\.\d{4} µm/);
  await expect(result.getByRole("img", { name: /^Each move's residual/ })).toBeVisible();

  // Exported, made twice as coarse, and imported: the key's next version, not in use.
  const sets = said(page, "sets-10x");
  const downloading = page.waitForEvent("download");
  await sets.getByRole("button", { name: "Export", exact: true }).click();
  const download = await downloading;
  expect(download.suggestedFilename()).toBe("oscope-calibration-geometry-10x-1-v1.json");
  const exported = JSON.parse(readFileSync(await download.path(), "utf8"));
  const values = exported.values;
  values.pixel_um *= 2;
  values.matrix = values.matrix.map((row: number[]) => row.map((m) => m * 2));
  const edited = test.info().outputPath("coarser.json");
  writeFileSync(edited, JSON.stringify(exported));
  const choosing = page.waitForEvent("filechooser");
  await said(page, "sets").getByRole("button", { name: "Import a set…" }).click();
  await (await choosing).setFiles(edited);
  await expect(said(page, "sets-said")).toHaveText(
    "coarser.json imported as version 2: chosen, it is used.",
  );
  await expect(sets.getByRole("listitem")).toHaveCount(2);
  await expect(sets.getByRole("button", { name: "Use this one" })).toHaveCount(1);

  // The live view's scale bar before, and after the import is chosen.
  await page.goto(`${server.url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  await expect(said(page, "scale")).toContainText("µm");
  const before = await perPixel(page);
  await calibration(page, server.url);
  await sets.getByRole("button", { name: "Use this one" }).click();
  await expect(said(page, "sets-said")).toContainText("In use:");
  await expect(result).toContainText(`${(values.pixel_um as number).toFixed(4)} µm`);
  // Twice the µm a CSS pixel spans, to the bar's rounding of a pixel.
  await page.goto(`${server.url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  await expect.poll(async () => (await perPixel(page)) / before).toBeCloseTo(2, 1);
});

test("the calibration view has no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  await calibration(page, server.url);
  await expect(said(page, "routines")).toContainText("start streaming first");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the calibration view looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.setViewportSize({ width: 1280, height: 1000 });
  await calibration(page, server.url);
  await expect(said(page, "last-check")).toHaveText("Not checked yet.");
  await expect(said(page, "sets")).toContainText("No calibration yet.");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`calibration-${scheme}.png`);
  }
});

test("the calibration in use, for the notes' screenshots", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
  test.setTimeout(300_000);
  const on = new Api(server.url);
  const to = (x: number, y: number) =>
    fast(server.url, () => act("/api/v1/stage/move_to", { x, y, z: 5_000, speed: 25_000 }, { on }));
  await page.setViewportSize({ width: 1280, height: 1500 });
  await overGlass(server.url);
  await calibration(page, server.url);
  await run(page, "Flat field");
  await run(page, "Dark frame");
  await to(40_000, 12_000);
  await run(page, "Geometry");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W8.2-calibration-${scheme}`);
  }
  // Over the glass again, the live image with the flat field divided out.
  await to(10_000, 12_500);
  await page.setViewportSize({ width: 1280, height: 900 });
  await withValues(page, server.url);
  await said(page, "corrections").getByRole("switch", { name: "Divide by the flat field" }).click();
  await expect.poll(() => falloff(page), { timeout: 30_000 }).toBeLessThan(1.015);
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W8.2-flat-on-${scheme}`);
  }
});
