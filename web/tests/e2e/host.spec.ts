// The host adapter (W1.4), in the browser: a saved file reaches the disk, an
// opened one is read, and what is kept survives a reload, in Chromium,
// Firefox and WebKit; a login kept for its tab alone; the clipboard, a
// notification, the window's title and full screen, in Chromium; axe, and
// the baselines.
import { readFile } from "node:fs/promises";

import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";
import { Server } from "./server.ts";

const PAGE = "/#/dev/gallery/host";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const field = (page: Page, name: string) => page.getByRole("textbox", { name, exact: true });

test("a saved file reaches the disk @main", async ({ page, server }) => {
  await page.goto(server.url + PAGE);
  const text = "Saved by the host's test.\nIts second line.";
  await field(page, "Text").fill(text);
  await field(page, "File name").fill("saved-by-a-test.txt");
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Save as a file" }).click();
  const saved = await download;
  expect(saved.suggestedFilename()).toBe("saved-by-a-test.txt");
  expect(await readFile(await saved.path(), "utf-8")).toBe(text);
});

test("an opened file is read @main", async ({ page, server }) => {
  await page.goto(server.url + PAGE);
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Open a file" }).click();
  const buffer = Buffer.from("x,y\n1,2\n");
  await (await chooser).setFiles({ name: "points.csv", mimeType: "text/csv", buffer });
  const opened = said(page, "opened");
  await expect(opened).toContainText("points.csv");
  await expect(opened).toContainText("8 bytes");
  await expect(opened).toContainText("text/csv");
  await expect(opened).toContainText("x,y");
});

test("what is kept survives a reload, and the theme with it @main", async ({ page, server }) => {
  await page.goto(server.url + PAGE);
  await field(page, "Something to keep").fill("kept by the host");
  await choose(page, "Theme", "Dark");
  await page.reload();
  await expect(field(page, "Something to keep")).toHaveValue("kept by the host");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("a login is kept for its tab: a reload stays in, and another tab logs in again", async ({
  page,
}) => {
  const server = new Server();
  server.addAccount("alice", "alice's password");
  await server.start();
  try {
    const heading = (on: Page) => on.getByRole("heading", { level: 1 });
    await page.goto(server.url + PAGE);
    await expect(heading(page)).toHaveText("Log in to OScope");
    await field(page, "Name").fill("alice");
    await page.getByLabel("Password").fill("alice's password");
    await page.getByRole("button", { name: "Log in" }).click();
    await expect(heading(page)).toHaveText("The host");
    await page.reload();
    await expect(heading(page)).toHaveText("The host");
    const other = await page.context().newPage();
    await other.goto(server.url + PAGE);
    await expect(heading(other)).toHaveText("Log in to OScope");
  } finally {
    await server.dispose();
  }
});

test("the clipboard, a notification, the title and full screen work in Chromium", async ({
  context,
  page,
  server,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write", "notifications"]);
  await page.goto(server.url + PAGE);
  await field(page, "Text").fill("Copied by the host's test.");
  await page.getByRole("button", { name: "Copy the text" }).click();
  await expect(said(page, "told")).toHaveText("The text was copied.");
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
    "Copied by the host's test.",
  );
  await page.getByRole("button", { name: "Raise a notification" }).click();
  await expect(said(page, "told")).toHaveText("The notification was shown.");

  await field(page, "Title").fill("A title of the test's");
  await expect(page).toHaveTitle("A title of the test's");
  const full = page.getByRole("switch", { name: "Full screen" });
  await full.click();
  await expect.poll(() => page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
  await expect(full).toBeChecked();
  await full.click();
  await expect.poll(() => page.evaluate(() => document.fullscreenElement === null)).toBe(true);
  await expect(full).not.toBeChecked();

  // Leaving the page gives the window back its name.
  await page.getByRole("link", { name: "Connection" }).click();
  await expect(page).toHaveTitle("OScope");
});

test("the host page has no violation axe can find", async ({ page, server }) => {
  await page.goto(server.url + PAGE);
  await expect(said(page, "platform")).toContainText("a browser");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the host page looks as its baselines do @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.goto(server.url + PAGE);
  await expect(said(page, "platform")).toContainText("a browser");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`host-${scheme}.png`, { fullPage: true });
    await shot(page, `W1.4-host-${scheme}`);
  }
});
