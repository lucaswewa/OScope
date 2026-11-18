// The API's page (W1.1): an action called through the typed client and
// followed to its end, refused, and cancelled; and the server's failures,
// each read as the client's one model of them.
import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

/** The page, and its parts that say what happened. */
async function open(page: Page, url: string) {
  await page.goto(`${url}/#/dev/gallery/api`);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("The API");
  const main = page.getByRole("main");
  return {
    main,
    states: main.locator('[data-said="states"]'),
    error: main.locator('[data-said="error"]'),
    output: main.locator('[data-said="output"]'),
    start: main.getByRole("button", { name: "Start" }),
  };
}

/** Chooses an action. */
async function choose(page: Page, action: string): Promise<void> {
  await page.getByRole("combobox", { name: "Action" }).click();
  await page.getByRole("option", { name: action }).click();
}

test("an action is followed to its end, refused, and cancelled @main", async ({ page, server }) => {
  test.setTimeout(60_000);
  const { main, states, error, output, start } = await open(page, server.url);
  // Before homing, the stage refuses to move: a device's refusal.
  await choose(page, "Move the stage to its middle");
  await start.click();
  await expect(error).toContainText("Refused by a device");
  await expect(error).toContainText("alarm");
  await expect(error).toContainText("409");
  // Homing, followed to its end, each state as it is seen.
  await choose(page, "Home the stage");
  await start.click();
  await expect(states).toContainText("completed", { timeout: 30_000 });
  await expect(states.getByRole("listitem").first()).toContainText("pending");
  await expect(output).toContainText('"position"');
  await expect(error).toHaveCount(0);
  // A slow move, cancelled on the server.
  await choose(page, "Move the stage slowly to its far corner");
  await start.click();
  await expect(states).toContainText("running");
  await main.getByRole("button", { name: "Cancel" }).click();
  await expect(states).toContainText("cancelled");
  await expect(error).toContainText("Cancelled");
});

test("the server's failures are read as one model", async ({ page, server }) => {
  const { main, error } = await open(page, server.url);
  const provoke = (name: string) => main.getByRole("button", { name }).click();
  await provoke("A value out of range");
  await expect(error).toContainText("Refused by a device");
  await expect(error).toContainText("out of range");
  await expect(error).toContainText("the gain must be from 0 to 240, and was -5");
  await provoke("A value of the wrong type");
  await expect(error).toContainText("Refused by the server's checks");
  for (const field of ["Field x", "Field y", "Field z"]) await expect(error).toContainText(field);
  await provoke("An invocation that isn't there");
  await expect(error).toContainText("Not there");
  await expect(error).toContainText("404");
  await provoke("A time limit of 0 ms");
  await expect(error).toContainText("Out of time");
});

test("the API's page has no violation axe can find, in either theme", async ({ page, server }) => {
  const { main, error } = await open(page, server.url);
  await main.getByRole("button", { name: "A value of the wrong type" }).click();
  await expect(error).toContainText("Field x");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the API's page looks as its baselines do, in both themes @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  const { main, error } = await open(page, server.url);
  // A failure whose words don't change from run to run.
  await main.getByRole("button", { name: "A value out of range" }).click();
  await expect(error).toContainText("was -5");
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`api-${scheme}.png`, { fullPage: true });
    await shot(page, `W1.1-api-${scheme}`);
  }
});
