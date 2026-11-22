// Covers: web/src/features/gallery
// The Gallery's projects (W6.1): a project made elsewhere appears; tags,
// a search by tag and a rename reach the server; a deletion asks first,
// saying what it takes; the grid is walked with the arrow keys; axe; and
// the grid's and its empty state's baselines.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { projectPage } from "../../src/api/projects.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { homed } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const card = (page: Page, name: string) =>
  said(page, "grid").getByRole("article", { name, exact: true });

/** Makes the projects named, by the API, as another client would. */
async function made(url: string, ...names: string[]): Promise<void> {
  for (const name of names) await act("/api/v1/projects/create", { name }, { on: new Api(url) });
}

/** Chooses a change from a card's menu. */
async function changing(page: Page, name: string, change: string): Promise<void> {
  await card(page, name)
    .getByRole("button", { name: `Change ${name}` })
    .click();
  await page.getByRole("menuitem", { name: change }).click();
}

/** Types into the open dialog's one field, and saves. */
async function typeAndSave(page: Page, value: string): Promise<void> {
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("textbox").fill(value);
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(dialog).toBeHidden();
}

test("a project made elsewhere appears @main", async ({ page, server }) => {
  await page.goto(`${server.url}/#/gallery`);
  await expect(said(page, "empty")).toContainText("No project yet");
  await made(server.url, "Tonsil");
  await expect(card(page, "Tonsil")).toBeVisible();
  await expect(said(page, "count")).toHaveText("One project");
});

test("tags, a search by tag and a rename reach the server", async ({ page, server }) => {
  await made(server.url, "Kidney", "Tonsil");
  await page.goto(`${server.url}/#/gallery`);
  await changing(page, "Tonsil", "Tags…");
  await typeAndSave(page, "he, human");
  await changing(page, "Kidney", "Tags…");
  await typeAndSave(page, "he");
  // Only the one tagged so.
  await page.getByLabel("Tag", { exact: true }).fill("human");
  await expect(said(page, "count")).toHaveText("One project");
  await expect(card(page, "Tonsil")).toBeVisible();
  await page.getByLabel("Tag", { exact: true }).fill("");
  await expect(said(page, "count")).toHaveText("2 projects");
  await changing(page, "Kidney", "Rename…");
  await typeAndSave(page, "Kidney 2");
  await expect(card(page, "Kidney 2")).toBeVisible();
  const listed = (await projectPage({ sort: "name" }, { on: new Api(server.url) })).projects;
  expect(listed.map((one) => [one.name, one.tags])).toEqual([
    ["Kidney 2", ["he"]],
    ["Tonsil", ["he", "human"]],
  ]);
});

test("a deletion asks first, saying what it takes", async ({ page, server }) => {
  await made(server.url, "Scratch");
  await page.goto(`${server.url}/#/gallery`);
  await changing(page, "Scratch", "Delete…");
  const question = page.getByRole("alertdialog");
  await expect(question).toContainText("it holds nothing yet");
  await question.getByRole("button", { name: "Cancel" }).click();
  await expect(card(page, "Scratch")).toBeVisible();
  await changing(page, "Scratch", "Delete…");
  await page.getByRole("alertdialog").getByRole("button", { name: "Delete the project" }).click();
  await expect(said(page, "empty")).toBeVisible();
  expect((await projectPage({}, { on: new Api(server.url) })).total).toBe(0);
});

test("the grid is walked with the arrow keys", async ({ page, server }) => {
  await made(server.url, "A", "B", "C");
  await page.goto(`${server.url}/#/gallery`);
  await page.getByLabel("Sort", { exact: true }).click();
  await page.getByRole("option", { name: "By name" }).click();
  const cards = said(page, "grid").locator(":scope > li");
  await expect(cards).toHaveCount(3);
  await cards.nth(0).focus();
  await page.keyboard.press("ArrowRight");
  await expect(cards.nth(1)).toBeFocused();
  await page.keyboard.press("End");
  await expect(cards.nth(2)).toBeFocused();
  await page.keyboard.press("Home");
  await expect(cards.nth(0)).toBeFocused();
});

test("the Gallery has no violation axe can find, in either theme", async ({ page, server }) => {
  await made(server.url, "Tonsil");
  await page.goto(`${server.url}/#/gallery`);
  await expect(card(page, "Tonsil")).toBeVisible();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the grid, and its empty state, look as their baselines do @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  // The stage homed: the rail's indicator steady, as in W4.1's baselines.
  await homed(server.url);
  await page.goto(`${server.url}/#/gallery`);
  await expect(said(page, "empty")).toBeVisible();
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`projects-empty-${scheme}.png`);
  }
  // One card: the grid's look, in any order.
  await made(server.url, "Tonsil");
  await expect(said(page, "grid").locator(":scope > li")).toHaveCount(1);
  await page.mouse.move(5, 5);
  // The dates are today's: masked.
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`projects-${scheme}.png`, {
      mask: [said(page, "grid").locator("p", { hasText: /^Made / })],
    });
    await shot(page, `W6.1-gallery-${scheme}`);
  }
});
