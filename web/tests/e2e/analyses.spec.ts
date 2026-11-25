// Covers: web/src/features/workspace
// The Workspace's analyses (W7.4, P48): a run on a region makes a result
// with its provenance; a result made again is the same; a parameter changed
// shows in the difference, with the counts; a result deleted, once asked;
// axe; and the results' baseline.
import type { Page } from "@playwright/test";

import { prepared } from "./analyses.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const run = (page: Page) => said(page, "run");
const results = (page: Page) => said(page, "results").locator("li");

/** One of a select's options chosen: `option` of the field `field`. */
async function choose(page: Page, field: string, option: string): Promise<void> {
  await run(page).getByRole("combobox", { name: field }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
}

/** The analyses, the discs' project chosen, with `recipe`. */
async function analyses(page: Page, url: string, recipe = "Discs counted"): Promise<void> {
  await page.goto(`${url}/#/workspace`);
  await page.getByRole("tab", { name: "Analyses" }).click();
  await choose(page, "The recipe", recipe);
  await choose(page, "The project", "Discs");
  await expect(run(page).getByRole("combobox", { name: "The image" })).toContainText("discs");
}

/** The recipe run: a result listed, its job's notice put away. */
async function ran(page: Page): Promise<void> {
  const before = await results(page).count();
  await run(page).getByRole("button", { name: "Run" }).click();
  await expect(said(page, "run-said")).toHaveText("Done: its result is listed.", {
    timeout: 60_000,
  });
  await expect(results(page)).toHaveCount(before + 1);
}

/** Results `a` and `b`, by their place in the list, newest first, compared: others let go. */
async function compared(page: Page, a: number, b: number): Promise<void> {
  const pressed = said(page, "results").getByRole("button", { name: /^Compare /, pressed: true });
  while ((await pressed.count()) > 0) await pressed.first().click();
  for (const at of [a, b])
    await results(page)
      .nth(at)
      .getByRole("button", { name: /^Compare / })
      .click();
  await expect(said(page, "compared")).toBeVisible();
}

test("a run on a region makes a result with its provenance @main", async ({ page, server }) => {
  await prepared(server.url);
  await analyses(page, server.url);
  // The left half: 4 of the 8 columns of discs, 24 of the 48.
  await run(page).getByRole("switch", { name: "All of the image" }).click();
  await run(page).getByRole("spinbutton", { name: "Width" }).fill("192");
  await run(page).getByRole("spinbutton", { name: "Height" }).fill("288");
  await run(page).getByRole("spinbutton", { name: "Height" }).press("Tab");
  await ran(page);
  const result = results(page).first();
  await expect(result.locator('[data-said="counts"]')).toHaveText("objects: 24 rows");
  await expect(result.locator('[data-said="provenance"]')).toHaveText(/^From discs, by OScope /);
  await expect(result.getByRole("link", { name: "summary.json" })).toBeVisible();
});

test("a result made again is the same, and a parameter changed shows in the difference @main", async ({
  page,
  server,
}) => {
  test.setTimeout(150_000);
  await prepared(server.url);
  await analyses(page, server.url);
  await ran(page);
  // Made again: alike.
  await results(page).first().getByRole("button", { name: "Make again: Discs counted" }).click();
  await expect(said(page, "results-said")).toHaveText("Discs counted made again.", {
    timeout: 60_000,
  });
  await expect(results(page)).toHaveCount(2);
  await compared(page, 0, 1);
  await expect(said(page, "compared")).toContainText("They were made alike, and made the same.");
  // Its recipe opened in the editor, the least area raised, and run.
  await results(page)
    .last()
    .getByRole("button", { name: "Open its recipe: Discs counted" })
    .click();
  await said(page, "canvas").locator('.vue-flow__node[data-id="measure"]').click();
  const least = said(page, "pane").getByRole("spinbutton", { name: "Min area", exact: true });
  await least.fill("300");
  await least.press("Enter");
  await expect(said(page, "changed")).toHaveText("Changed since it was saved: min_area.");
  await page.getByRole("tab", { name: "Analyses" }).click();
  await choose(page, "The recipe", "In the editor: Discs counted");
  await ran(page);
  await compared(page, 2, 0);
  const rows = said(page, "compared").getByRole("row");
  await expect(rows.nth(1)).toHaveText(["measure: min_area", "10", "300", "+290"].join(""));
  await expect(rows.nth(2)).toHaveText(["objects: rows", "48", "24", "−24"].join(""));
  // The editor, kept as it was while the analyses were shown.
  await page.getByRole("tab", { name: "Recipe" }).click();
  await expect(said(page, "kept")).toHaveText("Not in the library, with unsaved changes");
});

test("a result is deleted, once asked", async ({ page, server }) => {
  await prepared(server.url);
  await analyses(page, server.url);
  await ran(page);
  await results(page).first().getByRole("button", { name: "Delete Discs counted" }).click();
  const question = page.getByRole("alertdialog", { name: "Delete the result?" });
  await question.getByRole("button", { name: "Cancel" }).click();
  await expect(results(page)).toHaveCount(1);
  await results(page).first().getByRole("button", { name: "Delete Discs counted" }).click();
  await question.getByRole("button", { name: "Delete" }).click();
  await expect(said(page, "results-said")).toHaveText("Discs counted deleted.");
  await expect(results(page)).toHaveCount(0);
});

test("the analyses have no violation axe can find, in either theme", async ({ page, server }) => {
  await prepared(server.url);
  await analyses(page, server.url);
  await ran(page);
  await ran(page);
  await compared(page, 0, 1);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the results look as their baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await prepared(server.url);
  await analyses(page, server.url);
  await ran(page);
  await ran(page);
  await compared(page, 0, 1);
  // The jobs' notices dismissed, and the times masked: they differ from run to run.
  const toasts = page.getByRole("region", { name: "Notifications (F8)" });
  await expect(toasts.getByRole("listitem")).toHaveCount(2);
  for (let left = 2; left > 0; left--) {
    await toasts.getByRole("button", { name: "Dismiss" }).first().click();
    await expect(toasts.getByRole("listitem")).toHaveCount(left - 1);
  }
  await page.mouse.move(5, 5);
  // The times, and the version that made each, which each release changes.
  const mask = [
    said(page, "made"),
    said(page, "compared").getByRole("heading"),
    said(page, "provenance"),
  ];
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`analyses-${scheme}.png`, { mask });
    await shot(page, `W7.4-analyses-${scheme}`);
  }
});
