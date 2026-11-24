// Checkpoint L (W8.6's notes): the 20x calibrated by the wizard, on a
// simulator whose camera is turned by -20° and whose objectives magnify 2 %
// less than the catalogue says; one's own sample imported and seen; the
// sample extension installed, loaded at a restart, and its panel run; the
// display's units; and the first account, security on at the next restart,
// which then asks for the login. Both restarts are the test's own server
// really stopping and starting again (ADR-0060).
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { embossPackage } from "../e2e/extensions.ts";
import { choose, expect, test } from "../e2e/fixtures.ts";
import { rgbOf } from "../e2e/pixels.ts";
import { rgbTiffOf } from "../e2e/tiff.ts";
import type { Server } from "../e2e/server.ts";
import { homeAndMove, image, moveTo, readout, said, stream } from "./steps.ts";

test.use({ restarts: true, simulator: "rotation = -20.0\nscale = 0.98" });

const AT = { x: 40_000, y: 12_000 };
/** The coverslip's middle, where one's own image lies. */
const MIDDLE = { x: 47_000, y: 12_500, z: 5_000 };

/** Settings' view `view`. */
async function settings(page: Page, view: string): Promise<void> {
  await page.getByRole("link", { name: "Settings", exact: true }).click();
  await page.getByRole("tab", { name: view, exact: true }).click();
}

/** Restarted from Power, its name typed: the `n`th restart, and the server answering again. */
async function restarted(page: Page, server: Server, n: number): Promise<void> {
  await page.getByRole("link", { name: "Power", exact: true }).click();
  await page.locator('[data-action="restart"]').getByRole("button", { name: "Restart…" }).click();
  const dialog = page.getByRole("alertdialog");
  await dialog.getByRole("textbox").fill("restart");
  await dialog.getByRole("button", { name: "Restart", exact: true }).click();
  await expect.poll(() => server.restarts, { timeout: 60_000 }).toBe(n);
  const answers = () =>
    page.request.get(`${server.url}/health`).then(
      (reply) => reply.ok(),
      () => false,
    );
  await expect.poll(answers, { timeout: 60_000 }).toBe(true);
}

