// Covers: web/src/features/workspace
// A cell count and stains, seen on their image (W7.6): the outlines' count
// the table's; a row leading the view to its cell; a stain's channel shown
// alone, and DAB's score the server's; axe; and the outlines' baseline.
import type { Page } from "@playwright/test";

import { Server as Api } from "../../src/api/client.ts";
import { finished } from "../../src/api/jobs.ts";
import { fileAddress } from "../../src/api/media.ts";
import { recipeNamed, saveRecipe } from "../../src/api/pipeline.ts";
import { analyse } from "../../src/api/workspace.ts";
import { outlining, prepared, stained } from "./analyses.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { imported } from "./records.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

/** The discs counted and outlined, and a slide's stains separated, by the API: the stains' result's ID. */
async function analysed(url: string): Promise<{ project: string; stains: string }> {
  const on = new Api(url);
  const { project, acquisition } = await prepared(url);
  const slide = await imported(url, project, "hdab.tif", stained());
  await saveRecipe("outline-discs", outlining(10), { on });
  const run = async (recipe: Parameters<typeof analyse>[1]["recipe"], id: string) => {
    const ended = await finished(
      await analyse(project, { recipe, input: { kind: "acquisition", id } }, { on }),
      {
        on,
        timeout: 60_000,
      },
    );
    return ended.results[0]!.split("/").pop()!;
  };
  await run(outlining(10), acquisition);
  const stains = await run((await recipeNamed("stains-hdab", { on })).recipe, slide);
  return { project, stains };
}

/** The Analyses tab, the discs' project's results, and `name` shown. */
async function shown(page: Page, url: string, name: string): Promise<void> {
  await page.goto(`${url}/#/workspace`);
  await page.getByRole("tab", { name: "Analyses" }).click();
  await said(page, "run").getByRole("combobox", { name: "The project" }).click();
  await page.getByRole("option", { name: "Discs", exact: true }).click();
  await said(page, "results")
    .getByRole("button", { name: `Show it: ${name}` })
    .click();
  await expect(said(page, "result")).toBeVisible();
}

test("a count's outlines are its table's, and a row leads to its cell @main", async ({
  page,
  server,
}) => {
  await analysed(server.url);
  await shown(page, server.url, "Discs outlined");
  await expect(said(page, "count")).toHaveText(/^48 cells, [\d.]+ per mm²\.$/);
  await expect(said(page, "cells").locator("tbody tr")).toHaveCount(48);
  await expect(said(page, "outlines").locator("polygon")).toHaveCount(48);
  // A row: the view goes to its cell, and only the outlines about it are drawn.
  await said(page, "cells").getByRole("rowheader", { name: "7", exact: true }).click();
  await expect(said(page, "outlines").locator("polygon.chosen")).toHaveCount(1);
  await expect
    .poll(() => said(page, "outlines").locator("polygon").count(), { timeout: 15_000 })
    .toBeLessThan(10);
});

test("a stain's channel is shown alone, and DAB's score is the server's @main", async ({
  page,
  server,
}) => {
  const { project, stains } = await analysed(server.url);
  await shown(page, server.url, "Stains separated: H-DAB, and DAB scored");
  const channel = said(page, "result").getByRole("combobox", { name: "The channel shown" });
  await expect(channel).toHaveText("Hematoxylin");
  const dab = page.waitForResponse((answer) => answer.url().includes("channel=1"));
  await channel.click();
  await page.getByRole("option", { name: "DAB", exact: true }).click();
  expect((await dab).status()).toBe(200);
  await expect(said(page, "vectors").getByRole("row", { name: /^DAB/ })).toHaveText(
    "DAB0.2680.5700.776",
  );
  const csv = await (
    await fetch(fileAddress(project, `results/${stains}/score.csv`, new Api(server.url)))
  ).text();
  const [head, row] = csv
    .trim()
    .split(/\r?\n/)
    .map((line) => line.split(","));
  const of = (name: string) => Number(row![head!.indexOf(name)]).toFixed(2);
  await expect(said(page, "positive")).toHaveText(`${of("positive %")} %`);
  await expect(said(page, "h-score")).toHaveText(of("H-score"));
  await said(page, "result").scrollIntoViewIfNeeded();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W7.6-stains-${scheme}`);
  }
});

test("a count shown has no violation axe can find, in either theme", async ({ page, server }) => {
  await analysed(server.url);
  await shown(page, server.url, "Discs outlined");
  await expect(said(page, "outlines").locator("polygon")).toHaveCount(48);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("outlines over the whole image look as their baseline does @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await analysed(server.url);
  await shown(page, server.url, "Discs outlined");
  await expect(said(page, "outlines").locator("polygon")).toHaveCount(48);
  await said(page, "result").scrollIntoViewIfNeeded();
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(said(page, "cells-view")).toHaveScreenshot(`cells-${scheme}.png`);
    await shot(page, `W7.6-cells-${scheme}`);
  }
});
