// Covers: web/src
// Every dialog of the app (W10.2), opened on fixed data: axe, a name for a
// screen reader on every control in it, in both themes; and, with
// OSCOPE_VISUAL=1, its baseline: the dialog alone, as the page behind it is
// another baseline's.
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { finished } from "../../src/api/jobs.ts";
import { analyse } from "../../src/api/workspace.ts";
import { counting, prepared } from "./analyses.ts";
import { expectAccessible, expectNamed } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { imported, made } from "./records.ts";
import { Server } from "./server.ts";
import { overTissueTiny } from "./sim.ts";
import { tiffOf } from "./tiff.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const LAYER = '[role="dialog"], [role="alertdialog"]';
const HIDE_TOOLTIPS = fileURLToPath(new URL("hide-tooltips.css", import.meta.url));
/** A dialog's baseline: its tooltips hidden; its rounded corners blend with a page that moves. */
const AS_BEFORE = { stylePath: HIDE_TOOLTIPS, maxDiffPixels: 20 };

/** The dialog open, checked in both themes; `name` its baseline's. */
async function checked(page: Page, name: string): Promise<void> {
  const dialog = page.locator(LAYER);
  await expect(dialog).toHaveCount(1);
  await expect(dialog).toBeVisible();
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectNamed(page, LAYER);
    await expectAccessible(page, LAYER);
    if (process.env.OSCOPE_VISUAL) {
      await expect(dialog).toHaveScreenshot(`dialog-${name}-${scheme}.png`, AS_BEFORE);
    }
  }
}

/** A button on the page, by its name. */
const button = (page: Page, name: string | RegExp) =>
  page.getByRole("button", { name, exact: typeof name === "string" });

test("the keyboard's shortcuts @a11y @visual", async ({ page, server }) => {
  await page.goto(`${server.url}/#/control`);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await page.keyboard.press("?");
  await checked(page, "shortcuts");
});

test("a notice of the server's, read @a11y @visual", async ({ page }) => {
  const server = new Server();
  try {
    mkdirSync(join(server.folder, "notices"), { recursive: true });
    writeFileSync(
      join(server.folder, "notices", "THIRD-PARTY-NOTICES.txt"),
      "A crate, and its licence: MIT.\n",
    );
    await server.start();
    await page.goto(`${server.url}/#/about`);
    await said(page, "server-notices").getByRole("button").first().click();
    await checked(page, "notice");
  } finally {
    await server.dispose();
  }
});

test("a new project, and one deleted @a11y @visual", async ({ page, server }) => {
  await made(server.url, "Dialogs");
  await page.goto(`${server.url}/#/gallery`);
  await button(page, "New project").first().click();
  await checked(page, "project-new");
  await page.keyboard.press("Escape");
  const card = said(page, "card").filter({ hasText: "Dialogs" });
  await card.getByRole("button", { name: "Change Dialogs" }).click();
  await page.getByRole("menuitem", { name: "Delete…" }).click();
  await checked(page, "project-delete");
});

test("a record deleted @a11y @visual", async ({ page, server }) => {
  const project = await made(server.url, "Dialogs");
  const record = await imported(server.url, project, "a field.tif", tiffOf(64, 48));
  await page.goto(`${server.url}/#/gallery?project=${project}&record=${record}`);
  await button(page, "Delete this record…").click();
  await checked(page, "record-delete");
});

