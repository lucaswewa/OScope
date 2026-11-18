import { expect, test as base, type Page } from "@playwright/test";
import { resolve } from "node:path";

import { Server, web } from "./server.ts";

/** Each test's own server, started before it and gone after it. */
export const test = base.extend<{ server: Server }>({
  // Playwright reads the fixtures a fixture needs from this pattern: none.
  // eslint-disable-next-line no-empty-pattern
  server: async ({}, use) => {
    const server = new Server();
    await server.start();
    await use(server);
    await server.dispose();
  },
});

export { expect };

/**
 * Takes a screenshot for the phase's notes, into
 * `docs/milestone-2/notes/shots/`, when `OSCOPE_SHOTS` is set: a run without
 * it leaves the repository as it was.
 */
export async function shot(page: Page, name: string): Promise<void> {
  if (!process.env.OSCOPE_SHOTS) return;
  const shots = resolve(web, "..", "docs", "milestone-2", "notes", "shots");
  await page.screenshot({ path: resolve(shots, `${name}.png`) });
}

/** The page's surface, as the tokens give it to each scheme. */
export const surface = { light: "rgb(255, 255, 255)", dark: "rgb(31, 31, 31)" } as const;

/**
 * Puts the browser in a scheme, and waits until the page is drawn in it. With
 * less motion asked for, so that nothing is still fading from the scheme
 * before when the page is measured.
 */
export async function inScheme(page: Page, scheme: "light" | "dark"): Promise<void> {
  await page.emulateMedia({ colorScheme: scheme, reducedMotion: "reduce" });
  await expect(page.locator("body")).toHaveCSS("background-color", surface[scheme]);
}

/** Chooses from one of the app's own lists, as a user does: opens it, and presses a choice. */
export async function choose(page: Page, list: string, choice: string): Promise<void> {
  await page.getByRole("combobox", { name: list, exact: true }).click();
  await page.getByRole("option", { name: choice, exact: true }).click();
}
