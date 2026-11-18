import { test as base, type Page } from "@playwright/test";
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

export { expect } from "@playwright/test";

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
