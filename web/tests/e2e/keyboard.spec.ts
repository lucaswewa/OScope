// Covers: web/src
// The main workflows by the keyboard alone (W10.2): each control reached by
// Tab, in the page's order, and worked by its keys, never the mouse. A frame
// captured into a project made on Control; a scan planned and run; a record
// found in the Gallery and viewed; and an analysis run and reported on.
import type { Locator, Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { saveRecipe } from "../../src/api/pipeline.ts";
import { counting, prepared } from "./analyses.ts";
import { expect, test } from "./fixtures.ts";
import { imported, made } from "./records.ts";
import { clockAt, FAST, TINY } from "./sim.ts";
import { tiffOf } from "./tiff.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

/** Whether `target`, or what it holds, has the focus. */
const focused = (target: Locator) =>
  target.evaluate((el) => el === document.activeElement || el.contains(document.activeElement));

/** What has the focus, as a number of its own, or -1 for none: to tell a walk that has stopped. */
const focusNow = (page: Page) =>
  page.evaluate(() => {
    const kept = window as unknown as { focusIds?: WeakMap<Element, number>; focusCount?: number };
    const now = document.hasFocus() ? document.activeElement : null;
    if (!now) return -1;
    kept.focusIds ??= new WeakMap();
    if (!kept.focusIds.has(now))
      kept.focusIds.set(now, (kept.focusCount = (kept.focusCount ?? 0) + 1));
    return kept.focusIds.get(now)!;
  });

/**
 * Tab until `target` has the focus: it is reached by the keyboard, in order.
 * Forwards, then backwards by Shift and Tab once the focus stops moving:
 * Firefox takes the focus out to its own window past the page's last control.
 */
async function tabTo(page: Page, target: Locator, most = 300): Promise<void> {
  await expect(target).toBeVisible();
  for (const key of ["Tab", "Shift+Tab"]) {
    let still = 0;
    let was = await focusNow(page);
    for (let n = 0; n < most && still < 5; n++) {
      if (await focused(target)) return;
      await page.keyboard.press(key);
      const now = await focusNow(page);
      still = now === was ? still + 1 : 0;
      was = now;
    }
  }
  if (!(await focused(target))) {
    throw new Error(`not reached by Tab in ${most} presses: ${target.toString()}`);
  }
}

/** A control reached by Tab, and pressed by Enter. */
async function press(page: Page, target: Locator): Promise<void> {
  await tabTo(page, target);
  await page.keyboard.press("Enter");
}

/** A field reached by Tab, its value replaced by typing. */
async function type(page: Page, field: Locator, value: string): Promise<void> {
  await tabTo(page, field);
  await page.keyboard.press("ControlOrMeta+A");
  await page.keyboard.type(value);
  await page.keyboard.press("Enter");
}

/**
 * A tab chosen by its keys: its list is one stop of Tab, at the tab chosen,
 * and the arrows move along it, as the ARIA pattern has it.
 */
async function chooseTab(page: Page, name: string): Promise<void> {
  const wanted = page.getByRole("tab", { name, exact: true });
  const list = page.getByRole("tablist").filter({ has: wanted });
  await tabTo(page, list.getByRole("tab", { selected: true }));
  for (let n = 0; n < 20 && !(await focused(wanted)); n++) await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  await expect(wanted).toHaveAttribute("aria-selected", "true");
}

/** One of a list's options chosen by its keys: opened by Enter, moved to by the arrows. */
async function choose(page: Page, list: Locator, option: string): Promise<void> {
  await tabTo(page, list);
  await page.keyboard.press("Enter");
  const wanted = page.getByRole("option", { name: option, exact: true });
  await expect(wanted).toBeVisible();
  for (let n = 0; n < 50 && !(await wanted.getAttribute("data-highlighted")); n++) {
    await page.keyboard.press("ArrowDown");
  }
  await page.keyboard.press("Enter");
  await expect(list).toContainText(option);
}

test("a frame captured into a project made on Control, by the keyboard alone @main", async ({
  page,
  server,
}) => {
  test.setTimeout(180_000);
  await page.goto(`${server.url}/#/control`);
  const stage = said(page, "stage");
  const field = (axis: string) => stage.getByRole("spinbutton", { name: new RegExp(`^${axis}`) });
  // Homed, and over the tissue, on the fast clock.
  await clockAt(server.url, FAST);
  try {
    await press(page, stage.getByRole("button", { name: "Home", exact: true }));
    await expect(said(page, "stage-state")).toHaveText("Idle", { timeout: 60_000 });
    await type(page, field("X"), "40000");
    await type(page, field("Y"), "12000");
    await type(page, field("Z"), "5000");
    await press(page, stage.getByRole("button", { name: "Move", exact: true }));
    // Within the 3 µm the simulator's backlash may leave.
    const off = async () => {
      const at = (
        await call((signal) => new Api(server.url).api.GET("/api/v1/stage/position", { signal }))
      ).position;
      return Math.max(Math.abs(at.x - 40_000), Math.abs(at.y - 12_000));
    };
    await expect.poll(off, { timeout: 60_000 }).toBeLessThanOrEqual(3);
    await expect(said(page, "stage-state")).toHaveText("Idle", { timeout: 60_000 });
  } finally {
    await clockAt(server.url, 1);
  }
  // Streaming through the camera's tiny region: a debug server renders each frame.
  const on = new Api(server.url);
  await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: TINY, signal }));
  await press(page, page.getByRole("button", { name: "Start", exact: true }));
  await expect(said(page, "state").locator("dd")).toHaveText("Live", { timeout: 60_000 });
  // A project made in the capture pane, and a frame captured into it.
  const capture = said(page, "capture");
  await type(page, capture.getByRole("textbox", { name: "New project" }), "By the keys");
  await expect(capture.getByRole("combobox", { name: "Project" })).toContainText("By the keys");
  await press(page, said(page, "capture-run"));
  await expect(said(page, "captured")).toContainText("Kept in By the keys", { timeout: 60_000 });
});

