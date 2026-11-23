// Covers: web/src/features/dev
import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

const HISTOGRAM = "The histogram of each frame of a burst, ten a second";
const PROFILE = "A profile across the frame's middle, with the spread of the rows around it";

/** The page of charts, and a chart's plot, readout and button by the chart's title. */
async function open(page: Page, url: string) {
  await page.goto(`${url}/#/dev/gallery/charts`);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  const figure = (title: string) => page.locator("figure").filter({ hasText: title });
  return {
    figure,
    plot: (title: string) =>
      page.getByRole("img", { name: new RegExp(`^${title.replace(/[()]/g, "\\$&")}\\.`) }),
    readout: (title: string) => figure(title).locator("output"),
    showAll: (title: string) => figure(title).getByRole("button", { name: "Show all" }),
  };
}

test("a chart is read, zoomed and panned from the keyboard @main", async ({ page, server }) => {
  const { plot, readout, showAll } = await open(page, server.url);
  const keys = page.keyboard;
  await plot(PROFILE).focus();
  await keys.press("Home");
  await expect(readout(PROFILE)).toHaveText(/^Position \(µm\) 0\.0; Mean of 16 rows \d+(\.\d)?$/);
  await keys.press("ArrowRight");
  await expect(readout(PROFILE)).toHaveText(/^Position \(µm\) 0\.[1-9]; /);
  // Zoomed in, the view follows the cursor; 0 shows it all again.
  await keys.press("+");
  await expect(showAll(PROFILE)).toBeVisible();
  await keys.press("End");
  await expect(readout(PROFILE)).toHaveText(/^Position \(µm\) \d{3}\.\d; /);
  await keys.press("PageUp");
  await keys.press("0");
  await expect(showAll(PROFILE)).toHaveCount(0);

  // The live histogram, paused from its toolbar, holds its frame.
  const pause = page.getByRole("button", { name: "Pause" });
  await pause.focus();
  await keys.press("Enter");
  await expect(pause).toHaveAttribute("aria-pressed", "true");
  await plot(HISTOGRAM).focus();
  await keys.press("Home");
  const held = await readout(HISTOGRAM).textContent();
  await expect(readout(HISTOGRAM)).toHaveText(
    /^Value \(ADU\) from 0\.0 to 16\.0; Frame \d+ \d+(\.\d)?$/,
  );
  await expect(readout(HISTOGRAM)).toHaveText(held!);
});

test("a chart zooms with the wheel, pans with a drag, and shows everything again on a double press", async ({
  page,
  server,
}) => {
  const { plot, readout, showAll } = await open(page, server.url);
  await plot(PROFILE).scrollIntoViewIfNeeded();
  const box = (await plot(PROFILE).boundingBox())!;
  const middle = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await page.mouse.move(middle.x, middle.y);
  await expect(readout(PROFILE)).toHaveText(/^Position \(µm\) /);
  const before = await readout(PROFILE).textContent();
  await page.mouse.wheel(0, -300);
  await expect(showAll(PROFILE)).toBeVisible();
  // Zoomed about the pointer: the value under it stays under it.
  await page.mouse.move(middle.x + 1, middle.y);
  await page.mouse.move(middle.x, middle.y);
  await expect(readout(PROFILE)).toHaveText(before!);
  // Dragged, the view moves: another value is under the pointer.
  await page.mouse.down();
  await page.mouse.move(middle.x + 120, middle.y, { steps: 5 });
  await page.mouse.up();
  await page.mouse.move(middle.x, middle.y);
  await expect(readout(PROFILE)).not.toHaveText(before!);
  await plot(PROFILE).dblclick({ position: { x: box.width / 2, y: box.height / 2 } });
  await expect(showAll(PROFILE)).toHaveCount(0);

  // The cursor goes to a point of the data: here, a bin of 16 values, from its lower edge.
  await plot(HISTOGRAM).scrollIntoViewIfNeeded();
  const bins = (await plot(HISTOGRAM).boundingBox())!;
  await page.mouse.move(bins.x + bins.width * 0.437, bins.y + bins.height / 2);
  await expect(readout(HISTOGRAM)).toHaveText(/^Value \(ADU\) from \d+\.0 to \d+\.0; /);
  const [from, to] = (await readout(HISTOGRAM).textContent())!.match(/\d+(?=\.0)/g)!.map(Number);
  expect(from! % 16).toBe(0);
  expect(to! - from!).toBe(16);
});

