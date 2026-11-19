// The camera's controls (W3.2): each setting changes the camera; a value out
// of range refused at its field, by the field or by the camera in its words;
// the mode applied, and kept from while streaming; auto exposure run as a
// job, its value taken, and the saved settings put back; the recorded
// capabilities still the server's; axe; and the baselines.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { aSmallRegion, overTissue } from "./sim.ts";

const pane = (page: Page) => page.locator('[data-said="camera"]');
// A field's name may end with its unit, as the number field says it.
const field = (page: Page, name: string) =>
  pane(page).getByRole("spinbutton", { name: new RegExp(`^${name}`) });
const button = (page: Page, name: string) => pane(page).getByRole("button", { name, exact: true });

/** A camera property, as the server has it now. */
async function camera(url: string, name: string): Promise<unknown> {
  const answer = await fetch(`${url}/api/v1/camera/${name}`);
  return answer.json();
}

/** Chooses the mode whose label starts with `name`. */
async function chooseMode(page: Page, name: string): Promise<void> {
  await page.getByRole("combobox", { name: "Mode", exact: true }).click();
  await page.getByRole("option", { name: new RegExp(`^${name}:`) }).click();
}

/** Types `value` into the field named `name`, and takes it, as its user would. */
async function type(page: Page, name: string, value: string): Promise<void> {
  await field(page, name).fill(value);
  await field(page, name).press("Enter");
}

async function control(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/control`);
  await expect(pane(page)).toContainText("× 3648 pixels");
}

test("each setting changes the camera @main", async ({ page, server }) => {
  await control(page, server.url);
  await type(page, "Exposure", "12.5");
  await expect.poll(() => camera(server.url, "exposure")).toBe(12.5);
  await type(page, "Gain", "2");
  await expect.poll(() => camera(server.url, "gain")).toBe(2);
  await choose(page, "Trigger", "Software");
  await expect.poll(() => camera(server.url, "trigger")).toEqual({ kind: "software" });
  await choose(page, "Trigger", "Free running");
  await expect.poll(() => camera(server.url, "trigger")).toEqual({ kind: "free_running" });

  // The region, typed and applied; then the whole sensor again.
  await type(page, "Left", "1776");
  await type(page, "Top", "1176");
  await type(page, "Width", "1920");
  await type(page, "Height", "1272");
  await button(page, "Apply the region").click();
  await expect
    .poll(() => camera(server.url, "roi"))
    .toEqual({ x: 1776, y: 1176, width: 1920, height: 1272 });
  await button(page, "The whole sensor").click();
  await expect
    .poll(() => camera(server.url, "roi"))
    .toEqual({ x: 0, y: 0, width: 5472, height: 3648 });
});

test("a value out of range is refused at its field", async ({ page, server }) => {
  await control(page, server.url);
  // Past what the camera says it can do: the field itself refuses it.
  const caps = (await camera(server.url, "capabilities")) as { exposure_max: number };
  await type(page, "Exposure", String(caps.exposure_max * 2));
  await expect(pane(page)).toContainText(`At most ${caps.exposure_max}`);
  // A region off the sensor: the camera refuses it, in its words.
  await type(page, "Left", "99999");
  await button(page, "Apply the region").click();
  await expect(pane(page).locator('[data-said="region"]')).not.toBeEmpty();
  expect(await camera(server.url, "roi")).toBeNull();
});

test("the mode is applied, and kept from while the camera streams", async ({ page, server }) => {
  await aSmallRegion(server.url);
  await control(page, server.url);
  const before = (await camera(server.url, "settings")) as { mode: string };
  const other = before.mode === "full" ? "design" : "full";
  await chooseMode(page, other);
  await button(page, "Apply the mode").click();
  await expect
    .poll(async () => ((await camera(server.url, "settings")) as { mode: string }).mode)
    .toBe(other);
  // Streaming, the mode can't change: the button says why.
  await page.getByRole("button", { name: "Start", exact: true }).click();
  // A mode neither was: only streaming keeps it from being applied.
  await chooseMode(page, "bin2");
  await expect(button(page, "Apply the mode")).toBeDisabled();
  await expect(pane(page)).toContainText("Stop streaming to change the mode.");
});

test("auto exposure runs as a job, its value is taken, and the saved settings put back", async ({
  page,
  server,
}) => {
  test.setTimeout(90_000);
  await overTissue(server.url);
  await control(page, server.url);
  await type(page, "Exposure", "40");
  await expect.poll(() => camera(server.url, "exposure")).toBe(40);
  // It reads the live frames: off until the camera streams.
  await expect(button(page, "Auto exposure")).toBeDisabled();
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await button(page, "Auto exposure").click();
  const found = pane(page).locator('[data-said="auto"]');
  await expect(found).toHaveText(/ms/, { timeout: 60_000 });
  const exposure = (await camera(server.url, "exposure")) as number;
  expect(exposure).not.toBe(40);
  await expect(field(page, "Exposure")).toHaveValue(exposure.toFixed(2));
  // Auto exposure changed the camera behind the pane: the saved 40 ms can come back.
  await expect(pane(page).locator('[data-said="saved"]')).toContainText("exposure");
  await button(page, "Put the saved settings back").click();
  await expect.poll(() => camera(server.url, "exposure")).toBe(40);
  await expect(pane(page).locator('[data-said="saved"]')).toHaveCount(0);
});

test("the recorded capabilities are still the server's", async ({ server }) => {
  const recorded: unknown = JSON.parse(
    readFileSync(
      fileURLToPath(
        new URL("../../src/features/control/camera/recorded-capabilities.json", import.meta.url),
      ),
      "utf8",
    ),
  );
  expect(await camera(server.url, "capabilities")).toEqual(recorded);
});

test("the camera's controls have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  await control(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the camera's controls look as their baselines do @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await control(page, server.url);
  await expect(field(page, "Exposure")).not.toHaveValue("");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(pane(page)).toHaveScreenshot(`camera-${scheme}.png`);
    await shot(page, `W3.2-camera-${scheme}`);
  }
});
