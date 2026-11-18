import AxeBuilder from "@axe-core/playwright";
import { expect, type Page } from "@playwright/test";

/**
 * Checks the page as it stands with axe (ADR-0040), against WCAG 2.2's
 * levels A and AA and axe's own good practices: no violation, each named
 * with the elements it was found on.
 *
 * With `layer`, a selector, only what is inside it is checked: for a list or
 * a dialog that is open. The library hides the page behind an open layer
 * from a screen reader, as it should, and axe then finds a page with no
 * heading and no landmark, which says nothing about the layer.
 */
export async function expectAccessible(page: Page, layer?: string): Promise<void> {
  const tags = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"];
  const axe = new AxeBuilder({ page }).withTags(tags);
  if (layer) axe.include(layer).disableRules(["region"]);
  const { violations, passes } = await axe.analyze();
  const found = violations.map((violation) => {
    const where = violation.nodes.map((node) => node.target.join(" ")).join("; ");
    return `${violation.id}: ${violation.help} [${where}]`;
  });
  expect(found).toEqual([]);
  // A check that looked at nothing would pass too.
  expect(passes.length).toBeGreaterThan(0);
}
