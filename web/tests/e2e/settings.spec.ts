// Covers: web/src/features/settings web/src/features/display
// Settings (W8.1, P52): a display preference following the account to a
// second browser, the stage's readout and the theme with it; a system change
// saved for the next start, the restart said, a wrong one refused; the whole
// document exported and imported back as it was; the devices' saved settings
// shown and forgotten; the account's scale bar and colour map in the Gallery's
// viewer; axe; and the display's baseline.
import { readFileSync } from "node:fs";

import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { imported, made } from "./records.ts";
import { tiffOf } from "./tiff.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);

/** The Settings tab, at `pane`. */
async function settings(page: Page, url: string, pane = "Display"): Promise<void> {
  await page.goto(`${url}/#/settings`);
  await page.getByRole("tab", { name: pane, exact: true }).click();
}

/** `option` chosen in the field `label`. */
async function choose(page: Page, label: string, option: string): Promise<void> {
  await page.getByRole("combobox", { name: label }).click();
  await page.getByRole("option", { name: option, exact: true }).click();
}

test("a display preference follows the account to a second browser, the readout and theme with it @main", async ({
  page,
  server,
  browser,
}) => {
  await settings(page, server.url);
  await choose(page, "Lengths in", "mm");
  await expect(said(page, "display-said")).toHaveText("Saved with your account.");
  // The second browser: the same account, the local one, with nothing of its own.
  const other = await browser.newContext();
  try {
    const second = await other.newPage();
    await second.goto(`${server.url}/#/control`);
    await expect(said(second, "position")).toContainText("mm");
    // Changed in the first, while the second is open: followed there.
    await choose(page, "Theme", "Dark");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
    await expect(second.locator("html")).toHaveAttribute("data-theme", "dark", { timeout: 15_000 });
    await choose(page, "Lengths in", "µm");
    await expect(said(second, "position")).toContainText("µm", { timeout: 15_000 });
    await second.goto(`${server.url}/#/settings`);
    await expect(second.getByRole("combobox", { name: "Theme" })).toHaveText("Dark");
    await expect(second.getByRole("combobox", { name: "Lengths in" })).toHaveText("µm");
  } finally {
    await other.close();
  }
  // Back to the defaults: as the computer is, again.
  await page.getByRole("button", { name: "Back to the defaults" }).click();
  await expect(said(page, "display-said")).toHaveText("Back to the defaults.");
  await expect(page.locator("html")).not.toHaveAttribute("data-theme", "dark", { timeout: 15_000 });
});

test("a system change is saved for the next start, the restart said; a wrong one is refused", async ({
  page,
  server,
}) => {
  await settings(page, server.url, "System");
  const port = page.getByRole("spinbutton", { name: "The port" });
  await expect(port).not.toHaveValue("");
  await expect(said(page, "restart-needed")).toHaveCount(0);
  await port.fill("8299");
  await port.press("Enter");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "system-said")).toHaveText("Saved: port.");
  await expect(said(page, "restart-needed")).toContainText("when it restarts", { timeout: 15_000 });
  const system = await (await page.request.get(`${server.url}/api/v1/settings/system`)).json();
  expect(system.port).toBe(8299);
  // An address on the network, without accounts: refused, saying why, and nothing written.
  await page.getByRole("textbox", { name: "The address it listens on" }).fill("0.0.0.0");
  await page.getByRole("button", { name: "Save" }).click();
  await expect(said(page, "system-said")).toContainText("needs accounts first");
  const kept = await (await page.request.get(`${server.url}/api/v1/settings/system`)).json();
  expect(kept.address).toBe("127.0.0.1");
  // The restart is offered on Power.
  await said(page, "restart-needed").getByRole("link", { name: "Restart it on Power" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Power");
});

test("the whole document exports to a file, and imports back as it was", async ({
  page,
  server,
}) => {
  // The account's own preferences first: the document holds them.
  const on = `${server.url}/api/v1/settings`;
  const defaults = await (await page.request.get(`${on}/display_defaults`)).json();
  await page.request.put(`${on}/display`, { data: { ...defaults, units: "mm" } });
  await settings(page, server.url, "The whole document");
  const downloading = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export to a file" }).click();
  const download = await downloading;
  expect(download.suggestedFilename()).toMatch(/^oscope-settings-\d{4}-\d{2}-\d{2}\.json$/);
  const file = await download.path();
  const exported = JSON.parse(readFileSync(file, "utf8"));
  expect(exported.format).toBe("oscope.settings");
  expect(exported.accounts.local.units).toBe("mm");
  // Changed since: the account's own, and the defaults.
  await page.request.put(`${on}/display`, { data: { ...defaults, units: "um", theme: "dark" } });
  await page.request.put(`${on}/display_defaults`, { data: { ...defaults, colormap: "magma" } });
  expect(await (await page.request.get(`${on}/document`)).json()).not.toEqual(exported);
  // Imported: as it was.
  const choosing = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Import from a file…" }).click();
  await (await choosing).setFiles(file);
  await expect(said(page, "document-said")).toContainText("imported");
  await expect(said(page, "imported").getByRole("listitem")).not.toHaveCount(0);
  expect(await (await page.request.get(`${on}/document`)).json()).toEqual(exported);
  // A file that isn't one is refused here, before the server.
  const notOne = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Import from a file…" }).click();
  await (
    await notOne
  ).setFiles({ name: "notes.json", mimeType: "application/json", buffer: Buffer.from("[1]") });
  await expect(said(page, "document-said")).toHaveText(
    "notes.json isn't an OScope settings document.",
  );
});