test("a record found in the Gallery and viewed, by the keyboard alone @main", async ({
  page,
  server,
  browserName,
}) => {
  // Safari passes links by Tab unless Option is held, and Playwright's WebKit doesn't take it.
  test.skip(browserName === "webkit", "WebKit, as Safari, passes the record's link by Tab");
  const project = await made(server.url, "Found by the keys");
  await imported(server.url, project, "a field.tif", tiffOf(512, 384));
  await made(server.url, "Another");
  await page.goto(`${server.url}/#/gallery`);
  await type(page, page.getByRole("textbox", { name: "Find" }), "keys");
  await expect(said(page, "card")).toHaveCount(1);
  // The card itself, a stop of Tab: Enter opens it.
  await press(page, said(page, "grid").getByRole("listitem").first());
  await press(page, said(page, "records-acquisition").getByRole("button").first());
  await expect(said(page, "details")).toContainText("512 × 384");
  await press(page, said(page, "view"));
  const tiles = said(page, "viewers").locator('[data-said="tiles"]').first();
  await expect(tiles).toHaveAttribute("data-sharp", "true", { timeout: 60_000 });
});

test("a scan planned and run, by the keyboard alone", async ({ page, server }) => {
  test.setTimeout(180_000);
  const on = new Api(server.url);
  await clockAt(server.url, FAST);
  try {
    await act("/api/v1/stage/home", {}, { on, timeout: 30_000 });
    await act("/api/v1/stage/move_to", { x: 40_000, y: 12_000, z: 5_000, speed: 25_000 }, { on });
  } finally {
    await clockAt(server.url, 1);
  }
  await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: TINY, signal }));
  await act("/api/v1/camera/start_streaming", {}, { on });
  await made(server.url, "Scanned by the keys");
  await page.goto(`${server.url}/#/scan`);
  const region = said(page, "plan-region");
  for (const [label, value] of [
    ["x", "39.85"],
    ["y", "11.9"],
    ["Width", "0.15"],
    ["Height", "0.1"],
  ] as const) {
    await type(page, region.getByLabel(label, { exact: true }), value);
  }
  await expect(said(page, "estimate")).toContainText("Tiles", { timeout: 30_000 });
  const run = said(page, "run");
  await choose(page, run.getByRole("combobox", { name: "Project" }), "Scanned by the keys");
  await press(page, run.getByRole("button", { name: "Run the scan" }));
  await expect(said(page, "scan-tiles")).toHaveText(/^Completed: (\d+) of \1 tiles$/, {
    timeout: 120_000,
  });
});

test("an analysis run and reported on, by the keyboard alone", async ({ page, server }) => {
  test.setTimeout(180_000);
  const on = new Api(server.url);
  await prepared(server.url);
  await saveRecipe("count-discs", counting(10), { on });
  await page.goto(`${server.url}/#/workspace`);
  await chooseTab(page, "Analyses");
  const run = said(page, "run");
  await choose(page, run.getByRole("combobox", { name: "The recipe" }), "Discs counted");
  await choose(page, run.getByRole("combobox", { name: "The project" }), "Discs");
  await expect(run.getByRole("combobox", { name: "The image" })).toContainText("discs");
  await press(page, run.getByRole("button", { name: "Run", exact: true }));
  await expect(said(page, "run-said")).toHaveText("Done: its result is listed.", {
    timeout: 60_000,
  });
  // The report on it, made and shown.
  await chooseTab(page, "Reports");
  await choose(page, said(page, "reports").getByRole("combobox", { name: "The project" }), "Discs");
  await press(page, said(page, "make").getByRole("button", { name: "Make it" }));
  await expect(said(page, "making")).toHaveText("Made: it is shown.", { timeout: 60_000 });
});
