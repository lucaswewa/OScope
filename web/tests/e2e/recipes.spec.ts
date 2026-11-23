// Covers: web/src/features/workspace
// The editor's parameters, library and live (W7.3): a parameter changed
// reaches the live image; a recipe saved, the page reloaded, and opened
// again, the same; unsaved changes asked about before they are left; the
// library's names checked as they are typed, and a recipe deleted; axe; and
// the pane's baseline.
import { readFile } from "node:fs/promises";

import type { Page } from "@playwright/test";

import { Server as Api, call } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { mean, rgbOf } from "./pixels.ts";
import { overTissue } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const tool = (page: Page, name: string) =>
  said(page, "editor").getByRole("button", { name, exact: true });
const param = (page: Page, name: string) =>
  said(page, "pane").getByRole("spinbutton", { name, exact: true });

/** The Workspace's editor, its catalogue read. */
async function workspace(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/workspace`);
  await expect(
    said(page, "palette").getByRole("button", { name: "Add filter.gaussian" }),
  ).toBeVisible();
}

/** The library's recipe `name`, opened. */
async function opened(page: Page, name: string): Promise<void> {
  await tool(page, "Open…").click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: `Open ${name}` })
    .click();
  await expect(said(page, "editor-said")).toHaveText(`${name} opened.`);
}

/** Node `id` chosen: its form in the pane. */
async function chosen(page: Page, id: string): Promise<void> {
  await said(page, "canvas").locator(`.vue-flow__node[data-id="${id}"]`).click();
  await expect(said(page, "pane").getByRole("heading")).toContainText(id);
}

/** The recipe exported: its file's text. */
async function exported(page: Page): Promise<string> {
  const saving = page.waitForEvent("download");
  await tool(page, "Export").click();
  return readFile(await (await saving).path(), "utf-8");
}

test("a parameter changed reaches the live image @main", async ({ page, server }) => {
  test.setTimeout(150_000);
  await overTissue(server.url);
  // A budget the debug build meets, beside other tests: 20 ms is a release build's.
  const on = new Api(server.url);
  await call((signal) => on.api.PUT("/api/v1/preview/recipe_budget_ms", { body: 500, signal }));
  await page.goto(`${server.url}/#/control`);
  await page.getByRole("button", { name: "Start", exact: true }).click();
  await workspace(page, server.url);
  await opened(page, "levels");
  await tool(page, "Live").click();
  await expect(said(page, "live")).toContainText("ms budget", { timeout: 30_000 });
  await expect(said(page, "node-levels").locator('[data-said="cost"]')).toContainText(" ms · ");
  const image = page.getByRole("img", { name: /^The live image/ });
  const luminance = ([r, g, b]: number[]) => 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
  const brightness = async () => mean((await rgbOf(page, image)).map(luminance));
  await expect.poll(brightness, { timeout: 30_000 }).toBeGreaterThan(10);
  const before = await brightness();
  // White at 0.3 of the range, where it was 0.85: the image brighter.
  await chosen(page, "levels");
  await param(page, "White").fill("0.3");
  await param(page, "White").press("Enter");
  await expect.poll(brightness, { timeout: 30_000 }).toBeGreaterThan(before + 20);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W7.3-live-${scheme}`);
  }
});

test("a recipe saved, the page reloaded, and opened again is the same @main", async ({
  page,
  server,
}) => {
  await workspace(page, server.url);
  await opened(page, "levels");
  await chosen(page, "levels");
  await expect(param(page, "White")).toHaveValue("0.85");
  await param(page, "White").fill("0.5");
  await param(page, "White").press("Enter");
  await expect(said(page, "changed")).toHaveText("Changed since it was saved: white.");
  await expect(said(page, "kept")).toHaveText("OScope's levels, with unsaved changes");
  // OScope's own: saved under a name of the user's.
  await tool(page, "Save").click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("textbox", { name: "Name" }).fill("my-levels");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "editor-said")).toHaveText("Saved as my-levels.");
  await expect(said(page, "kept")).toHaveText("Saved as my-levels");
  const file = await exported(page);
  await page.reload();
  await expect(
    said(page, "palette").getByRole("button", { name: "Add filter.gaussian" }),
  ).toBeVisible();
  await opened(page, "my-levels");
  expect(await exported(page)).toBe(file);
  await chosen(page, "levels");
  await expect(param(page, "White")).toHaveValue("0.5");
  await expect(said(page, "changed")).toHaveText("As it was saved.");
});

test("unsaved changes are asked about before they are left", async ({ page, server }) => {
  await workspace(page, server.url);
  await said(page, "palette").getByRole("button", { name: "Add threshold.otsu" }).click();
  // For another tab: kept, then left.
  const rail = page.getByRole("navigation", { name: "The app's tabs" });
  await rail.getByRole("link", { name: "Gallery", exact: true }).click();
  const question = page.getByRole("alertdialog", { name: "Leave the changes?" });
  await question.getByRole("button", { name: "Cancel" }).click();
  await expect(page).toHaveURL(/#\/workspace/);
  await expect(said(page, "node-otsu")).toHaveCount(1);
  // For another recipe: left.
  await tool(page, "New").click();
  await question.getByRole("button", { name: "Leave them" }).click();
  await expect(said(page, "node-otsu")).toHaveCount(0);
  // Nothing unsaved: not asked.
  await rail.getByRole("link", { name: "Gallery", exact: true }).click();
  await expect(page).toHaveURL(/#\/gallery/);
});

test("the library's names are checked as they are typed, and a recipe is deleted", async ({
  page,
  server,
}) => {
  await workspace(page, server.url);
  await tool(page, "Save as…").click();
  const dialog = page.getByRole("dialog");
  const name = dialog.getByRole("textbox", { name: "Name" });
  await expect(name).toHaveValue("a-new-recipe");
  await name.fill("My recipe");
  await expect(dialog).toContainText("Only a to z, 0 to 9, - and _.");
  await expect(dialog.getByRole("button", { name: "Save" })).toBeDisabled();
  await name.fill("levels");
  await expect(dialog).toContainText("That is one of OScope's own recipes.");
  await name.fill("mine");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "editor-said")).toHaveText("Saved as mine.");
  // Saved as itself: it says it replaces it.
  await tool(page, "Save as…").click();
  await expect(dialog).toContainText("Your recipe mine is replaced.");
  await dialog.getByRole("button", { name: "Cancel" }).click();
  // Deleted, once asked.
  await tool(page, "Open…").click();
  await dialog.getByRole("button", { name: "Delete mine" }).click();
  await page.getByRole("alertdialog").getByRole("button", { name: "Delete" }).click();
  await expect(said(page, "editor-said")).toHaveText("mine deleted from the library.");
  await expect(dialog.getByRole("button", { name: "Open mine" })).toHaveCount(0);
  await expect(dialog.getByRole("button", { name: "Open levels" })).toBeVisible();
});

test("the pane and the library have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  await workspace(page, server.url);
  await opened(page, "levels");
  await chosen(page, "levels");
  // At its own size, each port is 24 px tall: drawn smaller to fit, it is zoomed.
  await tool(page, "Actual size").click();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
  await tool(page, "Open…").click();
  await expect(page.getByRole("dialog").getByRole("button", { name: "Open levels" })).toBeVisible();
  await expectAccessible(page, '[role="dialog"]');
});

test("a node's form looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await workspace(page, server.url);
  await opened(page, "levels");
  await chosen(page, "levels");
  await param(page, "White").fill("0.5");
  await param(page, "White").press("Enter");
  await expect(said(page, "checked")).toHaveText("It can run.");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`recipes-${scheme}.png`);
    await shot(page, `W7.3-recipes-${scheme}`);
  }
});
