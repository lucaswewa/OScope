// Covers: web/src/features/dev
// The live preview (W1.5), in the browser: frames drawn and acknowledged in
// Chromium, Firefox and WebKit; at 1920 pixels wide, every frame the server
// sent drawn; the frames' size following the window; a region asked for
// when zoomed in; resting while the page is hidden; statistics with the
// frames; axe, and the baselines.
import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { aRegion, onTissue } from "./sim.ts";

const PAGE = "/#/dev/gallery/preview";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

/** The camera started from the page, as its user would. */
async function streaming(page: Page): Promise<void> {
  await expect(said(page, "camera")).not.toHaveText("Not read yet.");
  await page.getByRole("button", { name: "Start" }).click();
  await expect(said(page, "camera")).toHaveText("Streaming.");
}

/** The readout's numbers: frames drawn and replaced, and the last frame's size. */
async function readout(page: Page) {
  const text = await said(page, "readout").innerText();
  const number = (pattern: RegExp) => Number(pattern.exec(text)?.[1] ?? NaN);
  return {
    drawn: number(/(\d+) drawn/),
    replaced: number(/(\d+) replaced/),
    width: number(/(\d+) × \d+, quality/),
    region: number(/Sensor pixels\s+(\d+) ×/),
  };
}

/**
 * How long the server's count of sessions may take to follow: a session
 * whose socket wasn't closed cleanly lingers 15 s (W1.5), which a loaded
 * computer meets (W3.1).
 */
const LINGER = { timeout: 60_000 };

/** The server's preview sessions, as its `preview` Thing lists them. */
async function sessions(url: string): Promise<Array<{ sent: number; dropped: number }>> {
  return (await fetch(`${url}/api/v1/preview/sessions`)).json();
}

/** Waits until more than `count` frames have been drawn. */
async function drawnPast(page: Page, count: number): Promise<void> {
  await expect
    .poll(async () => (await readout(page)).drawn, { timeout: 60_000 })
    .toBeGreaterThan(count);
}

test("frames are drawn and acknowledged, and the image moves on @main", async ({
  page,
  server,
}) => {
  await onTissue(server.url);
  await page.goto(server.url + PAGE);
  await streaming(page);
  await drawnPast(page, 2);
  await expect(said(page, "readout")).toContainText("open");
  const canvas = page.getByRole("img", { name: "The camera's live frame" });
  const before = await canvas.screenshot();
  // The tissue, not a blank canvas, whose picture would be a few kilobytes.
  expect(before.length).toBeGreaterThan(200_000);
  await drawnPast(page, (await readout(page)).drawn + 2);
  expect((await canvas.screenshot()).equals(before)).toBe(false);
});

test("at 1920 pixels wide, every frame the server sends is drawn @performance", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  // The canvas is 1920 pixels wide: the page's padding, and the side's column, around it.
  await page.setViewportSize({ width: 2344, height: 1500 });
  await onTissue(server.url);
  await page.goto(server.url + PAGE);
  await expect(said(page, "readout")).toContainText("WebGL2");
  await streaming(page);
  await drawnPast(page, 2);
  expect((await readout(page)).width).toBe(1920);
  const [start] = await sessions(server.url);
  const { drawn: from, replaced } = await readout(page);
  await drawnPast(page, from + 20);
  const [end] = await sessions(server.url);
  const to = await readout(page);
  // Sent while counting, and drawn, the server dropping none: two may be on
  // their way, or replaced by a newer one where a loaded computer's software
  // GL missed a refresh, as the Linux replays' does (W3.1).
  const sent = end!.sent - start!.sent;
  expect(to.drawn - from).toBeGreaterThanOrEqual(sent - 2);
  expect(end!.dropped - start!.dropped).toBe(0);
  expect(to.replaced - replaced).toBeLessThanOrEqual(2);
});

test("the frames' size follows the window", async ({ page, server }) => {
  await page.setViewportSize({ width: 1600, height: 1200 });
  await aRegion(server.url);
  await page.goto(server.url + PAGE);
  await streaming(page);
  await drawnPast(page, 1);
  const canvas = page.getByRole("img", { name: "The camera's live frame" });
  const width = async () => Math.round((await canvas.boundingBox())!.width);
  await expect.poll(async () => (await readout(page)).width).toBe(await width());
  await page.setViewportSize({ width: 1100, height: 1200 });
  await expect.poll(async () => (await readout(page)).width).toBe(await width());
  expect(await width()).toBeLessThan(1100);
});

test("zoomed in, it asks for the region it shows, and for the whole frame again", async ({
  page,
  server,
}) => {
  await aRegion(server.url);
  await page.goto(server.url + PAGE);
  await streaming(page);
  await drawnPast(page, 1);
  const whole = (await readout(page)).region;
  const canvas = page.getByRole("img", { name: "The camera's live frame" });
  await canvas.focus();
  for (const press of ["+", "+", "+"]) await page.keyboard.press(press);
  await expect.poll(async () => (await readout(page)).region).toBeLessThan(whole * 0.6);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("0");
  await expect.poll(async () => (await readout(page)).region).toBe(whole);
});

test("it rests while the page is hidden, and goes on when it is shown", async ({
  page,
  server,
}) => {
  await aRegion(server.url);
  await page.goto(server.url + PAGE);
  await streaming(page);
  await drawnPast(page, 1);
  await expect.poll(async () => (await sessions(server.url)).length, LINGER).toBe(1);
  const turn = (state: "hidden" | "visible") =>
    page.evaluate((now) => {
      Object.defineProperty(document, "visibilityState", { value: now, configurable: true });
      document.dispatchEvent(new Event("visibilitychange"));
    }, state);
  await turn("hidden");
  await expect(said(page, "readout")).toContainText("resting while the page is hidden");
  await expect.poll(async () => (await sessions(server.url)).length, LINGER).toBe(0);
  await turn("visible");
  await drawnPast(page, (await readout(page)).drawn);
  await expect.poll(async () => (await sessions(server.url)).length, LINGER).toBe(1);
});

test("statistics come with the frames when asked for", async ({ page, server }) => {
  await aRegion(server.url);
  await page.goto(server.url + PAGE);
  await streaming(page);
  await page.getByRole("switch", { name: "Statistics with the frames" }).click();
  await expect(said(page, "stats")).toContainText("Focus");
  await expect(said(page, "stats")).toContainText("Median");
  await expect(
    page.getByRole("img", { name: /It shows a histogram of red, green, blue,/ }),
  ).toBeVisible();
});

test("the preview page has no violation axe can find", async ({ page, server }) => {
  await page.goto(server.url + PAGE);
  await expect(said(page, "camera")).toHaveText("Not streaming: start it to see frames.");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the preview page looks as its baselines do @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.goto(server.url + PAGE);
  // The camera still: a black frame, and numbers that don't move.
  await expect(said(page, "camera")).toHaveText("Not streaming: start it to see frames.");
  await expect(said(page, "readout")).toContainText("open");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`preview-${scheme}.png`, { fullPage: true });
    await shot(page, `W1.5-preview-${scheme}`);
  }
});
