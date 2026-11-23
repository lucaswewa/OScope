// Covers: web/src/features/live, web/src/features/control
// The live viewer (W3.1): the camera's frames drawn live on Control, with
// the readouts; zooming in asking for the region shown, and that region
// coming back; the states, from not streaming to stale; the scale bar
// following the zoom; the view saved as a PNG, and full screen; axe; and
// the chrome's baselines, the frame hidden.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { call, Server as Api } from "../../src/api/client.ts";
import type { PixelRect } from "../../src/api/preview/message.ts";
import { roiFor } from "../../src/api/preview/view.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { aSmallRegion as aRegion, overTissue, SMALL } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });
const button = (page: Page, name: string) => page.getByRole("button", { name, exact: true });
const readout = (page: Page, name: string) => said(page, name).locator("dd");

/** The camera's region for these tests (sim.ts). */
const EXTENT = SMALL;
const region = (r: PixelRect) => `x ${r.x}, y ${r.y}, ${r.width} × ${r.height}`;

/** The region the frame readout says. */
async function frameOf(page: Page): Promise<PixelRect> {
  const text = await readout(page, "frame").innerText();
  const [x, y, width, height] =
    /x (\d+), y (\d+), (\d+) × (\d+)/.exec(text)?.slice(1).map(Number) ?? [];
  return { x: x ?? NaN, y: y ?? NaN, width: width ?? NaN, height: height ?? NaN };
}

/**
 * Whether `sent` is the region `asked`, as the camera sends it: rounded out
 * to its grid of two pixels, each edge at most a pixel out.
 */
function isAsked(sent: PixelRect, asked: PixelRect): boolean {
  const edges = [
    asked.x - sent.x,
    asked.y - sent.y,
    sent.x + sent.width - (asked.x + asked.width),
    sent.y + sent.height - (asked.y + asked.height),
  ];
  return edges.every((edge) => edge >= 0 && edge < 2);
}

/** The canvas's size, in device pixels. */
const canvasSize = (page: Page) =>
  image(page).evaluate((canvas: HTMLCanvasElement) => ({
    width: canvas.width,
    height: canvas.height,
  }));

/**
 * How long a first frame may take: on a loaded computer WebKit's are slow,
 * as W1.5's tests found.
 */
const FIRST_FRAME = { timeout: 60_000 };

/** Control, its camera started from the viewer, as its user would. */
async function live(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Not streaming");
  await button(page, "Start").click();
  await expect(readout(page, "state")).toHaveText("Live", FIRST_FRAME);
}

test("the camera's frames are drawn live on Control, and read out @main", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  await overTissue(server.url);
  // The stage moved on the fast clock, which is back at real time before anything streams.
  const rate = await call((signal) =>
    new Api(server.url).api.GET("/api/v1/simulation/rate", { signal }),
  );
  expect(rate).toBe(1);
  await live(page, server.url);
  await expect(readout(page, "rate")).not.toHaveText("0.0 fps");
  await expect(readout(page, "frame")).toHaveText(region(EXTENT));
  await expect(said(page, "scale")).toContainText(/\d µm$/);
  // The pointer over the image's middle: the camera region's middle pixel, near the sensor's.
  const box = (await image(page).boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await expect(readout(page, "pixel")).toHaveText(/^x \d+, y \d+$/);
  // A CSS pixel spans about a sensor pixel here: within a few of the middle, (2736, 1824).
  const [x, y] = ((await readout(page, "pixel").innerText()).match(/\d+/g) ?? []).map(Number);
  expect([Math.abs(x! - 2736), Math.abs(y! - 1824)].every((off) => off <= 4)).toBe(true);
  await expect(readout(page, "sample")).toHaveText(/^x -?\d+\.\d, y -?\d+\.\d µm$/);
  await button(page, "Stop").click();
  await expect(readout(page, "state")).toHaveText("Not streaming");
});