test("Checkpoint L: calibrated by the wizard, one's own sample, an extension's panel, the units, and an account @main", async ({
  page,
  server,
}) => {
  test.setTimeout(600_000);
  const on = new Api(server.url);

  // The 20x fitted, over the tissue in focus, streaming.
  await page.goto(`${server.url}/#/control`);
  const z = (await act("/api/v1/simulation/in_focus", AT, { on })) as number;
  await homeAndMove(page, server.url, { ...AT, z: Math.round(z * 10) / 10 });
  await choose(page, "Fitted", "20x, NA 0.42");
  await stream(page, server.url);

  // The wizard: the objective confirmed, the geometry measured beside the live image, within
  // the simulator's truth; reloaded, where it was; a dark frame; the rest skipped.
  await settings(page, "Calibration");
  const wizard = said(page, "wizard");
  const steps = said(page, "wizard-steps").getByRole("listitem");
  const inHand = said(page, "wizard-step");
  const button = (name: string) => wizard.getByRole("button", { name, exact: true });
  const measured = async (n: number) => {
    await button("Done: measure it").click();
    await expect(steps.nth(n - 1)).toContainText("Done", { timeout: 180_000 });
  };
  await button("Start the wizard").click();
  await expect(inHand).toContainText("Step 1 of 6: Check the objective");
  await measured(1);
  await expect(steps.nth(0)).toContainText("plausible: the 20x");
  await expect(wizard.getByRole("img", { name: /^The live image/ })).toBeVisible();
  await measured(2);
  const active = await (await page.request.get(`${server.url}/api/v1/calibration/active`)).json();
  const { pixel_um: pixel, rotation_deg: rotation } = active.geometry.values;
  expect(Math.abs(pixel / (2.4 / (20 * 0.98)) - 1)).toBeLessThan(0.002);
  expect(Math.abs(rotation + 20)).toBeLessThan(0.1);
  await page.reload();
  await page.getByRole("tab", { name: "Calibration", exact: true }).click();
  await expect(inHand).toContainText("Step 3 of 6: Take a dark frame");
  await measured(3);
  for (const n of [4, 5, 6]) {
    await button("Skip it").click();
    await expect(steps.nth(n - 1)).toContainText("Skipped");
  }
  await expect(said(page, "wizard-ended")).toHaveText("Finished: 6 of 6 steps done or skipped.");

  // One's own sample: a photograph of 10 µm a pixel, imported, loaded, and seen at the
  // coverslip's middle.
  await settings(page, "Simulation");
  const choosing = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Choose a file…" }).click();
  const red = rgbTiffOf(400, 300, () => [220, 30, 30]);
  await (await choosing).setFiles({ name: "my slide.tif", mimeType: "image/tiff", buffer: red });
  const form = said(page, "own-sample");
  await expect(form.getByRole("textbox", { name: "Its name" })).toHaveValue("my slide");
  await form.getByRole("spinbutton", { name: "A pixel, in µm" }).fill("10");
  await form.getByRole("spinbutton", { name: "A pixel, in µm" }).press("Tab");
  await form.getByRole("button", { name: "Import it" }).click();
  const sample = page.locator('[data-sample="my slide"]');
  await expect(sample).toContainText("Your own image, without ground truth.");
  await sample.getByRole("button", { name: "Load it" }).click();
  await expect(sample).toContainText("On the stage");
  await page.getByRole("link", { name: "Control", exact: true }).click();
  await moveTo(page, server.url, MIDDLE);
  const reddish = async () => {
    const pixels = await rgbOf(page, image(page));
    const red = pixels.filter(([r, g, b]) => r! > 120 && r! > 2 * g! && r! > 2 * b!);
    return red.length / pixels.length;
  };
  await expect.poll(reddish, { timeout: 60_000 }).toBeGreaterThan(0.5);

  // The sample extension: installed, loaded at a restart, its panel run on the newest frame.
  const file = embossPackage(test.info().outputPath());
  await settings(page, "Extensions");
  const choosingPackage = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Install a package…" }).click();
  await (await choosingPackage).setFiles(file);
  await expect(said(page, "extensions-said")).toHaveText(
    "Emboss 0.1.0 is installed: the next start loads it.",
    { timeout: 30_000 },
  );
  await restarted(page, server, 1);
  await page.goto(`${server.url}/#/control`);
  await homeAndMove(page, server.url, { ...AT, z: Math.round(z * 10) / 10 });
  await stream(page, server.url);
  await settings(page, "Extensions");
  const card = page.locator('[data-extension="org.oscope.examples.emboss"]');
  await expect(card.locator('[data-said="standing"]')).toHaveText("Loaded: 0.1.0", {
    timeout: 30_000,
  });
  await card.getByRole("link", { name: "Emboss the live view" }).click();
  await page.getByRole("button", { name: "Run it" }).click();
  await expect(
    said(page, "panel-answer").getByRole("img", { name: "What the node gave, image" }),
  ).toBeVisible({ timeout: 60_000 });

  // The display's units: lengths in mm, the stage's readout on Control following.
  await settings(page, "Display");
  await choose(page, "Lengths in", "mm");
  await page.getByRole("link", { name: "Control", exact: true }).click();
  await expect(readout(page, "position").first()).toContainText("mm");

  // The first account: security on at the next start, which then asks for the login.
  await page.goto(`${server.url}/#/settings?view=accounts`);
  const account = said(page, "new-account");
  await account.getByRole("textbox", { name: "Its name" }).fill("ana");
  await account.getByLabel("Its password").fill("ana's password");
  await account.getByLabel("The password again").fill("ana's password");
  await account.getByRole("button", { name: "Add it" }).click();
  await expect(said(page, "accounts-restart")).toContainText(
    "Security turns on at the next start.",
  );
  await restarted(page, server, 2);
  await page.goto(`${server.url}/#/control`);
  await page.reload();
  const heading = page.getByRole("heading", { level: 1 });
  await expect(heading).toHaveText("Log in to OScope");
  await page.getByRole("textbox", { name: "Name" }).fill("ana");
  await page.getByLabel("Password").fill("ana's password");
  await page.getByRole("button", { name: "Log in" }).click();
  await expect(heading).not.toHaveText("Log in to OScope");
});
