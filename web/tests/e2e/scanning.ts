// The scan tab's specs' own (W5.1, W5.2): what the tab says where, a plan's
// estimate, and a region typed, in mm, into a pane's fields.
import type { Locator, Page } from "@playwright/test";

/** What the page says under `name`. */
export const said = (page: Page, name: string): Locator => page.locator(`[data-said="${name}"]`);

/** A term of the plan's estimate: its value. */
export const estimate = (page: Page, term: string): Locator =>
  said(page, "estimate").locator("dt", { hasText: term }).locator("xpath=following-sibling::dd[1]");

/** Types a region, in mm, into a pane's fields. */
export async function typeRegion(
  page: Page,
  pane: string,
  [x, y, width, height]: number[],
): Promise<void> {
  const fields = said(page, pane);
  for (const [label, value] of [
    ["x", x],
    ["y", y],
    ["Width", width],
    ["Height", height],
  ] as const) {
    const field = fields.getByLabel(label, { exact: true });
    await field.fill(String(value));
    await field.press("Tab");
  }
}
