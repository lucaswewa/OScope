// Covers: web/src/features/settings
// Extensions and their panels (W8.5): the sample Rust extension installed by
// uploading its package, loaded at a restart, its node offered and its
// panel opened at its route, run on the live view's newest frame and its
// answer shown; then uninstalled. A package that isn't one refused; axe;
// the view's baseline; and the notes' screenshots.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { embossPackage } from "./extensions.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";
import { overTissue } from "./sim.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const EMBOSS = "org.oscope.examples.emboss";
const card = (page: Page) => page.locator(`[data-extension="${EMBOSS}"]`);

/** Settings' extensions view. */
async function extensions(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/settings?view=extensions`);
  await expect(page.getByRole("tab", { name: "Extensions", exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
}

/** The sample's package, chosen in the view's file chooser. */
async function installed(page: Page, file: string): Promise<void> {
  const choosing = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Install a package…" }).click();
  await (await choosing).setFiles(file);
  await expect(said(page, "extensions-said")).toHaveText(
    "Emboss 0.1.0 is installed: the next start loads it.",
    { timeout: 30_000 },
  );
}

/**
 * The server restarted, and answering again: its count of restarts rises as
 * the old one exits, before the new one listens.
 */
async function restarted(page: Page, server: { url: string; restarts: number }): Promise<void> {
  await expect.poll(() => server.restarts, { timeout: 60_000 }).toBe(1);
  const answers = () =>
    page.request.get(`${server.url}/health`).then(
      (reply) => reply.ok(),
      () => false,
    );
  await expect.poll(answers, { timeout: 60_000 }).toBe(true);
}

test.describe("a server that restarts", () => {
  test.use({ restarts: true });

  test("an extension installed, loaded at a restart, its panel run, and uninstalled @main", async ({
    page,
    server,
  }) => {
    test.setTimeout(300_000);
    const file = embossPackage(test.info().outputPath());
    await extensions(page, server.url);
    await expect(said(page, "extensions")).toContainText("No extension is installed.");
    await installed(page, file);
    await expect(card(page)).toContainText("The next start loads 0.1.0");
    await expect(card(page)).toContainText("example.emboss");
    await expect(said(page, "extensions-restart")).toBeVisible();
    // Restarted: loaded, its node among the live recipes', its panel listed.
    const on = new Api(server.url);
    await act("/api/v1/power/restart", { confirm: true, force: false }, { on }).catch(
      () => undefined,
    );
    await restarted(page, server);
    await extensions(page, server.url);
    await expect(card(page).locator('[data-said="standing"]')).toHaveText("Loaded: 0.1.0", {
      timeout: 30_000,
    });
    await expect(said(page, "extensions-restart")).toHaveCount(0);
    const nodes = await (await page.request.get(`${server.url}/api/v1/preview/nodes`)).json();
    expect(nodes.map((node: { id: string }) => node.id)).toContain("example.emboss");

    // Its panel, at its own route: the form from its schema, run on the newest frame.
    await overTissue(server.url);
    await act("/api/v1/camera/start_streaming", {}, { on });
    await card(page).getByRole("link", { name: "Emboss the live view" }).click();
    await expect(page).toHaveURL(
      /#\/settings\/extensions\/org\.oscope\.examples\.emboss\/example\.emboss$/,
    );
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Emboss the live view");
    await expect(said(page, "panel-submits")).toHaveText(
      "Runs the node example.emboss on the live view's newest frame.",
    );
    await expect(said(page, "panel-unknown")).toHaveCount(0);
    const strength = said(page, "panel").getByRole("spinbutton", { name: "Strength" });
    await expect(strength).toHaveValue("1");
    const answer = said(page, "panel-answer").getByRole("img", {
      name: "What the node gave, image",
    });
    /** The answer's mean grey, how coloured it is, and how much its grey varies. */
    const measured = () =>
      answer.evaluate((img: HTMLImageElement) => {
        const canvas = document.createElement("canvas");
        [canvas.width, canvas.height] = [img.naturalWidth, img.naturalHeight];
        const drawn = canvas.getContext("2d")!;
        drawn.drawImage(img, 0, 0);
        const { data } = drawn.getImageData(0, 0, canvas.width, canvas.height);
        const greys: number[] = [];
        let chroma = 0;
        for (let i = 0; i < data.length; i += 4) {
          const [r, g, b] = [data[i]!, data[i + 1]!, data[i + 2]!];
          greys.push((r + g + b) / 3);
          chroma += Math.max(r, g, b) - Math.min(r, g, b);
        }
        const mean = greys.reduce((sum, v) => sum + v, 0) / greys.length;
        const spread = Math.sqrt(greys.reduce((sum, v) => sum + (v - mean) ** 2, 0) / greys.length);
        return { mean, colour: chroma / greys.length, spread, src: img.src };
      });
    /** Runs it at `value`: what it gave, once a new answer is drawn. */
    const runAt = async (value: string, before?: string) => {
      await strength.fill(value);
      await strength.press("Tab");
      await page.getByRole("button", { name: "Run it" }).click();
      await expect(answer).toBeVisible({ timeout: 60_000 });
      await expect
        .poll(async () => (await measured()).src !== before, { timeout: 60_000 })
        .toBe(true);
      return measured();
    };
    // Strength 0: no relief, a flat grey of 0.5. Strength 3: the stained tissue in relief, still grey.
    const flat = await runAt("0");
    expect(flat.mean).toBeGreaterThan(120);
    expect(flat.mean).toBeLessThan(136);
    expect(flat.spread).toBeLessThan(2);
    const relief = await runAt("3", flat.src);
    expect(relief.spread).toBeGreaterThan(flat.spread + 5);
    expect(relief.colour).toBeLessThan(20);
    await expect(said(page, "panel-answer")).toContainText(/\d+ × \d+/);

    // Back, and uninstalled: its files go at the next start, which the page says.
    await page.getByRole("link", { name: "Back to the extensions" }).click();
    await card(page).getByRole("button", { name: "Uninstall" }).click();
    await expect(said(page, "extensions-said")).toHaveText("Emboss is uninstalled.");
    await expect(card(page).locator('[data-said="standing"]')).toHaveText(
      "Uninstalled at the next start",
    );
    await expect(said(page, "extensions-restart")).toBeVisible();
  });
});

test("a package that isn't one is refused, saying why", async ({ page, server }) => {
  await extensions(page, server.url);
  const choosing = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "Install a package…" }).click();
  await (
    await choosing
  ).setFiles({
    name: "not.omext",
    mimeType: "application/octet-stream",
    buffer: Buffer.from("not a package"),
  });
  await expect(said(page, "extensions-said")).toContainText("Refused:");
  await expect(said(page, "extensions")).toContainText("No extension is installed.");
});

test("the extensions view has no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  test.setTimeout(240_000);
  const file = embossPackage(test.info().outputPath());
  await extensions(page, server.url);
  await installed(page, file);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the extensions view looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await extensions(page, server.url);
  await expect(said(page, "extensions")).toContainText("This OScope offers the extension API");
  await expect(said(page, "extensions")).toContainText("No extension is installed.");
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`extensions-${scheme}.png`);
  }
});

test.describe("for the notes' screenshots", () => {
  test.use({ restarts: true });

  test("the extensions view and a panel's answer", async ({ page, server }) => {
    test.skip(!process.env.OSCOPE_SHOTS, "only when the shots are taken: OSCOPE_SHOTS=1");
    test.setTimeout(300_000);
    const on = new Api(server.url);
    await extensions(page, server.url);
    await installed(page, embossPackage(test.info().outputPath()));
    await act("/api/v1/power/restart", { confirm: true, force: false }, { on }).catch(
      () => undefined,
    );
    await restarted(page, server);
    await extensions(page, server.url);
    await expect(card(page).locator('[data-said="standing"]')).toHaveText("Loaded: 0.1.0", {
      timeout: 30_000,
    });
    await page.mouse.move(5, 5);
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await shot(page, `W8.5-extensions-${scheme}`);
    }
    await overTissue(server.url);
    await act("/api/v1/camera/start_streaming", {}, { on });
    await card(page).getByRole("link", { name: "Emboss the live view" }).click();
    await page.getByRole("button", { name: "Run it" }).click();
    await expect(said(page, "panel-answer").getByRole("img")).toBeVisible({ timeout: 60_000 });
    await page.mouse.move(5, 5);
    for (const scheme of ["light", "dark"] as const) {
      await inScheme(page, scheme);
      await shot(page, `W8.5-panel-${scheme}`);
    }
  });
});
