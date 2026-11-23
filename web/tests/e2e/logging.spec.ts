// Covers: web/src/features/logging
// The logging tab (W4.2): a record emitted appears at the tail, and under
// its filter; scrolling up holds the tail, and its end follows it again; a
// record opened shows its fields; a daily file is listed and downloaded;
// axe; and the tab's baseline.
import { readFileSync } from "node:fs";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import type { Level } from "../../src/api/media.ts";
import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, shot, test } from "./fixtures.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const records = (page: Page) => said(page, "records");

/** A record of the server's log, made from outside the page. */
async function emit(url: string, level: Level, message: string): Promise<void> {
  await act("/api/v1/logs/emit", { level, message }, { on: new Api(url) });
}

/** Whether the list shows its foot. */
const atItsEnd = (page: Page) =>
  records(page).evaluate((box) => box.scrollTop + box.clientHeight >= box.scrollHeight - 4);

test("a record emitted appears at the tail, and under its filter @main", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/logging`);
  await expect(records(page)).toBeVisible();
  // CUDA-default startup can report CPU fallback. Count this test's records.
  await page.getByRole("textbox", { name: "Text" }).fill("W4.2");
  await emit(server.url, "warn", "W4.2 a warning from the test");
  await expect(records(page).getByText("W4.2 a warning from the test")).toBeVisible();
  await expect(said(page, "counts")).toContainText("Warning 1");
  // Errors and worse: the warning goes, and an error comes.
  await choose(page, "At least", "Error");
  await expect(records(page).getByText("W4.2 a warning from the test")).toHaveCount(0);
  await emit(server.url, "error", "W4.2 an error from the test");
  await expect(records(page).getByText("W4.2 an error from the test")).toBeVisible();
  // By its words, at any level from information up: the error, and the warning not.
  await choose(page, "At least", "Info");
  await page.getByRole("textbox", { name: "Text" }).fill("an error from");
  await expect(records(page).getByText("W4.2 an error from the test")).toBeVisible();
  await expect(records(page).getByText("W4.2 a warning from the test")).toHaveCount(0);
});

test("scrolling up holds the tail, and its end follows it again", async ({ page, server }) => {
  test.setTimeout(120_000);
  await page.goto(`${server.url}/#/logging`);
  for (let n = 1; n <= 60; n++) await emit(server.url, "info", `W4.2 record ${n}`);
  await expect(records(page).getByText("W4.2 record 60", { exact: true })).toBeVisible();
  expect(await atItsEnd(page)).toBe(true);
  // Scrolled to the top: three more come, and the list holds where it is.
  await records(page).evaluate((box) => {
    box.scrollTop = 0;
    box.dispatchEvent(new Event("scroll"));
  });
  for (let n = 61; n <= 63; n++) await emit(server.url, "info", `W4.2 record ${n}`);
  await expect(said(page, "since")).toContainText("3 new records");
  expect(await records(page).evaluate((box) => box.scrollTop)).toBe(0);
  // Back to the end: it follows again.
  await said(page, "since").click();
  await expect.poll(() => atItsEnd(page)).toBe(true);
  await emit(server.url, "info", "W4.2 record 64");
  await expect(records(page).getByText("W4.2 record 64", { exact: true })).toBeVisible();
  expect(await atItsEnd(page)).toBe(true);
});

test("a record opened shows its fields, and a daily file is downloaded", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/logging`);
  await emit(server.url, "warn", "W4.2 a record to open");
  await records(page).getByText("W4.2 a record to open").click();
  await expect(said(page, "record")).toContainText("Number");
  await expect(said(page, "record")).toContainText("W4.2 a record to open");
  // Today's file, downloaded as it is: a record a line.
  const file = said(page, "files").getByRole("listitem").first();
  await expect(file).toContainText(/oscope-\d{4}-\d{2}-\d{2}\.jsonl/);
  const [download] = await Promise.all([
    page.waitForEvent("download"),
    file.getByRole("button", { name: /^Download/ }).click(),
  ]);
  expect(download.suggestedFilename()).toMatch(/^oscope-\d{4}-\d{2}-\d{2}\.jsonl$/);
  const lines = readFileSync((await download.path())!, "utf8")
    .trim()
    .split("\n");
  expect(lines.some((line) => line.includes("W4.2 a record to open"))).toBe(true);
});

test("the logging tab has no violation axe can find, in either theme", async ({ page, server }) => {
  await page.goto(`${server.url}/#/logging`);
  await emit(server.url, "warn", "W4.2 a warning for axe");
  await records(page).getByText("W4.2 a warning for axe").click();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the logging tab looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.goto(`${server.url}/#/logging`);
  await emit(server.url, "warn", "W4.2 a warning for the baseline");
  await records(page).getByText("W4.2 a warning for the baseline").click();
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The server's own records, their counts, the times and the day's file are the moment's.
    await expect(page).toHaveScreenshot(`logging-${scheme}.png`, {
      mask: [
        records(page),
        said(page, "counts"),
        said(page, "record").locator("dd"),
        said(page, "files").locator("li"),
      ],
    });
    await shot(page, `W4.2-logging-${scheme}`);
  }
});
