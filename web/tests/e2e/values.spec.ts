// The frame's values (W3.6): the saturated pixels marked where the frame is
// at the sensor's limit; a look's change reaching the preview, and not the
// frame's values; false colour chosen and kept; axe; and the values
// panel's baseline, the histogram masked, the colour map's bar shown.
import type { Page } from "@playwright/test";

import { Server as Api, call } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { mean } from "./pixels.ts";
import { overTissue } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });
const tools = (page: Page) => page.getByRole("toolbar", { name: "What is drawn over the image" });
const tool = (page: Page, name: string) => tools(page).getByRole("button", { name, exact: true });
const readout = (page: Page, name: string) => said(page, name).locator("dd");

/** Control, the camera at `exposure` ms over the tissue, streaming, its frames drawn. */
async function live(page: Page, url: string, exposure: number): Promise<void> {
  await overTissue(url);
  const on = new Api(url);
  await call((signal) => on.api.PUT("/api/v1/camera/exposure", { body: exposure, signal }));
  await page.goto(`${url}/#/control`);
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
}

/** The image's pixels in the page, each [r, g, b], from a screenshot. */
async function pixels(page: Page): Promise<number[][]> {
  const png = await image(page).screenshot();
  return page.evaluate(async (base64) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
    const canvas = document.createElement("canvas");
    [canvas.width, canvas.height] = [bitmap.width, bitmap.height];
    const drawn = canvas.getContext("2d")!;
    drawn.drawImage(bitmap, 0, 0);
    const { data } = drawn.getImageData(0, 0, bitmap.width, bitmap.height);
    const out: number[][] = [];
    for (let i = 0; i < data.length; i += 4 * 7) out.push([data[i]!, data[i + 1]!, data[i + 2]!]);
    return out;
  }, png.toString("base64"));
}
const isRed = ([r, g, b]: number[]) => r! > 240 && g! < 20 && b! < 20;
const luminance = ([r, g, b]: number[]) => 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;

test("the saturated pixels are marked where the frame is at the sensor's limit @main", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  // Long enough for about half the frame, the brightest of it, to reach the sensor's limit.
  await live(page, server.url, 30);
  await tool(page, "Saturated").click();
  await expect
    .poll(async () => (await pixels(page)).filter(isRed).length, { timeout: 30_000 })
    .toBeGreaterThan(0);
  const marked = await pixels(page);
  // Off, as the frames come: the same place, unmarked. Where the marks were, it is near white.
  await tool(page, "Saturated").click();
  await expect.poll(async () => (await pixels(page)).filter(isRed).length).toBe(0);
  const plain = await pixels(page);
  const under = plain.filter((_, i) => isRed(marked[i]!));
  const away = plain.filter((_, i) => !isRed(marked[i]!));
  expect(under.length).toBeGreaterThan(0);
  expect(under.filter((pixel) => luminance(pixel) > 200).length / under.length).toBeGreaterThan(
    0.95,
  );
  // And the marks are not everywhere: what they left out is darker.
  expect(mean(away.map(luminance))).toBeLessThan(mean(under.map(luminance)));
});

test("a look's change reaches the preview, and not the frame's values", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  await live(page, server.url, 10);
  await tool(page, "Histogram").click();
  await expect(said(page, "percentiles").first()).toContainText("99 %");
  const brightness = async () => mean((await pixels(page)).map(luminance));
  const before = await brightness();
  const percentiles = await said(page, "percentiles").first().innerText();
  // The white point at a quarter of the range: the image brightens.
  const white = said(page, "values").getByRole("spinbutton", { name: "White point" });
  await white.fill("1024");
  await white.press("Enter");
  await expect.poll(brightness, { timeout: 30_000 }).toBeGreaterThan(before * 1.3);
  // The frame's own values are as they were, to the noise.
  const [, ninetyNine] = /99 % (\d+)/.exec(percentiles)!;
  const now = /99 % (\d+)/.exec(await said(page, "percentiles").first().innerText())![1];
  expect(Math.abs(Number(now) - Number(ninetyNine))).toBeLessThan(Number(ninetyNine) * 0.1);
  // Put back.
  await said(page, "values").getByRole("button", { name: "Put the look back" }).click();
  await expect.poll(brightness, { timeout: 30_000 }).toBeLessThan(before * 1.15);
});

test("false colour is chosen, and kept @main", async ({ page, server, browserName }) => {
  // Playwright's WebKit on Windows draws a WebGL canvas black once it is resized, as the
  // values panel resizes it; a plain window's resize does it too. WebKit on Linux, the
  // desktop app's engine there, passes; on Windows the app is Chromium's.
  test.skip(
    browserName === "webkit" && process.platform === "win32",
    "WebKit on Windows draws a resized WebGL canvas black",
  );
  test.setTimeout(150_000);
  await live(page, server.url, 10);
  await tool(page, "Histogram").click();
  const greyish = async () =>
    mean((await pixels(page)).map(([r, g, b]) => Math.max(r!, g!, b!) - Math.min(r!, g!, b!)));
  const before = await greyish();
  await choose(page, "Colour map", "Grey");
  await expect.poll(greyish, { timeout: 30_000 }).toBeLessThan(2);
  await choose(page, "Colour map", "Turbo");
  await expect.poll(greyish, { timeout: 30_000 }).toBeGreaterThan(before);
  await expect(said(page, "map-bar")).toBeVisible();
  await page.reload();
  await expect(page.getByRole("combobox", { name: "Colour map" })).toHaveText("Turbo");
});

test("the values panel has no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  await live(page, server.url, 10);
  await tool(page, "Histogram").click();
  await choose(page, "Colour map", "Viridis");
  await expect(said(page, "percentiles").first()).toContainText("99 %");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the values panel looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(150_000);
  await page.setViewportSize({ width: 1280, height: 1100 });
  await live(page, server.url, 10);
  await tool(page, "Histogram").click();
  await choose(page, "Colour map", "Magma");
  await expect(said(page, "percentiles").first()).toContainText("99 %");
  // The focus trace drawn first: it shows from the second statistics on.
  await expect(said(page, "values").getByText("Focus", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Stop", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Not streaming");
  await page.mouse.move(5, 5);
  const panel = said(page, "values");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The frame's values are the simulator's noise: the charts and their numbers masked, the
    // colour map's bar shown.
    await expect(panel).toHaveScreenshot(`values-${scheme}.png`, {
      mask: [
        panel.locator("canvas:not([data-said='map-bar'])"),
        said(page, "percentiles"),
        said(page, "focus"),
        panel.getByRole("spinbutton"),
      ],
    });
    // The notes' picture: the page, the frozen frame in the map chosen.
    await shot(page, `W3.6-values-${scheme}`);
  }
});
