import AxeBuilder from "@axe-core/playwright";
import { expect, type Page } from "@playwright/test";

/**
 * Checks the page as it stands with axe (ADR-0040), against WCAG 2.2's
 * levels A and AA and axe's own good practices: no violation, each named
 * with the elements it was found on.
 *
 * A frame sandboxed with nothing allowed is left out (W7.9).
 *
 * With `layer`, a selector, only what is inside it is checked: for a list or
 * a dialog that is open. The library hides the page behind an open layer
 * from a screen reader, as it should, and axe then finds a page with no
 * heading and no landmark, which says nothing about the layer.
 */
export async function expectAccessible(page: Page, layer?: string): Promise<void> {
  const tags = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"];
  // A frame sandboxed with nothing allowed runs no script, axe's own among
  // them, and axe would wait on it to the end of the test: its document is
  // a report through a user's template (W7.9), not the app's, and is left out.
  const axe = new AxeBuilder({ page }).withTags(tags).exclude('iframe[sandbox=""]');
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

/** The roles a person operates, each of which a screen reader says by its name. */
const CONTROLS = [
  "button",
  "link",
  "textbox",
  "searchbox",
  "combobox",
  "checkbox",
  "radio",
  "switch",
  "slider",
  "spinbutton",
  "tab",
  "menuitem",
  "menuitemcheckbox",
  "menuitemradio",
  "option",
  "treeitem",
] as const;
/** A control's line in an ARIA snapshot, with no name in quotes after its role. */
const UNNAMED = new RegExp(String.raw`^\s*- (${CONTROLS.join("|")})(?: \[[^\]]*\])*(?::.*)?$`);

/**
 * Checks that every control the page shows, or that `layer` holds, has a
 * name for a screen reader (W10.2), as Playwright's ARIA snapshot reads the
 * accessibility tree: each one without, by its line in the snapshot.
 */
export async function expectNamed(page: Page, layer = "body"): Promise<void> {
  const snapshot = await page.locator(layer).first().ariaSnapshot();
  const unnamed = snapshot.split("\n").filter((line) => UNNAMED.test(line));
  expect(unnamed, "controls without a name").toEqual([]);
}