test("the devices' saved settings are shown, and each forgotten", async ({ page, server }) => {
  const on = new Api(server.url);
  await page.request.put(`${server.url}/api/v1/camera/exposure`, { data: 12.5 });
  await act("/api/v1/objective/fit", { name: "50x" }, { on });
  const light = { channel: "transmitted", on: true, intensity: 0.25 };
  await act("/api/v1/illumination/set_light", light as never, { on });
  await page.request.put(`${server.url}/api/v1/stage/speed`, { data: 500 });
  await settings(page, server.url, "Devices");
  await expect(said(page, "saved-camera")).toContainText("12.5 ms");
  await expect(said(page, "saved-lights")).toContainText("on, at 25 %");
  await expect(said(page, "saved-objective")).toContainText("50x");
  await expect(said(page, "saved-speed")).toContainText("500 µm/s");
  for (const [device, word] of [
    ["camera", "camera's settings"],
    ["lights", "lights' settings"],
    ["objective", "objective fitted"],
  ] as const) {
    await said(page, `saved-${device}`).getByRole("button", { name: "Forget them" }).click();
    await expect(said(page, "devices-said")).toContainText(word);
    await expect(said(page, `saved-${device}`)).toContainText("Nothing saved", { timeout: 15_000 });
  }
  await said(page, "saved-speed").getByRole("button", { name: "Back to the stage's own" }).click();
  await expect(said(page, "saved-speed")).toContainText("The stage's own", { timeout: 15_000 });
  // Forgotten, not undone: the camera keeps the exposure it has.
  const exposure = await (await page.request.get(`${server.url}/api/v1/camera/exposure`)).json();
  expect(exposure).toBe(12.5);
});

test("the account's scale bar and colour map reach the Gallery's viewer", async ({
  page,
  server,
}) => {
  const project = await made(server.url, "Grey");
  const record = await imported(
    server.url,
    project,
    "grey.tif",
    tiffOf(512, 384, 1, (x, y) => (x * 61 + y * 37) % 60_000),
  );
  const view = async () => {
    const query = new URLSearchParams({ project, record, view: "1" });
    await page.goto(`${server.url}/#/gallery?${query.toString()}`);
    await expect(page.getByRole("combobox", { name: "Colour map" })).toBeVisible({
      timeout: 30_000,
    });
  };
  await view();
  await expect(said(page, "tile-scale")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("combobox", { name: "Colour map" })).toHaveText("Its own colours");
  await settings(page, server.url);
  await page.getByRole("switch", { name: "A scale bar on images" }).click();
  await choose(page, "One-channel images in", "Viridis");
  await expect(said(page, "display-said")).toHaveText("Saved with your account.");
  await view();
  await expect(page.getByRole("combobox", { name: "Colour map" })).toHaveText("Viridis");
  await expect(said(page, "tile-scale")).toHaveCount(0);
});

test("the settings have no violation axe can find, in either theme", async ({ page, server }) => {
  for (const pane of ["Display", "System", "Devices", "The whole document"]) {
    await settings(page, server.url, pane);
    await expect(page.getByRole("tabpanel")).toBeVisible();
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await expectAccessible(page);
    }
  }
});

test("the display's settings look as their baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await settings(page, server.url);
  await expect(page.getByRole("combobox", { name: "Lengths in" })).toHaveText("µm");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`settings-${scheme}.png`);
  }
});

test("the display's and the devices' settings, for the notes' screenshots", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
  const on = new Api(server.url);
  await act("/api/v1/objective/fit", { name: "20x" }, { on });
  await act(
    "/api/v1/illumination/set_light",
    { channel: "transmitted", on: true, intensity: 0.6 } as never,
    { on },
  );
  await page.request.put(`${server.url}/api/v1/camera/exposure`, { data: 8 });
  await settings(page, server.url);
  await choose(page, "Lengths in", "mm");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W8.1-display-${scheme}`);
  }
  await page.getByRole("tab", { name: "Devices", exact: true }).click();
  await expect(said(page, "saved-camera")).toContainText("8 ms");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W8.1-devices-${scheme}`);
  }
});
