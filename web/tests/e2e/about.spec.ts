// Covers: web/src/features/about, web/src/features/status
// About and the status in full (W4.3): the server's computer and its time
// up; its notices, listed and read, and the app's own, made at build; the
// status's page from the rail's indicator, each subsystem's details, and a
// fault injected and cleared, which the indicator shows too; axe; and the
// baselines.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { homed } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const subsystem = (page: Page, name: string) => page.locator(`[data-subsystem="${name}"]`);

/** The server's notices, as its package puts them: in the test's server's `notices`. */
function notices(folder: string): void {
  const into = join(folder, "notices");
  mkdirSync(into, { recursive: true });
  writeFileSync(
    join(into, "THIRD-PARTY-NOTICES.html"),
    "<h1>Third-party notices</h1><p>A crate, and its licence.</p>",
  );
  writeFileSync(join(into, "Roboto-LICENSE.txt"), "The fonts' licence, as the test puts it.");
}

test("About says the server's computer and its time up, and lists both licences @main", async ({
  page,
  server,
}) => {
  notices(server.folder);
  await page.goto(`${server.url}/#/about`);
  const system = said(page, "system");
  await expect(system).toContainText("Operating system");
  await expect(system).toContainText(/Processor.*\d+ cores/s);
  await expect(system).toContainText(/Up for\s*\d+ (s|min)/);
  // The server's notices, listed; the HTML read in a frame that runs nothing.
  const listed = said(page, "server-notices");
  await expect(listed.getByRole("listitem")).toHaveText([
    /Roboto-LICENSE\.txt/,
    /THIRD-PARTY-NOTICES\.html/,
  ]);
  await listed.getByRole("button", { name: "Read THIRD-PARTY-NOTICES.html" }).click();
  const html = page.getByRole("dialog", { name: "THIRD-PARTY-NOTICES.html" });
  const frame = html.locator("iframe").contentFrame();
  await expect(frame.getByRole("heading", { name: "Third-party notices" })).toBeVisible();
  // Its box has the focus, not the frame: Escape closes the dialog, in every engine.
  await expect(html.getByRole("region", { name: "THIRD-PARTY-NOTICES.html" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(html).toBeHidden();
  await listed.getByRole("button", { name: "Read Roboto-LICENSE.txt" }).click();
  const text = page.getByRole("dialog", { name: "Roboto-LICENSE.txt" });
  await expect(text).toContainText("The fonts' licence, as the test puts it.");
  await expect(text.getByRole("region", { name: "Roboto-LICENSE.txt" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(text).toBeHidden();
  // The app's own: the packages its bundle holds, Vue's runtime among them, with its licence's text.
  const own = said(page, "app-notices");
  await own.getByText(/^@vue\/runtime-core \d+\.\d+\.\d+ · MIT$/).click();
  await expect(own).toContainText("Permission is hereby granted");
});

test("the status's page shows each subsystem's details, and a fault injected and cleared @main", async ({
  page,
  server,
}) => {
  // The stage homed: the instrument is well, until a fault is injected.
  await homed(server.url);
  await page.goto(`${server.url}/#/control`);
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  // From the rail's indicator to the status in full.
  await said(page, "indicator").click();
  await said(page, "status-page").click();
  await expect(
    page.getByRole("heading", { name: "The instrument's status", level: 1 }),
  ).toBeVisible();
  await expect(subsystem(page, "disks")).toContainText(/Free\s*\d+\.\d GB/);
  await expect(subsystem(page, "jobs")).toContainText(/Queued\s*0/);
  // The camera unplugged: its card, the instrument and the rail's indicator say so.
  const faults = said(page, "faults");
  await faults.getByRole("button", { name: "The camera unplugged: Inject" }).click();
  await expect(subsystem(page, "camera")).toHaveAttribute("data-state", /^(offline|fault)$/);
  await expect(said(page, "overall")).toContainText("Fault");
  await expect(said(page, "indicator")).toContainText("Fault");
  // Taken away: the camera, and the instrument, are well again.
  await faults.getByRole("button", { name: "The camera unplugged: Clear" }).click();
  await expect(subsystem(page, "camera")).toHaveAttribute("data-state", "ok");
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
});

test("About and the status's page have no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  notices(server.folder);
  for (const [path, ready] of [
    ["about", "app-notices"],
    ["status", "faults"],
  ] as const) {
    await page.goto(`${server.url}/#/${path}`);
    await expect(said(page, ready)).toBeVisible();
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await expectAccessible(page);
    }
  }
});

/** The page at `path` opened, the stage homed and the rail's indicator saying so, as the baselines have it. */
async function steady(page: Page, url: string, path: string): Promise<void> {
  await homed(url);
  await page.setViewportSize({ width: 1280, height: 1000 });
  await page.goto(`${url}/#/${path}`);
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
}

test("About looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  notices(server.folder);
  await steady(page, server.url, "about");
  await expect(said(page, "app-notices")).toBeVisible();
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    // The computer, its time up and the app's packages are the moment's.
    await expect(page).toHaveScreenshot(`about-${scheme}.png`, {
      mask: [
        said(page, "system").locator("dd"),
        said(page, "app-count"),
        said(page, "app-notices"),
        page.getByRole("main").getByRole("status"),
      ],
    });
    await shot(page, `W4.3-about-${scheme}`);
  }
});

/** A status that holds still, all well: a computer's own GPU and disk would show in the baseline. */
const STEADY = {
  state: "ok",
  message: "all well",
  subsystems: [
    { subsystem: "camera", state: "ok", message: "streaming", details: null },
    { subsystem: "stage", state: "ok", message: "idle", details: null },
    { subsystem: "lights", state: "ok", message: "1 of 6 on", details: null },
    {
      subsystem: "gpu",
      state: "ok",
      message: "a GPU, 48 °C, 812 of 8192 MiB",
      details: { name: "a GPU", memory_used_mib: 812, memory_total_mib: 8192, temperature_c: 48 },
    },
    {
      subsystem: "disks",
      state: "ok",
      message: "the scratch disk: 412 GB free, filling at 0 MB/s",
      details: { available_gb: 412.3, total_gb: 952.8, filling_mb_s: 0, recordings: 0 },
    },
    {
      subsystem: "jobs",
      state: "ok",
      message: "0 running, 0 queued",
      details: { running: 0, queued: 0, failed_recently: 0 },
    },
    { subsystem: "extensions", state: "ok", message: "0 loaded", details: null },
  ],
};

test("the status's page looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await page.route("**/api/v1/status/instrument", (route) => route.fulfill({ json: STEADY }));
  await page.setViewportSize({ width: 1280, height: 1000 });
  await page.goto(`${server.url}/#/status`);
  await expect(subsystem(page, "disks")).toContainText("412.3 GB");
  await expect(said(page, "indicator")).toHaveAccessibleName("The instrument: OK");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`status-page-${scheme}.png`);
    await shot(page, `W4.3-status-${scheme}`);
  }
});