test("a running job cancelled @a11y @visual", async ({ page, server }) => {
  await overTissueTiny(server.url);
  const on = new Api(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const project = (await act("/api/v1/projects/create", { name: "Dialogs" }, { on })).id;
  const burst = { project, frames: 5_000, name: "a long burst" };
  const job = await act("/api/v1/jobs/submit", { kind: "capture.burst", params: burst }, { on });
  await page.goto(`${server.url}/#/jobs?job=${job.id}`);
  await expect(said(page, "job")).toContainText("Running", { timeout: 30_000 });
  await said(page, "job").getByRole("button", { name: "Cancel the job" }).click();
  await checked(page, "job-cancel");
  await act("/api/v1/jobs/cancel", { id: job.id }, { on });
});

test("a scan cancelled @a11y @visual", async ({ page, server }) => {
  await overTissueTiny(server.url);
  const on = new Api(server.url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  const project = (await act("/api/v1/projects/create", { name: "Dialogs" }, { on })).id;
  const region = { rect: { origin: { x: 39_850, y: 11_900 }, size: { width: 600, height: 400 } } };
  const request = { pattern: "snake", region, positions: [] };
  const scan = await act("/api/v1/scan/run", { request, project, name: "a scan" }, { on });
  await page.goto(`${server.url}/#/scan`);
  await said(page, "scan").getByRole("button", { name: "Cancel" }).click({ timeout: 30_000 });
  await checked(page, "scan-cancel");
  await act("/api/v1/jobs/cancel", { id: scan.id }, { on });
});

test("the live mosaic cleared, and started anew @a11y @visual", async ({ page, server }) => {
  await overTissueTiny(server.url);
  await page.goto(`${server.url}/#/control`);
  await button(page, "Start").click();
  await button(page, "Mosaic").click();
  await button(page, "Start the mosaic").click();
  await expect(said(page, "mosaic-said")).toContainText(/Drawing: [1-9]/, { timeout: 30_000 });
  await button(page, "Clear").click();
  await checked(page, "mosaic-clear");
  await page.keyboard.press("Escape");
  await button(page, "Stop drawing").click();
  await expect(said(page, "mosaic-said")).toContainText("Stopped");
  await button(page, "Start the mosaic").click();
  await checked(page, "mosaic-again");
});

test("the service restarted, its name to type @a11y @visual", async ({ page, server }) => {
  await page.goto(`${server.url}/#/power`);
  await page.locator('[data-action="restart"]').getByRole("button", { name: "Restart…" }).click();
  await checked(page, "power-restart");
});

test("an account removed @a11y @visual", async ({ page, server }) => {
  const account = { name: "ana", password: "ana's password" };
  await act("/api/v1/accounts/add", { account }, { on: new Api(server.url) });
  await page.goto(`${server.url}/#/settings?view=accounts`);
  await page.locator('[data-account="ana"]').getByRole("button", { name: "Remove" }).click();
  await checked(page, "account-remove");
});

test("a result deleted @a11y @visual", async ({ page, server }) => {
  const on = new Api(server.url);
  const { project, acquisition } = await prepared(server.url);
  const input = { kind: "acquisition" as const, id: acquisition };
  await finished(await analyse(project, { recipe: counting(10), input }, { on }), {
    on,
    timeout: 60_000,
  });
  await page.goto(`${server.url}/#/workspace`);
  await page.getByRole("tab", { name: "Analyses" }).click();
  await said(page, "run").getByRole("combobox", { name: "The project" }).click();
  await page.getByRole("option", { name: "Discs", exact: true }).click();
  await said(page, "results").getByRole("button", { name: "Delete Discs counted" }).click();
  await checked(page, "result-delete");
});

test("the recipes' library: opened, saved as, one deleted, and changes left @a11y @visual", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/workspace`);
  const tool = (name: string) => said(page, "editor").getByRole("button", { name, exact: true });
  await expect(
    said(page, "palette").getByRole("button", { name: "Add filter.gaussian" }),
  ).toBeVisible();
  // Saved as, with a name of its own.
  await tool("Save as…").click();
  await checked(page, "recipe-save");
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("textbox", { name: "Name" }).fill("mine");
  await dialog.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "editor-said")).toHaveText("Saved as mine.");
  // The library, and one of the user's deleted.
  await tool("Open…").click();
  await checked(page, "recipe-open");
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W10.2-dialog-${scheme}`);
  }
  await dialog.getByRole("button", { name: "Delete mine" }).click();
  await expect(page.locator(LAYER)).toHaveCount(2);
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectNamed(page, '[role="alertdialog"]');
    await expectAccessible(page, '[role="alertdialog"]');
    if (process.env.OSCOPE_VISUAL) {
      await expect(page.getByRole("alertdialog")).toHaveScreenshot(
        `dialog-recipe-delete-${scheme}.png`,
        AS_BEFORE,
      );
    }
  }
  await page.getByRole("alertdialog").getByRole("button", { name: "Cancel" }).click();
  await page.keyboard.press("Escape");
  await expect(page.locator(LAYER)).toHaveCount(0);
  // Changes left for another recipe: asked first.
  await said(page, "palette").getByRole("button", { name: "Add threshold.otsu" }).click();
  await tool("New").click();
  await checked(page, "recipe-leave");
});

test("a report's template deleted @a11y @visual", async ({ page, server }) => {
  await page.goto(`${server.url}/#/workspace`);
  await page.getByRole("tab", { name: "Reports" }).click();
  await said(page, "reports").getByRole("tab", { name: "Templates" }).click();
  const templates = said(page, "templates");
  await templates.getByRole("textbox", { name: "Saved as" }).fill("ours");
  await templates.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "templates-said")).toHaveText("Saved as ours.");
  await templates.getByRole("button", { name: "Delete" }).click();
  await checked(page, "template-delete");
});
