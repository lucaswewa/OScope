// Checkpoint G (W3.8's notes): a live microscope in the app. Home the stage,
// move over the tissue, stream, jog, zoom to full resolution, watch the
// histogram and the saturation as the exposure changes, autofocus, capture
// a frame into a project made there, and build a mosaic by jogging. The
// stage's long moves run on the simulator's fast clock, at their real
// distances; everything waits for the instrument's own conditions.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { expect, test } from "../e2e/fixtures.ts";
import { clockAt, FAST, inFocus, SMALL, TISSUE } from "../e2e/sim.ts";
import {
  arrives,
  at,
  homeAndMove,
  image,
  makeProject,
  readout,
  said,
  step,
  stream,
} from "./steps.ts";

const tool = (page: Page, name: string) =>
  page
    .getByRole("toolbar", { name: "What is drawn over the image" })
    .getByRole("button", { name, exact: true });

/** How many of the image's pixels are marked saturated: red. */
async function marked(page: Page): Promise<number> {
  const png = await image(page).screenshot({ scale: "css" });
  return page.evaluate(async (base64) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    const bitmap = await createImageBitmap(new Blob([bytes], { type: "image/png" }));
    const canvas = document.createElement("canvas");
    [canvas.width, canvas.height] = [bitmap.width, bitmap.height];
    const drawn = canvas.getContext("2d")!;
    drawn.drawImage(bitmap, 0, 0);
    const { data } = drawn.getImageData(0, 0, bitmap.width, bitmap.height);
    let red = 0;
    for (let i = 0; i < data.length; i += 4) {
      if (data[i]! > 240 && data[i + 1]! < 20 && data[i + 2]! < 20) red += 1;
    }
    return red;
  }, png.toString("base64"));
}

test("Checkpoint G: a live microscope, from homing to a mosaic built by jogging @main", async ({
  page,
  server,
}) => {
  test.setTimeout(600_000);
  const on = new Api(server.url);
  await page.goto(`${server.url}/#/control`);
  await expect(page.getByText("Online")).toBeVisible();

  // Homed, then over the tissue, and streaming: through a region as wide as the view's
  // device pixels, so that one to one is no smaller than the view, as with the whole sensor.
  await homeAndMove(page, server.url);
  const scale = await page.evaluate(() => window.devicePixelRatio);
  const [width, height] = [SMALL.width * scale, SMALL.height * scale];
  const centre = { x: SMALL.x + SMALL.width / 2, y: SMALL.y + SMALL.height / 2 };
  await stream(page, server.url, {
    x: centre.x - width / 2,
    y: centre.y - height / 2,
    width,
    height,
  });

  // A jog: the image focused, an arrow moves the stage by the step chosen.
  await step(page, "xy", "100 µm").click();
  const before = await at(server.url);
  await image(page).focus();
  await page.keyboard.press("ArrowRight");
  await expect
    .poll(async () => Math.abs((await at(server.url)).x - before.x - 100), { timeout: 30_000 })
    .toBeLessThanOrEqual(3);

  // Full resolution: a sensor pixel to a screen pixel.
  await image(page).focus();
  await page.keyboard.press("1");
  await expect(readout(page, "zoom")).toHaveText("100 %");
  await page.keyboard.press("0");

  // The saturation and the histogram, as the exposure changes. The marks are read first:
  // Playwright's WebKit on Windows draws the canvas black once the histogram resizes it.
  await tool(page, "Saturated").click();
  const exposure = said(page, "camera").getByRole("spinbutton", { name: "Exposure" });
  await exposure.fill("30");
  await exposure.press("Enter");
  // Each sample a screenshot, which takes WebKit 12 to 18 s on two cores (W3.6).
  await expect.poll(() => marked(page), { timeout: 90_000 }).toBeGreaterThan(0);
  await exposure.fill("10");
  await exposure.press("Enter");
  await expect.poll(() => marked(page), { timeout: 90_000 }).toBe(0);
  await tool(page, "Saturated").click();
  await tool(page, "Histogram").click();
  await expect(said(page, "values")).toBeVisible();

  // Autofocus, from 20 µm out of focus: Page Up twice at a step of 10 µm.
  const truth = await inFocus(server.url);
  await fastMove(server.url, { ...TISSUE, z: truth });
  await step(page, "z", "10 µm").click();
  await image(page).focus();
  for (const up of [10, 20]) {
    // A step pressed while the last one's request is under way is let go, as a held key's
    // repeats are: pressed again if the stage didn't move.
    await expect(async () => {
      await page.keyboard.press("PageUp");
      await arrives(page, server.url, { z: truth + up }, 15_000);
    }).toPass({ timeout: 60_000 });
  }
  const autofocus = said(page, "autofocus");
  await autofocus.getByRole("button", { name: "Focus", exact: true }).click();
  await expect(said(page, "focused")).toContainText("In focus at z", { timeout: 300_000 });
  const last = (await call((signal) => on.api.GET("/api/v1/autofocus/last", { signal })))!;
  expect(Math.abs((await at(server.url)).z - truth)).toBeLessThan(last.depth_of_field_um / 4);

  // A frame captured into a project made there, its thumbnail shown.
  await makeProject(page, "Checkpoint G");
  await said(page, "capture-run").click();
  await expect(said(page, "captured")).toContainText("Kept in Checkpoint G", { timeout: 60_000 });
  await expect(said(page, "thumbnail")).toHaveJSProperty("complete", true);

  // A mosaic built by jogging, a field of view at a time: a frame drawn at each new place.
  await step(page, "xy", "1 mm").click();
  await page.getByRole("button", { name: "Mosaic", exact: true }).click();
  await page.getByRole("button", { name: "Start the mosaic" }).click();
  await expect(said(page, "mosaic-said")).toContainText("Drawing", { timeout: 30_000 });
  const drawn = async () =>
    (await call((signal) => on.api.GET("/api/v1/live_mosaic/state", { signal })))!.counts.drawn;
  for (const key of ["ArrowRight", "ArrowRight", "ArrowDown"]) {
    const then = await drawn();
    // Pressed again if the last step's request was still under way, as above.
    await expect(async () => {
      await image(page).focus();
      await page.keyboard.press(key);
      await expect.poll(drawn, { timeout: 20_000 }).toBeGreaterThan(then);
    }).toPass({ timeout: 90_000 });
  }
  // A double click on the mosaic sends the stage there.
  const mosaic = (await said(page, "tiles").boundingBox())!;
  const was = await at(server.url);
  await page.mouse.dblclick(mosaic.x + mosaic.width * 0.3, mosaic.y + mosaic.height * 0.3);
  const gone = async () => {
    const now = await at(server.url);
    return Math.hypot(now.x - was.x, now.y - was.y);
  };
  await expect.poll(gone, { timeout: 60_000 }).toBeGreaterThan(50);
  await page.getByRole("button", { name: "Stop", exact: true }).click();
});

/** The stage moved to `to` on the fast clock. */
async function fastMove(url: string, to: { x: number; y: number; z: number }): Promise<void> {
  await clockAt(url, FAST);
  try {
    await act("/api/v1/stage/move_to", { ...to, speed: 25_000 }, { on: new Api(url) });
  } finally {
    await clockAt(url, 1);
  }
}