test("the live histogram is drawn ten times a second, each time in a few milliseconds", async ({
  page,
  server,
}) => {
  await open(page, server.url);
  const draws = () =>
    page.evaluate(
      (title) =>
        performance
          .getEntriesByName("oscope.chart.draw")
          .filter((entry) => (entry as PerformanceMeasure).detail === title)
          .map((entry) => ({ at: entry.startTime, took: entry.duration })),
      HISTOGRAM,
    );
  await expect
    .poll(async () => (await draws()).length, { timeout: 10_000 })
    .toBeGreaterThanOrEqual(31);
  const measured = (await draws()).slice(1);
  const took = measured.map((one) => one.took).sort((a, b) => a - b);
  const median = took[Math.floor(took.length / 2)]!;
  const perSecond = ((measured.length - 1) * 1000) / (measured.at(-1)!.at - measured[0]!.at);
  console.log(
    `the histogram of 4,096 bins: ${measured.length} draws, ${perSecond.toFixed(1)} a second; ` +
      `median ${median.toFixed(2)} ms, longest ${took.at(-1)!.toFixed(2)} ms`,
  );
  expect(perSecond).toBeGreaterThan(8);
  expect(median).toBeLessThan(4);
});

test("each chart's numbers are in a table behind it", async ({ page, server }) => {
  const { figure } = await open(page, server.url);
  const numbers = async (title: string) => {
    await figure(title).getByRole("button", { name: "The numbers" }).click();
    return figure(title).getByRole("grid");
  };
  const histogram = await numbers(HISTOGRAM);
  await expect(histogram).toHaveAttribute("aria-rowcount", "4097");
  await expect(histogram.getByRole("columnheader")).toHaveText(["From", "To", /^Frame \d+$/]);
  const profile = await numbers(PROFILE);
  await expect(profile.getByRole("columnheader")).toHaveText([
    "Position (µm)",
    "Mean of 16 rows",
    "Mean of 16 rows, low",
    "Mean of 16 rows, high",
  ]);
});

test("the charts pass axe, in both themes", async ({ page, server }) => {
  await open(page, server.url);
  await page.getByRole("button", { name: "The numbers" }).first().click();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the charts page looks as its baselines do, in both themes @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  // The page's clock is held, and moved on only for a frame to be drawn after each change: the
  // live histogram, moved on every 100 ms, stays on its first frame.
  await page.clock.install({ time: new Date("2026-11-18T09:00:00Z") });
  await page.clock.pauseAt(new Date("2026-11-18T09:00:01Z"));
  await open(page, server.url);
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await page.clock.runFor(20);
    await expect(page).toHaveScreenshot(`charts-${scheme}.png`, { fullPage: true });
    if (scheme === "dark") {
      // The lower two, in the notes' second picture.
      await page
        .getByRole("heading", { name: "A line profile" })
        .evaluate((heading) => heading.closest("section")!.scrollIntoView({ block: "start" }));
      await page.mouse.move(5, 5);
    }
    await shot(page, `W0.7-charts-${scheme}`);
  }
});

test.describe("on a screen of twice the pixels", () => {
  test.use({ deviceScaleFactor: 2 });

  test("a chart's canvas has the screen's own pixels, and draws in them", async ({
    page,
    server,
  }) => {
    await open(page, server.url);
    const canvas = page.locator("canvas").first();
    const size = () =>
      canvas.evaluate((element: HTMLCanvasElement) => [
        element.width,
        element.clientWidth,
        element.height,
        element.clientHeight,
      ]);
    await expect.poll(async () => (await size())[0]).toBeGreaterThan(0);
    const [width, cssWidth, height, cssHeight] = await size();
    expect(width).toBe(Math.round(cssWidth! * 2));
    expect(height).toBe(Math.round(cssHeight! * 2));
  });
});
