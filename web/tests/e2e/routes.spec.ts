// Covers: web/src
// Every route and view of the app (W10.2): axe, and a name for a screen
// reader on every control, in both themes; and the baselines of the views
// whose phases took none, on fixed data.
import type { Locator, Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { expectAccessible, expectNamed } from "./axe.ts";
import { embossPackage } from "./extensions.ts";
import { expect, inScheme, test } from "./fixtures.ts";
import { imported, made } from "./records.ts";
import { counted } from "./reports.ts";
import { tiffOf } from "./tiff.ts";

/** A view: its address, and the tab within it to choose, by its name. */
interface View {
  name: string;
  path: string | ((url: string) => Promise<string>);
  tab?: string;
}

const settings = (view: string, name: string): View => ({
  name: `Settings: ${name}`,
  path: `#/settings?view=${view}`,
});
const workspace = (tab: string): View => ({ name: `Workspace: ${tab}`, path: "#/workspace", tab });

/** A project of one imported image, for the Gallery's pages: its ID and the record's. */
async function aProject(url: string): Promise<{ project: string; record: string }> {
  const project = await made(url, "Routes");
  const record = await imported(url, project, "a field.tif", tiffOf(512, 384));
  return { project, record };
}

const VIEWS: View[] = [
  { name: "Control", path: "#/control" },
  { name: "Scan: a plan", path: "#/scan" },
  { name: "Scan: a histo scan", path: "#/scan", tab: "Histo scan" },
  workspace("Recipe"),
  workspace("Analyses"),
  workspace("Annotations"),
  workspace("Surfaces"),
  workspace("Volumes"),
  workspace("Reports"),
  { name: "Gallery", path: "#/gallery" },
  {
    name: "Gallery: a project",
    path: async (url) => `#/gallery?project=${(await aProject(url)).project}`,
  },
  {
    name: "Gallery: a record deep-zoomed",
    path: async (url) => {
      const { project, record } = await aProject(url);
      return `#/gallery?project=${project}&record=${record}&view=1`;
    },
  },
  { name: "Jobs", path: "#/jobs" },
  settings("display", "the display"),
  settings("system", "the system"),
  settings("devices", "the devices"),
  settings("calibration", "calibration"),
  settings("simulation", "simulation"),
  settings("extensions", "extensions"),
  settings("accounts", "accounts"),
  settings("document", "the whole document"),
  { name: "Logging", path: "#/logging" },
  { name: "About", path: "#/about" },
  { name: "Power", path: "#/power" },
  { name: "The status in full", path: "#/status" },
];

/** `view` opened, its heading shown, and its tab chosen. */
async function opened(page: Page, url: string, view: View): Promise<void> {
  const path = typeof view.path === "string" ? view.path : await view.path(url);
  await page.goto(`${url}/${path}`);
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  if (view.tab) {
    await page.getByRole("tab", { name: view.tab, exact: true }).click();
    await expect(page.getByRole("tab", { name: view.tab, exact: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  }
}

for (const view of VIEWS) {
  test(`${view.name}: no violation axe can find, and every control named, in either theme @a11y`, async ({
    page,
    server,
  }) => {
    await opened(page, server.url, view);
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await expectNamed(page);
      await expectAccessible(page);
    }
  });
}

/** The page as its baseline `name` has it, in both themes, the pointer away. */
async function looksAsBefore(page: Page, name: string, mask: Locator[] = []): Promise<void> {
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`${name}-${scheme}.png`, { mask });
  }
}

test.describe("the views whose phases took no baseline", () => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");

  test("Settings' system looks as its baseline does @visual", async ({ page, server }) => {
    await opened(page, server.url, settings("system", "the system"));
    const pane = page.locator('[data-said="system-pane"]');
    await expect(pane.getByRole("textbox").first()).not.toHaveValue("");
    // The folders and the port are the test's own.
    await looksAsBefore(page, "settings-system", [pane.locator("input")]);
  });

  test("Settings' devices look as their baseline does @visual", async ({ page, server }) => {
    await opened(page, server.url, settings("devices", "the devices"));
    await expect(page.locator('[data-said="saved-objective"]')).toBeVisible();
    await looksAsBefore(page, "settings-devices");
  });

  test("Settings' whole document looks as its baseline does @visual", async ({ page, server }) => {
    await opened(page, server.url, settings("document", "the whole document"));
    await looksAsBefore(page, "settings-document");
  });

  test("the reports on a project's results look as their baseline does @visual", async ({
    page,
    server,
  }) => {
    await counted(server.url);
    await opened(page, server.url, workspace("Reports"));
    await page
      .locator('[data-said="reports"]')
      .getByRole("combobox", { name: "The project" })
      .click();
    await page.getByRole("option", { name: "Discs", exact: true }).click();
    await expect(
      page.locator('[data-said="make"]').getByRole("checkbox", { name: "Discs counted" }),
    ).toBeChecked();
    await looksAsBefore(page, "reports");
  });

  test("the calibration wizard's first step looks as its baseline does @visual", async ({
    page,
    server,
  }) => {
    await opened(page, server.url, settings("calibration", "calibration"));
    await page
      .locator('[data-said="wizard"]')
      .getByRole("button", { name: "Start the wizard" })
      .click();
    await expect(page.locator('[data-said="wizard-step"]')).toContainText("Step 1 of 6");
    await looksAsBefore(page, "wizard");
  });
});

test.describe("a server that restarts", () => {
  test.use({ restarts: true });

  test("an extension's panel: no violation axe can find, every control named, and its baseline @a11y @visual", async ({
    page,
    server,
  }) => {
    test.setTimeout(300_000);
    const file = embossPackage(test.info().outputPath());
    await page.goto(`${server.url}/#/settings?view=extensions`);
    const choosing = page.waitForEvent("filechooser");
    await page.getByRole("button", { name: "Install a package…" }).click();
    await (await choosing).setFiles(file);
    await expect(page.locator('[data-said="extensions-said"]')).toContainText("is installed", {
      timeout: 30_000,
    });
    await act(
      "/api/v1/power/restart",
      { confirm: true, force: false },
      { on: new Api(server.url) },
    ).catch(() => undefined);
    await expect.poll(() => server.restarts, { timeout: 60_000 }).toBe(1);
    const answers = () =>
      page.request.get(`${server.url}/health`).then(
        (reply) => reply.ok(),
        () => false,
      );
    await expect.poll(answers, { timeout: 60_000 }).toBe(true);
    await page.goto(
      `${server.url}/#/settings/extensions/org.oscope.examples.emboss/example.emboss`,
    );
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Emboss the live view");
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await expectNamed(page);
      await expectAccessible(page);
    }
    if (process.env.OSCOPE_VISUAL) await looksAsBefore(page, "panel");
  });
});
