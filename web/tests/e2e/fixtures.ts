import { expect, test as base, type Page } from "@playwright/test";
import { resolve } from "node:path";

import { Server, web } from "./server.ts";

/**
 * Each test's own server, started before it and gone after it. With
 * `restarts`, its restart is done, and it starts again (ADR-0060).
 */
export const test = base.extend<{ server: Server; restarts: boolean; thrown: void }>({
  restarts: [false, { option: true }],
  server: async ({ restarts }, use) => {
    const server = new Server({ restarts });
    try {
      await server.start();
      await use(server);
    } finally {
      await server.dispose();
    }
  },
  // An error the page didn't catch fails the test it happened in, whatever
  // the test was looking at. WebKit reports a request that a reload cuts off
  // as one access control refused, an error of the page, though the page
  // threw nothing: those are let be (W3.1).
  thrown: [
    async ({ page, browserName }, use) => {
      const errors: string[] = [];
      const cutByReload = (message: string) =>
        browserName === "webkit" && message.endsWith("due to access control checks.");
      page.on("pageerror", (error) => {
        if (!cutByReload(error.message)) errors.push(error.message);
      });
      await use();
      expect(errors, "what the page threw").toEqual([]);
    },
    { auto: true },
  ],
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
