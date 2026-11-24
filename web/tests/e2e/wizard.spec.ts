// Covers: web/src/features/settings
// The calibration wizard (W8.3): run through on the simulator, its camera
// turned and its objectives magnifying 2 % less than the catalogue says,
// the geometry it finds within that truth; the flat field failed over the
// tissue, then retried over glass; a step skipped; a reload, and another
// browser, finding it where it was; cancelled; axe; and the notes' shots.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { fast, overTissue } from "./sim.ts";

// The truth the geometry is to find: the camera turned by −20°, and the 10× magnifying 9.8 times.
test.use({ simulator: "rotation = -20.0\nscale = 0.98" });

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const steps = (page: Page) => said(page, "wizard-steps").getByRole("listitem");
const inHand = (page: Page) => said(page, "wizard-step");
const button = (page: Page, name: string) =>
  said(page, "wizard").getByRole("button", { name, exact: true });

/** Settings' calibration view. */
async function calibration(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/settings`);
  await page.getByRole("tab", { name: "Calibration", exact: true }).click();
}

/** The camera streaming over the tissue. */
async function streaming(url: string): Promise<void> {
  await overTissue(url);
  await act("/api/v1/camera/start_streaming", {}, { on: new Api(url) });
}

/** The step in hand confirmed, and its job run until the step `n` (from 1) is `standing`. */
async function measured(page: Page, n: number, standing: string): Promise<void> {
  await button(page, "Done: measure it").click();
  await expect(steps(page).nth(n - 1)).toContainText(standing, { timeout: 120_000 });
}

test("the wizard, run through, finds the simulator's truth; a failed step is tried again, one skipped, and a reload finds it where it was @main", async ({
  page,
  server,
}) => {
  test.setTimeout(300_000);
  await streaming(server.url);
  await calibration(page, server.url);
  await button(page, "Start the wizard").click();
  await expect(steps(page)).toHaveCount(6);
  await expect(inHand(page)).toContainText("Step 1 of 6: Check the objective");
  // The objective, fitted here: its own pane beside the prompt.
  await expect(inHand(page).getByRole("combobox", { name: "Fitted" })).toBeVisible();
  await measured(page, 1, "Done");
  await expect(steps(page).nth(0)).toContainText("plausible: the 10x");
  // The geometry: measured beside the live image, within the simulator's truth.
  await expect(inHand(page)).toContainText("Step 2 of 6: Calibrate the geometry");
  await expect(said(page, "wizard").getByRole("img", { name: /^The live image/ })).toBeVisible();
  await measured(page, 2, "Done");
  await expect(steps(page).nth(1)).toContainText(/µm a pixel, turned -20\.\d\d°/);
  const active = await (await page.request.get(`${server.url}/api/v1/calibration/active`)).json();
  const { pixel_um: pixel, rotation_deg: rotation } = active.geometry.values;
  expect(Math.abs(pixel / (2.4 / (10 * 0.98)) - 1)).toBeLessThan(0.002);
  expect(Math.abs(rotation + 20)).toBeLessThan(0.1);

  // Reloaded halfway: where it was, its steps done still said.
  await page.reload();
  await page.getByRole("tab", { name: "Calibration", exact: true }).click();
  await expect(inHand(page)).toContainText("Step 3 of 6: Take a dark frame");
  await expect(steps(page).nth(1)).toContainText("µm a pixel");
  await measured(page, 3, "Done");

  // The flat field over the tissue fails, saying why; tried again over clear glass, it passes.
  await measured(page, 4, "Failed");
  await expect(inHand(page).getByRole("alert")).toContainText("isn't blank");
  const glass = { x: 10_000, y: 12_500, z: 5_000, speed: 25_000 };
  await fast(server.url, () => act("/api/v1/stage/move_to", glass, { on: new Api(server.url) }));
  await button(page, "Try it again").click();
  await expect(steps(page).nth(3)).toContainText("To do");
  await measured(page, 4, "Done");
  await expect(steps(page).nth(3)).toContainText("flat for the 10x in transmitted");

  // The colour target isn't loaded, and the stage is left as it is: both skipped.
  await expect(inHand(page)).toContainText("Step 5 of 6: Calibrate colour");
  await button(page, "Skip it").click();
  await expect(steps(page).nth(4)).toContainText("Skipped");
  await button(page, "Skip it").click();
  await expect(said(page, "wizard-ended")).toHaveText("Finished: 6 of 6 steps done or skipped.");
  // At its end, each step's set, in use; the live image gone with the step in hand.
  for (const n of [1, 2, 3]) await expect(steps(page).nth(n)).toContainText("In use");
  await expect(inHand(page)).toHaveCount(0);
  await expect(said(page, "wizard").getByRole("img", { name: /^The live image/ })).toHaveCount(0);
  await expect(button(page, "Start it again")).toBeVisible();
});

test("another browser finds the wizard where it was, and its cancel ends it in both", async ({
  page,
  server,
  browser,
}) => {
  test.setTimeout(120_000);
  await calibration(page, server.url);
  await button(page, "Start the wizard").click();
  await expect(inHand(page)).toContainText("Step 1 of 6");
  const other = await browser.newContext();
  try {
    const second = await other.newPage();
    await calibration(second, server.url);
    await expect(inHand(second)).toContainText("Step 1 of 6");
    await button(second, "Skip it").click();
    await expect(inHand(second)).toContainText("Step 2 of 6");
    // Followed in the first within its reading of the wizard, every second.
    await expect(inHand(page)).toContainText("Step 2 of 6", { timeout: 10_000 });
    await button(second, "Cancel the wizard").click();
    await expect(said(page, "wizard-ended")).toHaveText(
      "Cancelled, with 1 of 6 steps done or skipped.",
      { timeout: 10_000 },
    );
    await expect(said(second, "wizard-ended")).toContainText("Cancelled");
  } finally {
    await other.close();
  }
});

test("the wizard has no violation axe can find, in either theme", async ({ page, server }) => {
  await calibration(page, server.url);
  await button(page, "Start the wizard").click();
  await expect(inHand(page)).toContainText("Step 1 of 6");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the wizard under way, for the notes' screenshots", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
  test.setTimeout(240_000);
  await page.setViewportSize({ width: 1280, height: 900 });
  await streaming(server.url);
  await calibration(page, server.url);
  await button(page, "Start the wizard").click();
  await measured(page, 1, "Done");
  await measured(page, 2, "Done");
  await expect(inHand(page)).toContainText("Step 3 of 6");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W8.3-wizard-${scheme}`);
  }
});
