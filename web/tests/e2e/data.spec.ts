// Covers: web/src/features/dev
import type { Locator, Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

/** The page of data, and what it says one of its parts holds now. */
async function open(page: Page, url: string): Promise<(name: string) => Locator> {
  await page.goto(`${url}/#/dev/gallery/data`);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  return (name) => page.locator(`[data-said="${name}"]`);
}

/** The log's table, and one of its cells by row (-1 the header's) and column. */
function table(page: Page) {
  const grid = page.getByRole("grid", { name: "The scan's log" });
  return { grid, cell: (row: number, col: number) => grid.locator(`[data-cell="${row}:${col}"]`) };
}

test("the table of 100,000 rows works from the keyboard @main", async ({ page, server }) => {
  const said = await open(page, server.url);
  const keys = page.keyboard;
  const { grid, cell } = table(page);
  await expect(grid).toHaveAttribute("aria-rowcount", "100001");

  // One stop for Tab: the first header's cell.
  // From the toolbar before it, itself one stop.
  await page.getByRole("button", { name: "Errors only" }).focus();
  await keys.press("Tab");
  await expect(cell(-1, 0)).toBeFocused();

  // The arrows, a row's ends, and the grid's.
  await keys.press("ArrowDown");
  await expect(cell(0, 0)).toBeFocused();
  await expect(cell(0, 0)).toHaveText("1");
  await keys.press("ArrowRight");
  await expect(cell(0, 1)).toHaveText("09:00:00.000");
  await expect(cell(0, 1)).toBeFocused();
  await keys.press("End");
  await expect(cell(0, 4)).toBeFocused();
  await keys.press("Control+End");
  await expect(cell(99_999, 4)).toBeFocused();
  await expect(cell(99_999, 0)).toHaveText("100000");
  await expect(grid.locator("[aria-rowindex='100001']")).toBeVisible();
  // Only the rows in view are drawn.
  expect(await grid.locator("[role=row]").count()).toBeLessThan(50);
  await keys.press("PageUp");
  await expect(grid.locator(":focus")).toHaveAttribute("data-cell", /^999[0-9]{2}:4$/);
  await keys.press("Control+Home");
  await expect(cell(-1, 0)).toBeFocused();
  await expect(cell(0, 0)).toBeInViewport();

  // A header sorts by its column: Enter, and again the other way.
  await keys.press("ArrowRight");
  await keys.press("ArrowRight");
  const level = grid.getByRole("columnheader", { name: "Level" });
  await expect(level).toBeFocused();
  await keys.press("Enter");
  await expect(level).toHaveAttribute("aria-sort", "ascending");
  await expect(cell(0, 2)).toHaveText("debug");
  await keys.press("Enter");
  await expect(level).toHaveAttribute("aria-sort", "descending");
  await expect(cell(0, 2)).toHaveText("warning");

  // Space picks a row, and another; Ctrl+A picks them all.
  await keys.press("ArrowDown");
  await keys.press("Space");
  await expect(grid.locator("[aria-rowindex='2']")).toHaveAttribute("aria-selected", "true");
  await keys.press("ArrowDown");
  await keys.press("Space");
  await expect(said("log")).toHaveText("100,000 of 100,000 lines shown, 2 picked");
  await keys.press("Space");
  await expect(said("log")).toHaveText("100,000 of 100,000 lines shown, 1 picked");
  await keys.press("Control+a");
  await expect(said("log")).toHaveText("100,000 of 100,000 lines shown, 100,000 picked");
});

test("the table sorts and picks with the pointer, and keeps its columns' widths @main", async ({
  page,
  server,
}) => {
  const said = await open(page, server.url);
  const { grid, cell } = table(page);

  // A press on a header sorts by it.
  await grid.getByRole("columnheader", { name: "Message" }).click();
  await expect(cell(0, 4)).toHaveText(/^Background checked/);

  // A press picks a row; Shift picks those between; Ctrl one more or one fewer.
  await cell(2, 0).click();
  await cell(5, 0).click({ modifiers: ["Shift"] });
  await expect(said("log")).toHaveText("100,000 of 100,000 lines shown, 4 picked");
  await cell(3, 0).click({ modifiers: ["Control"] });
  await expect(said("log")).toHaveText("100,000 of 100,000 lines shown, 3 picked");
  const picked = await cell(5, 0).textContent();
  await expect(page.getByRole("heading", { name: `Line ${picked}` })).toBeVisible();

  // A toolbar's button that stays pressed: errors only, an eighth of them.
  const errors = page.getByRole("button", { name: "Errors only" });
  await errors.click();
  await expect(errors).toHaveAttribute("aria-pressed", "true");
  await expect(said("log")).toHaveText("12,500 of 100,000 lines shown, 3 picked");
  await errors.click();
  await expect(errors).toHaveAttribute("aria-pressed", "false");

  // A column's edge dragged, and a header's width from the keyboard.
  const source = grid.getByRole("columnheader", { name: "Source" });
  const time = grid.getByRole("columnheader", { name: "Time" });
  const before = (await source.boundingBox())!;
  const grip = source.locator(".grip");
  const at = (await grip.boundingBox())!;
  await page.mouse.move(at.x + at.width / 2, at.y + at.height / 2);
  await page.mouse.down();
  await page.mouse.move(at.x + at.width / 2 + 60, at.y + at.height / 2, { steps: 4 });
  await page.mouse.up();
  await expect
    .poll(async () => (await source.boundingBox())!.width)
    .toBeCloseTo(before.width + 60, 0);
  await time.focus();
  const was = (await time.boundingBox())!.width;
  await page.keyboard.press("Shift+ArrowRight");
  await expect.poll(async () => (await time.boundingBox())!.width).toBeCloseTo(was + 16, 0);

  // Kept for the next visit.
  const widths = [(await source.boundingBox())!.width, (await time.boundingBox())!.width];
  await page.reload();
  await expect(grid).toBeVisible();
  expect((await source.boundingBox())!.width).toBeCloseTo(widths[0]!, 0);
  expect((await time.boundingBox())!.width).toBeCloseTo(widths[1]!, 0);
});

test("the panes resize from the keyboard, and keep their sizes @main", async ({ page, server }) => {
  const said = await open(page, server.url);
  const handle = page.getByRole("separator", { name: "The log and the line picked" });
  await expect(said("panes")).toHaveText("70 : 30");
  await handle.focus();
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await expect(said("panes")).toHaveText("60 : 40");
  await page.keyboard.press("End");
  await expect(said("panes")).toHaveText("80 : 20");
  await page.keyboard.press("ArrowLeft");
  await expect(said("panes")).toHaveText("75 : 25");
  await page.reload();
  await expect(said("panes")).toHaveText("75 : 25");
  await expect(handle).toHaveAttribute("aria-valuenow", "75");
});

test("the tabs, the tree, the toolbar, the pages and the sections work from the keyboard @main", async ({
  page,
  server,
}) => {
  const said = await open(page, server.url);
  const keys = page.keyboard;

  // A toolbar is one stop, its buttons a row for the arrows.
  await page.getByRole("button", { name: "Errors only" }).focus();
  await keys.press("ArrowRight");
  await expect(page.getByRole("button", { name: "Newest first" })).toBeFocused();
  await keys.press("Enter");
  await expect(table(page).cell(0, 1)).toHaveText(/^12:48:/);

  // The tabs: an arrow shows the next at once.
  const log = page.getByRole("tab", { name: "A long log" });
  await log.focus();
  await keys.press("ArrowRight");
  const files = page.getByRole("tab", { name: "Files" });
  await expect(files).toBeFocused();
  await expect(files).toHaveAttribute("aria-selected", "true");

  // The tree, by its pattern.
  const tree = page.getByRole("tree", { name: "The project's files" });
  const item = (name: string) => tree.getByRole("treeitem", { name, exact: true });
  await item("scans").focus();
  // A letter first, before any other key: the next item it starts, once.
  await keys.press("s");
  await expect(item("slide-4")).toBeFocused();
  await keys.press("Home");
  await keys.press("ArrowDown");
  await expect(item("slide-4")).toBeFocused();
  await expect(item("slide-4")).toHaveAttribute("aria-expanded", "false");
  await keys.press("ArrowRight");
  await expect(item("slide-4")).toHaveAttribute("aria-expanded", "true");
  await keys.press("ArrowRight");
  await expect(item("mosaic.ome.tiff").first()).toBeFocused();
  await keys.press("Enter");
  await expect(said("file")).toHaveText("scans/slide-4/mosaic.ome.tiff");
  await keys.press("ArrowLeft");
  await expect(item("slide-4")).toBeFocused();
  await keys.press("ArrowLeft");
  await expect(item("slide-4")).toHaveAttribute("aria-expanded", "false");
  await keys.press("End");
  await expect(item("project.json")).toBeFocused();
  await keys.press("Home");
  await expect(item("scans")).toBeFocused();
  await keys.press("c");
  await expect(item("captures")).toBeFocused();
  await keys.press("Space");
  await expect(said("file")).toHaveText("captures");

  // The pieces: the pages, and the sections.
  await files.focus();
  await keys.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "Pieces" })).toHaveAttribute("aria-selected", "true");
  const pages = page.getByRole("navigation", { name: "Pages" });
  await pages.getByRole("button", { name: "Page 12" }).focus();
  await keys.press("Enter");
  await expect(said("page")).toHaveText("Page 12 of 12");
  await expect(pages.getByRole("button", { name: "Page 12" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(pages.getByRole("button", { name: "The page after" })).toBeDisabled();
  await pages.getByRole("button", { name: "The page before" }).click();
  await expect(said("page")).toHaveText("Page 11 of 12");

  const stitching = page.getByRole("button", { name: "Stitching settings" });
  await expect(stitching).toHaveAttribute("aria-expanded", "false");
  await stitching.focus();
  await keys.press("Enter");
  await expect(stitching).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByText("Tiles are placed by their overlap")).toBeVisible();

  // A part that scrolls on its own is reached, and scrolled, by the keyboard.
  const notes = page.getByRole("region", { name: "Release notes" });
  await notes.focus();
  await keys.press("PageDown");
  await expect.poll(() => notes.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
});

test("scrolling 100,000 rows keeps to a frame's budget", async ({ page, server, browserName }) => {
  test.skip(browserName !== "chromium", "measured where the browser can say what took long");
  await open(page, server.url);
  await expect(table(page).grid).toBeVisible();
  const measured = await table(page).grid.evaluate(async (grid) => {
    const long: number[] = [];
    const watcher = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) long.push(entry.duration);
    });
    watcher.observe({ type: "longtask", buffered: false });
    const frames: number[] = [];
    let drawn = 0;
    await new Promise<void>((done) => {
      let last = performance.now();
      let n = 0;
      const step = (now: number): void => {
        frames.push(now - last);
        last = now;
        drawn = Math.max(drawn, grid.querySelectorAll("[role=row]").length);
        grid.scrollTop += 900;
        if (++n < 240) requestAnimationFrame(step);
        else done();
      };
      requestAnimationFrame(step);
    });
    watcher.disconnect();
    const sorted = frames.slice(1).sort((a, b) => a - b);
    const at = (share: number) => sorted[Math.floor(share * (sorted.length - 1))]!;
    return { median: at(0.5), p95: at(0.95), long, drawn, top: grid.scrollTop };
  });
  console.log(
    `scrolling 100,000 rows: frames median ${measured.median.toFixed(1)} ms, ` +
      `95th ${measured.p95.toFixed(1)} ms; ${measured.long.length} long tasks; ` +
      `at most ${measured.drawn} rows drawn; scrolled to ${measured.top} px`,
  );
  expect(measured.top).toBeGreaterThan(200_000);
  expect(measured.drawn).toBeLessThan(50);
  // A frame at 60 a second is 16.7 ms: half of them, at least, are drawn within it.
  expect(measured.median).toBeLessThan(17.5);
  // Nothing held the page up for as long as a long task (50 ms) while it scrolled.
  expect(measured.long).toEqual([]);
});

test("each tab passes axe, in both themes", async ({ page, server }) => {
  await open(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    for (const name of ["A long log", "Files", "Pieces"]) {
      await page.getByRole("tab", { name }).click();
      await expect(page.getByRole("tab", { name })).toHaveAttribute("aria-selected", "true");
      await expectAccessible(page);
    }
    await page.getByRole("tab", { name: "A long log" }).click();
  }
  await page.getByRole("tab", { name: "A long log" }).click();
  await inScheme(page, "light");
  await page.mouse.move(5, 5);
  await shot(page, "W0.5-data-log");
});

test("the data page looks as its baselines do, in both themes @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await open(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    for (const [name, file] of [
      ["A long log", "log"],
      ["Files", "files"],
      ["Pieces", "pieces"],
    ] as const) {
      await page.getByRole("tab", { name }).click();
      await page.mouse.move(5, 5);
      await expect(page.getByRole("tab", { name })).toHaveAttribute("aria-selected", "true");
      await expect(page).toHaveScreenshot(`data-${file}-${scheme}.png`);
      if (scheme === "dark" && file === "pieces") await shot(page, "W0.5-data-pieces-dark");
    }
  }
});