test("zooming in asks for the region shown, and that region comes back", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  await aRegion(server.url);
  await live(page, server.url);
  await expect(readout(page, "frame")).toHaveText(region(EXTENT));
  const whole = await said(page, "scale").innerText();
  // One to one, about the middle: the sensor's pixels the canvas shows, each on a device pixel.
  await image(page).focus();
  await page.keyboard.press("1");
  await expect(readout(page, "zoom")).toHaveText("100 %");
  const centre: [number, number] = [EXTENT.x + EXTENT.width / 2, EXTENT.y + EXTENT.height / 2];
  const asked = roiFor({ centre, scale: 1 }, EXTENT, await canvasSize(page))!;
  expect(asked.width).toBeLessThan(EXTENT.width);
  await expect.poll(async () => isAsked(await frameOf(page), asked), FIRST_FRAME).toBe(true);
  // Zoomed in further, the scale bar is shorter at the sample.
  for (let step = 0; step < 4; step++) await page.keyboard.press("+");
  await expect(said(page, "scale")).not.toHaveText(whole);
  // Fitted again, the whole region comes back.
  await page.keyboard.press("0");
  await expect(readout(page, "frame")).toHaveText(region(EXTENT), FIRST_FRAME);
  await expect(said(page, "scale")).toHaveText(whole);
});

test("the image waits for its first frame, and is stale once frames stop", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  const on = new Api(server.url);
  await aRegion(server.url);
  // Triggered by software, the camera streams and exposes only when told to.
  await call((signal) =>
    on.api.PUT("/api/v1/camera/trigger", { body: { kind: "software" }, signal }),
  );
  await page.goto(`${server.url}/#/control`);
  await expect(said(page, "live-said")).toHaveText(/isn't streaming/);
  await button(page, "Start").click();
  await expect(readout(page, "state")).toHaveText("Waiting for a frame");
  await expect(said(page, "live-said")).toHaveText(/first frame/);
  await act("/api/v1/camera/trigger_exposure", {}, { on });
  await expect(readout(page, "state")).toHaveText("Live", FIRST_FRAME);
  await expect(said(page, "live-said")).toHaveCount(0);
  // No frame for two seconds: stale, its last frame still shown.
  await expect(readout(page, "state")).toHaveText("Stale");
  await expect(said(page, "live-said")).toHaveText(/stale/);
  await act("/api/v1/camera/trigger_exposure", {}, { on });
  await expect(readout(page, "state")).toHaveText("Live");
});

test("the view is saved as a PNG, the preview as shown, and fills the screen", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  await overTissue(server.url);
  await live(page, server.url);
  const saving = page.waitForEvent("download");
  await button(page, "Save PNG").click();
  const download = await saving;
  expect(download.suggestedFilename()).toMatch(/^live-.+\.png$/);
  const png = readFileSync(await download.path());
  expect([...png.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
  // The canvas's size, not the frame's; and the tissue drawn, not a blank, which is a few kilobytes.
  const { width, height } = await canvasSize(page);
  expect([png.readUInt32BE(16), png.readUInt32BE(20)]).toEqual([width, height]);
  expect(png.length).toBeGreaterThan(50_000);

  await button(page, "Full screen").click();
  const filling = () =>
    page.evaluate(() => document.fullscreenElement?.getAttribute("aria-label") ?? null);
  await expect.poll(filling).toBe("The live image");
  await button(page, "Leave full screen").click();
  await expect.poll(filling).toBeNull();
});

test("the live viewer has no violation axe can find, in either theme", async ({ page, server }) => {
  await aRegion(server.url);
  await page.goto(`${server.url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Not streaming");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
  await button(page, "Start").click();
  await expect(readout(page, "state")).toHaveText("Live");
  await expectAccessible(page);
});

/** Hides the frame from the baselines: its noise differs each time. The chrome stays. */
const HIDDEN = fileURLToPath(new URL("hide-frame.css", import.meta.url));

test("the viewer's chrome looks as its baselines do, the frame hidden @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  test.setTimeout(120_000);
  await overTissue(server.url);
  await live(page, server.url);
  await button(page, "Stop").click();
  await expect(readout(page, "state")).toHaveText("Not streaming");
  await expect(readout(page, "rate")).toHaveText("0.0 fps");
  // The rail's indicator says the stage is homed: a moment after it was.
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W3.1-live-${scheme}`);
    await expect(page).toHaveScreenshot(`live-${scheme}.png`, {
      fullPage: true,
      stylePath: HIDDEN,
      mask: [readout(page, "latency")],
    });
  }
});
