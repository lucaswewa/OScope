// Covers: web/src/features/workspace
// Annotations and measurements (W7.5; the behaviour spec in the phase's
// notes): each shape drawn, measured as the server measures it, and still
// there after a reload; the CSV's numbers the table's; a shape selected,
// moved, reshaped, named and deleted; the keyboard's path; axe; and each
// shape's baseline, selected and not.
import { readFile } from "node:fs/promises";

import type { Page } from "@playwright/test";

import { prepared } from "./analyses.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const rows = (page: Page) => said(page, "measured").locator("tbody tr");
const cells = (page: Page, row: number) => rows(page).nth(row).locator("th, td");

/** The Annotations tab, the discs' image shown. */
async function opened(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/workspace`);
  await page.getByRole("tab", { name: "Annotations" }).click();
  await said(page, "annotate").getByRole("combobox", { name: "The project" }).click();
  await page.getByRole("option", { name: "Discs", exact: true }).click();
  await expect(said(page, "tile-scale")).toBeVisible();
}

/** A tool chosen: what a press draws, or none. */
async function tool(page: Page, name: string): Promise<void> {
  const tools = page.getByRole("toolbar", { name: "What a press draws" });
  await tools.getByRole("button", { name, exact: true }).click();
  await expect(tools.getByRole("button", { name, exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
}

/** A place on the image, as parts of the viewer across and down. */
async function at(page: Page, fx: number, fy: number): Promise<{ x: number; y: number }> {
  const box = (await said(page, "tiles").boundingBox())!;
  return { x: box.x + box.width * fx, y: box.y + box.height * fy };
}
async function presses(page: Page, ...places: [number, number][]): Promise<void> {
  for (const [fx, fy] of places) {
    const { x, y } = await at(page, fx, fy);
    await page.mouse.click(x, y);
  }
}
async function drag(page: Page, from: [number, number], to: [number, number]): Promise<void> {
  const [a, b] = [await at(page, ...from), await at(page, ...to)];
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 6 });
  await page.mouse.up();
}

/** Each of the five shapes drawn, apart. */
async function allFive(page: Page): Promise<void> {
  await tool(page, "Rectangle");
  await drag(page, [0.2, 0.2], [0.35, 0.4]);
  await tool(page, "Ellipse");
  await drag(page, [0.4, 0.6], [0.55, 0.85]);
  await tool(page, "Polygon");
  await presses(page, [0.5, 0.15], [0.7, 0.2], [0.65, 0.45]);
  // Through the pointer, its area as it would close.
  const fourth = await at(page, 0.5, 0.4);
  await page.mouse.move(fourth.x, fourth.y);
  await expect(said(page, "measuring")).toHaveText(/^Area [\d.]+ px², perimeter [\d.]+ px\.$/);
  await presses(page, [0.5, 0.4], [0.5, 0.15]);
  await tool(page, "Line");
  await presses(page, [0.15, 0.7], [0.35, 0.9], [0.35, 0.9]);
  await tool(page, "Point");
  await presses(page, [0.8, 0.7]);
  await expect(rows(page)).toHaveCount(5);
}

test("shapes drawn are measured as the server does, survive a reload, and export the table's numbers @main", async ({
  page,
  server,
}) => {
  await prepared(server.url);
  await opened(page, server.url);
  await allFive(page);
  // Each selected: the page's measure, the server's.
  await tool(page, "Select");
  for (const row of [0, 1, 2]) {
    await rows(page).nth(row).click();
    const [length, area] = [
      await cells(page, row).nth(2).innerText(),
      await cells(page, row).nth(3).innerText(),
    ];
    await expect(said(page, "measuring")).toHaveText(`Area ${area} px², perimeter ${length} px.`);
  }
  const table = await Promise.all(
    [0, 1, 2, 3, 4].map(async (row) => (await cells(page, row).allInnerTexts()).slice(2)),
  );
  // Reloaded: each still there, drawn and measured.
  await page.reload();
  await opened(page, server.url);
  await expect(rows(page)).toHaveCount(5);
  await expect(said(page, "annotations").locator("[data-shape]")).toHaveCount(5);
  // The CSV's numbers, to two decimals, the table's.
  const saving = page.waitForEvent("download");
  await said(page, "measured").getByRole("link", { name: "Export as CSV" }).click();
  const lines = (await readFile(await (await saving).path(), "utf-8")).trim().split("\r\n");
  // Its measures first; then a polygon's pixels' statistics.
  expect(lines[0]).toMatch(
    /^layer,id,name,geometry,length_px,area_px2,centroid_x_px,centroid_y_px,/,
  );
  const two = (value: string) => Number(value).toFixed(2);
  const exported = lines.slice(1).map((line) => {
    const [, , , , length, area, x, y] = line.split(",") as string[];
    return [two(length!), two(area!), `${two(x!)}, ${two(y!)}`];
  });
  expect(exported).toEqual(table);
});

test("a shape is selected, moved, reshaped, named and deleted", async ({ page, server }) => {
  await prepared(server.url);
  await opened(page, server.url);
  await tool(page, "Rectangle");
  await drag(page, [0.2, 0.2], [0.4, 0.5]);
  await expect(rows(page)).toHaveCount(1);
  await tool(page, "Select");
  await presses(page, [0.3, 0.35]);
  await expect(said(page, "annotations").locator("[data-handle]")).toHaveCount(4);
  const before = await cells(page, 0).allInnerTexts();
  // Moved: its centre, and not its area.
  await drag(page, [0.3, 0.35], [0.45, 0.35]);
  await expect(cells(page, 0).nth(4)).not.toHaveText(before[4]!);
  await expect(cells(page, 0).nth(3)).toHaveText(before[3]!);
  // Reshaped by its bottom right corner: larger.
  const corner = said(page, "annotations").locator('[data-handle="2"]');
  const box = (await corner.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + 80, box.y + 60, { steps: 6 });
  await page.mouse.up();
  await expect
    .poll(async () => Number(await cells(page, 0).nth(3).innerText()))
    .toBeGreaterThan(Number(before[3]));
  // Named, then deleted.
  const name = said(page, "annotate").getByRole("textbox", { name: "Name" });
  await name.fill("nucleus");
  await name.press("Enter");
  await expect(cells(page, 0).nth(1)).toHaveText("nucleus");
  await said(page, "annotate").getByRole("button", { name: "Delete the shape" }).click();
  await expect(rows(page)).toHaveCount(0);
});

test("from the keyboard, a shape is placed and adjusted with the arrows @main", async ({
  page,
  server,
}) => {
  await prepared(server.url);
  await opened(page, server.url);
  const canvas = said(page, "tiles").locator('.openseadragon-canvas[tabindex="0"]');
  // A rectangle: a corner at the view's middle, then the opposite, 50 by 30 screen pixels away.
  await tool(page, "Rectangle");
  await canvas.focus();
  await page.keyboard.press("Enter");
  for (let i = 0; i < 5; i++) await page.keyboard.press("Shift+ArrowRight");
  for (let i = 0; i < 3; i++) await page.keyboard.press("Shift+ArrowDown");
  await expect(said(page, "measuring")).toHaveText(/^Area /);
  await page.keyboard.press("Enter");
  await expect(rows(page)).toHaveCount(1);
  const [length, area] = (await cells(page, 0).allInnerTexts()).slice(2, 4).map(Number);
  // Its sides are 50 and 30 screen pixels, at whatever zoom: their ratio is the measures'.
  const side = length! / 160;
  expect(area).toBeCloseTo(1500 * side * side, 0);
  // Selected from the table, moved, then reshaped by its first corner.
  await tool(page, "Select");
  await rows(page).first().press("Enter");
  await canvas.focus();
  const centre = await cells(page, 0).nth(4).innerText();
  await page.keyboard.press("Shift+ArrowRight");
  await expect(cells(page, 0).nth(4)).not.toHaveText(centre);
  await page.keyboard.press("v");
  await expect(said(page, "annotations").locator('[data-handle="0"].chosen')).toHaveCount(1);
  await page.keyboard.press("Shift+ArrowRight");
  await expect
    .poll(async () => Number(await cells(page, 0).nth(3).innerText()))
    .toBeLessThan(area!);
  await page.keyboard.press("Delete");
  await expect(rows(page)).toHaveCount(0);
});

test("the annotations have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  await prepared(server.url);
  await opened(page, server.url);
  await allFive(page);
  await tool(page, "Select");
  await rows(page).nth(2).click();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("each shape looks as its baseline does, selected and not @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await prepared(server.url);
  await opened(page, server.url);
  await allFive(page);
  await tool(page, "Select");
  await rows(page).nth(2).click();
  await expect(said(page, "annotations").locator("[data-handle]")).toHaveCount(4);
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`annotate-${scheme}.png`);
    await shot(page, `W7.5-annotate-${scheme}`);
  }
});
