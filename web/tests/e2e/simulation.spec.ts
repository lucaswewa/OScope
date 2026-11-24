// Covers: web/src/features/settings
// The simulated samples (W8.4): a sample loaded from Settings changing the
// live frames; one's own image uploaded, imported as a sample, kept, and
// seen; the clock's rate changing how long a move takes; the next start's
// settings saved, and a wrong one refused there and then; a fault injected
// from the page and cleared; axe; the view's baseline; and the notes'
// screenshots.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { colourOf, rgbOf } from "./pixels.ts";
import { fast, homed, overTissue } from "./sim.ts";
import { rgbTiffOf } from "./tiff.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });
const readout = (page: Page, name: string) => said(page, name).locator("dd");
const card = (page: Page, name: string) => page.locator(`[data-sample="${name}"]`);

/** Settings' simulation view. */
async function simulation(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/settings`);
  await page.getByRole("tab", { name: "Simulation", exact: true }).click();
}

/** Control, its frames drawn: how coloured the live image is. */
async function liveColour(page: Page, url: string): Promise<() => Promise<number>> {
  await page.goto(`${url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  return async () => colourOf(await rgbOf(page, image(page)));
}

test("a sample loaded from Settings changes the live frames @main", async ({ page, server }) => {
  test.setTimeout(150_000);
  await overTissue(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on: new Api(server.url) });
  // The stained tissue: pink and purple.
  const colour = await liveColour(page, server.url);
  await expect.poll(colour, { timeout: 30_000 }).toBeGreaterThan(40);
  await simulation(page, server.url);
  await expect(card(page, "H&E tissue")).toContainText("On the stage");
  await expect(card(page, "H&E tissue").getByRole("img")).toBeVisible();
  // The test target, grey, which isn't under the stage: the clear slide.
  await card(page, "test target").getByRole("button", { name: "Load it" }).click();
  await expect(said(page, "samples-said")).toHaveText("test target is on the stage.");
  await expect(card(page, "test target")).toContainText("On the stage");
  const now = await liveColour(page, server.url);
  await expect.poll(now, { timeout: 30_000 }).toBeLessThan(5);
});

test("one's own image, uploaded, is a sample, kept with the data and seen", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  const on = new Api(server.url);
  await homed(server.url);
  // 400 × 300 pixels of 10 µm, red, about the coverslip's middle, where an image lies.
  const middle = { x: 47_000, y: 12_500, z: 5_000, speed: 25_000 };
  await fast(server.url, () => act("/api/v1/stage/move_to", middle, { on }));
  await simulation(page, server.url);
  const choosing = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Choose a file…" }).click();
  const red = rgbTiffOf(400, 300, () => [220, 30, 30]);
  await (await choosing).setFiles({ name: "red slide.tif", mimeType: "image/tiff", buffer: red });
  const form = said(page, "own-sample");
  await expect(form.getByRole("textbox", { name: "Its name" })).toHaveValue("red slide");
  const pixel = form.getByRole("spinbutton", { name: "A pixel, in µm" });
  await pixel.fill("10");
  await pixel.press("Tab");
  await form.getByRole("button", { name: "Import it" }).click();
  await expect(said(page, "samples-said")).toHaveText("red slide is a sample: load it to see it.");
  await expect(card(page, "red slide")).toContainText("Your own image, without ground truth.");
  await card(page, "red slide").getByRole("button", { name: "Load it" }).click();
  await expect(card(page, "red slide")).toContainText("On the stage");
  // Seen: the live image is red.
  await act("/api/v1/camera/start_streaming", {}, { on });
  await page.goto(`${server.url}/#/control`);
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
  const reddish = async () => {
    const pixels = await rgbOf(page, image(page));
    return (
      pixels.filter(([r, g, b]) => r! > 120 && r! > 2 * g! && r! > 2 * b!).length / pixels.length
    );
  };
  await expect.poll(reddish, { timeout: 30_000 }).toBeGreaterThan(0.5);
});

