// The Things page (W1.2): the stage's state kept live while another client
// moves it; marked stale without the server, and live again when it
// returns; and a refused write taken back, with its reason at the field.
import type { Locator, Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server } from "../../src/api/client.ts";
import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

/** The page, read once its socket is open, and what each part of it says. */
async function open(page: Page, url: string) {
  await page.goto(`${url}/#/dev/gallery/things`);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Live Things");
  const main = page.getByRole("main");
  const said = (name: string) => main.locator(`[data-said="${name}"]`);
  await expect(said("socket")).toContainText("is open");
  return { main, said };
}

/** The x a position on the page is at, in µm. */
async function xOf(place: Locator): Promise<number> {
  const found = /x (-?[\d.]+)/.exec((await place.textContent()) ?? "");
  return found ? Number(found[1]) : Number.NaN;
}

const VALUES = ["position", "status", "moving", "speed"];

test("the page follows the stage, moved by another client @main", async ({ page, server }) => {
  test.setTimeout(60_000);
  const { said } = await open(page, server.url);
  const other = new Server(server.url);
  await act("/api/v1/stage/home", {}, { on: other, timeout: 30_000 });
  await act("/api/v1/stage/move_to", { x: 5000, y: 4000, z: 1000 }, { on: other });
  // The simulated stage stops within a few µm of its target.
  await expect.poll(async () => Math.abs((await xOf(said("position"))) - 5000) < 5).toBe(true);
  await expect(said("status")).toContainText("idle");
  await expect(said("moving")).toContainText("no");
  await expect(said("moved").getByRole("listitem")).toHaveCount(2);
  expect(Math.abs((await xOf(said("moved").getByRole("listitem").last())) - 5000) < 5).toBe(true);
});

test("its values are stale without the server, and live again when it returns", async ({
  page,
  server,
}) => {
  test.setTimeout(90_000);
  const { said } = await open(page, server.url);
  await expect(said("position")).not.toContainText("not read yet");
  for (const name of VALUES) await expect(said(name)).not.toContainText("stale");
  await server.stop();
  await expect(said("socket")).toContainText("closed");
  for (const name of VALUES) await expect(said(name)).toContainText("stale", { timeout: 15_000 });
  await server.start();
  await expect(said("socket")).toContainText("is open", { timeout: 30_000 });
  for (const name of VALUES)
    await expect(said(name)).not.toContainText("stale", { timeout: 15_000 });
});

test("a refused write goes back, with the server's reason at the field", async ({
  page,
  server,
}) => {
  const { main, said } = await open(page, server.url);
  await expect(said("gain")).toContainText("The camera's gain is");
  const before = await said("gain").textContent();
  const gain = main.getByRole("spinbutton", { name: "Gain, in the camera's units" });
  const write = main.getByRole("button", { name: "Write" });
  const reason = "the gain must be from 0 to 240, and was 300";
  await gain.fill("300");
  await gain.press("Enter");
  await write.click();
  // The reason at the field, which names it as its description; the gain as it was.
  await expect(gain).toHaveAccessibleDescription(reason);
  await expect(said("gain")).toHaveText(before!);
  // A gain it takes, the reason gone with the refusal.
  await gain.fill("100");
  await gain.press("Enter");
  await write.click();
  await expect(said("gain")).toContainText("The camera's gain is 100.");
  await expect(gain).not.toHaveAccessibleDescription(reason);
});

test("the Things page has no violation axe can find, in either theme", async ({ page, server }) => {
  await open(page, server.url);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("the Things page looks as its baselines do, in both themes @visual", async ({
  page,
  server,
}) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  const { said } = await open(page, server.url);
  await expect(said("position")).not.toContainText("not read yet");
  await expect(said("speed")).toContainText("default");
  await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`things-${scheme}.png`, { fullPage: true });
    await shot(page, `W1.2-things-${scheme}`);
  }
});
