// The lights and the objective (W3.4): a light's intensity changing the
// image's mean, and the light switched off; fitting an objective changing
// the scale bar and the place's µm, and its check offered and run, or
// waiting for the live frames; the lights' recorded capabilities still the
// server's; axe; and the baselines.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { greys, mean } from "./pixels.ts";
import { overTissue } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const lights = (page: Page) => said(page, "lights");
const image = (page: Page) => page.getByRole("img", { name: /^The live image/ });
const readout = (page: Page, name: string) => said(page, name).locator("dd");

/** Control, the camera streaming over the tissue, its frames drawn. */
async function live(page: Page, url: string): Promise<void> {
  await overTissue(url);
  await page.goto(`${url}/#/control`);
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await expect(readout(page, "state")).toHaveText("Live", { timeout: 60_000 });
}

/** The image's mean grey, over a square in its middle, as drawn. */
async function brightness(page: Page): Promise<number> {
  const box = (await image(page).boundingBox())!;
  const png = await image(page).screenshot();
  return mean(await greys(page, png, [box.width / 2, box.height / 2], 200));
}

/** A light as the server has it now. */
async function light(url: string, channel: string): Promise<{ on: boolean; intensity: number }> {
  const all = (await (await fetch(`${url}/api/v1/illumination/lights`)).json()) as {
    channel: string;
    on: boolean;
    intensity: number;
  }[];
  return all.find((one) => one.channel === channel)!;
}

/** The µm from the centre the readout says, along x. */
async function fromCentre(page: Page): Promise<number> {
  const text = await readout(page, "sample").innerText();
  return Number(/^x (-?[\d.]+)/.exec(text)?.[1] ?? Number.NaN);
}

test("a light's intensity changes the image's mean @main", async ({ page, server }) => {
  test.setTimeout(120_000);
  await live(page, server.url);
  const bright = await brightness(page);
  // Dimmed to a fifth, typed as its user may: the image darkens.
  const intensity = lights(page).getByRole("spinbutton", { name: "Intensity of transmitted" });
  await intensity.fill("20");
  await intensity.press("Enter");
  await expect
    .poll(async () => (await light(server.url, "transmitted")).intensity)
    .toBeCloseTo(0.2);
  await expect.poll(() => brightness(page), { timeout: 30_000 }).toBeLessThan(bright * 0.75);
  // Off: the camera sees the dark.
  await lights(page).getByRole("switch", { name: "transmitted" }).click();
  await expect.poll(async () => (await light(server.url, "transmitted")).on).toBe(false);
  await expect.poll(() => brightness(page), { timeout: 30_000 }).toBeLessThan(bright * 0.2);
});

test("fitting an objective changes the scale, and offers its check @main", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  await live(page, server.url);
  const bar = said(page, "scale");
  await expect(bar).toContainText("µm");
  const before = await bar.innerText();
  // The pointer at a place right of the middle: its µm from the centre.
  const box = (await image(page).boundingBox())!;
  const place: [number, number] = [box.x + box.width * 0.75, box.y + box.height / 2];
  await page.mouse.move(...place);
  await expect(readout(page, "sample")).toHaveText(/^x \d+\.\d/);
  const through10x = await fromCentre(page);
  await choose(page, "Fitted", "50x, NA 0.55");
  await expect(bar).not.toHaveText(before);
  // The same place, the pointer back on it, through five times the magnification: a fifth of the µm.
  await page.mouse.move(...place);
  await expect.poll(async () => (await fromCentre(page)) / through10x).toBeCloseTo(0.2, 2);
  // The check is offered, and run: the simulated camera sees through the objective fitted.
  const offer = said(page, "objective-check");
  await expect(offer).toContainText("The 50x is fitted.");
  await offer.getByRole("button", { name: "Check the objective" }).click();
  await expect(said(page, "check")).toContainText("The 50x checks out", { timeout: 90_000 });
  await expect(offer).toHaveCount(0);
});

test("the check waits for the live frames, and says so", async ({ page, server }) => {
  await page.goto(`${server.url}/#/control`);
  // The panes drawn first: the lights' growing as their channels come would move the list.
  await expect(lights(page).getByRole("switch", { name: "red 640" })).toBeVisible();
  await expect(said(page, "objective")).toContainText("Mitutoyo M Plan Apo 10×");
  await choose(page, "Fitted", "20x, NA 0.42");
  const offer = said(page, "objective-check");
  await expect(offer).toContainText("The 20x is fitted.");
  await expect(offer).toContainText("start streaming first");
  await expect(offer.getByRole("button", { name: "Check the objective" })).toBeDisabled();
  await expect(said(page, "objective")).toContainText("Mitutoyo M Plan Apo 20×");
});

test("the lights' recorded capabilities are still the server's", async ({ server }) => {
  const recorded: unknown = JSON.parse(
    readFileSync(
      fileURLToPath(
        new URL("../../src/features/control/lights/recorded-capabilities.json", import.meta.url),
      ),
      "utf8",
    ),
  );
  const answer = await fetch(`${server.url}/api/v1/illumination/capabilities`);
  expect(await answer.json()).toEqual(recorded);
});

test("the lights and the objective have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/control`);
  await expect(lights(page).getByRole("switch", { name: "transmitted" })).toBeVisible();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the lights and the objective look as their baselines do @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  // Tall enough for the whole pane: scrolled, the stage's Halt would stand over it.
  await page.setViewportSize({ width: 1280, height: 3000 });
  await page.goto(`${server.url}/#/control`);
  await expect(lights(page).getByRole("switch", { name: "transmitted" })).toBeChecked();
  await expect(said(page, "objective")).toContainText("Mitutoyo M Plan Apo 10×");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(lights(page)).toHaveScreenshot(`lights-${scheme}.png`);
    await expect(said(page, "objective")).toHaveScreenshot(`objective-${scheme}.png`);
  }
  // The notes' pictures, at the usual size: the pane scrolled to the lights.
  await page.setViewportSize({ width: 1280, height: 800 });
  const controls = page.getByRole("region", { name: "The controls" });
  await expect
    .poll(() => controls.evaluate((box) => box.scrollHeight > box.clientHeight))
    .toBe(true);
  await lights(page).evaluate((section) => section.scrollIntoView({ block: "start" }));
  await expect.poll(async () => (await lights(page).boundingBox())!.y).toBeLessThan(100);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W3.4-lights-${scheme}`);
  }
});