test("the clock's rate changes how long a move takes", async ({ page, server }) => {
  test.setTimeout(120_000);
  const on = new Api(server.url);
  await homed(server.url);
  /** How long, in s, a move of 2 mm at 1 mm/s takes. */
  let x = 20_000;
  const move = async () => {
    x += 2_000;
    const started = performance.now();
    await act("/api/v1/stage/move_to", { x, y: 12_000, z: 5_000, speed: 1_000 }, { on });
    return (performance.now() - started) / 1_000;
  };
  expect(await move()).toBeGreaterThan(1.8);
  await simulation(page, server.url);
  const clock = page.getByRole("spinbutton", { name: "Its clock" });
  await expect(clock).toHaveValue("1.00");
  await clock.fill("10");
  await clock.press("Enter");
  await expect
    .poll(async () => (await page.request.get(`${server.url}/api/v1/simulation/rate`)).json())
    .toBe(10);
  expect(await move()).toBeLessThan(0.6);
});

test("the next start's settings are saved, a wrong one refused there and then, and a fault injected", async ({
  page,
  server,
}) => {
  await simulation(page, server.url);
  const form = said(page, "simulator-form");
  const backlash = form.getByRole("spinbutton", { name: "Backlash, in µm" });
  await expect(backlash).toHaveValue("2.00");
  await backlash.fill("3.5");
  await backlash.press("Tab");
  // A magnification of 0 can't be: said at its field, and nothing saved.
  const scale = form.getByRole("spinbutton", { name: "Magnification over the catalogue's" });
  await scale.fill("0");
  await scale.press("Tab");
  await expect(form).toContainText("More than 0");
  await expect(form.getByRole("button", { name: "Save" })).toBeDisabled();
  await scale.fill("1.02");
  await scale.press("Tab");
  await form.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "simulator-said")).toHaveText(
    "Saved: Magnification over the catalogue's, Backlash, in µm.",
  );
  const saved = await (await page.request.get(`${server.url}/api/v1/settings/simulator`)).json();
  expect([saved.backlash, saved.scale, saved.response]).toEqual([
    3.5,
    1.02,
    [
      [1, 0, 0],
      [0, 1, 0],
      [0, 0, 1],
    ],
  ]);
  await expect(said(page, "simulator-restart")).toContainText("when it restarts", {
    timeout: 15_000,
  });
  // The camera unplugged, from here: the shell's indicator says so; then plugged in again.
  // Homed first, the instrument is ready: an unhomed stage is a fault of its own.
  await homed(server.url);
  const indicator = said(page, "indicator");
  const unplugged = said(page, "faults").getByRole("switch", { name: "The camera unplugged" });
  await expect(indicator).toHaveAccessibleName("The instrument: OK", { timeout: 15_000 });
  await unplugged.click();
  await expect(indicator).toHaveAccessibleName("The instrument: Fault");
  await expect(unplugged).toBeChecked();
  await unplugged.click();
  await expect(unplugged).not.toBeChecked();
  await expect(indicator).toHaveAccessibleName("The instrument: OK", { timeout: 15_000 });
});

test("the simulation view has no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  await simulation(page, server.url);
  await expect(card(page, "colour target").getByRole("img")).toBeVisible();
  await page.getByRole("button", { name: "Choose a file…" }).click();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the simulation view looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.setViewportSize({ width: 1280, height: 1000 });
  await simulation(page, server.url);
  for (const name of ["H&E tissue", "colour target"]) {
    await expect(card(page, name).getByRole("img")).toBeVisible();
  }
  await expect(card(page, "H&E tissue")).toContainText("On the stage");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`simulation-${scheme}.png`);
  }
});

test("the simulation view, for the notes' screenshots", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
  await page.setViewportSize({ width: 1280, height: 1700 });
  await simulation(page, server.url);
  for (const name of ["H&E tissue", "colour target"]) {
    await expect(card(page, name).getByRole("img")).toBeVisible();
  }
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W8.4-simulation-${scheme}`);
  }
});
