// Covers: web/src/features/workspace
// The graph editor (W7.2): a graph built by hand passes `check`; a
// connection between unlike ports is refused as it is dragged; a broken
// graph's faults show where they are; the keyboard's path, delete and undo;
// a recipe's file exported and imported without loss; axe; and a graph's
// baseline.
import { readFile } from "node:fs/promises";

import type { Page } from "@playwright/test";

import { expectAccessible } from "./axe.ts";
import { expect, inScheme, shot, test } from "./fixtures.ts";

const said = (page: Page, name: string) => page.locator(`[data-said="${name}"]`);
const input = (page: Page, reference: string) => page.locator(`[data-input="${reference}"]`);
const output = (page: Page, reference: string) => page.locator(`[data-output="${reference}"]`);
const edges = (page: Page) => said(page, "canvas").locator(".vue-flow__edge");

/** The Workspace's editor, its catalogue read. */
async function editor(page: Page, url: string): Promise<void> {
  await page.goto(`${url}/#/workspace`);
  await expect(
    said(page, "palette").getByRole("button", { name: "Add filter.gaussian" }),
  ).toBeVisible();
}

/** A node added from the palette, found by `text`. */
async function add(page: Page, text: string, node: string): Promise<void> {
  const find = said(page, "palette").getByRole("textbox", { name: "Find a node" });
  await find.fill(text);
  await said(page, "palette")
    .getByRole("button", { name: `Add ${node}` })
    .click();
  await find.fill("");
}

/** `from`, an output, joined to `to`, an input, from the keyboard's buttons. */
async function join(page: Page, from: string, to: string): Promise<void> {
  const [fromNode, fromPort] = from.split(".") as [string, string];
  await output(page, from).getByRole("button").press("Enter");
  await expect(said(page, "editor-said")).toHaveText(
    `Joining from ${fromNode}.${fromPort}: choose an input.`,
  );
  await input(page, to).getByRole("button").press("Enter");
}

/** A blur, then Otsu's threshold, its mask the recipe's output. */
async function blurred(page: Page): Promise<void> {
  await add(page, "gaussian", "filter.gaussian");
  await add(page, "otsu", "threshold.otsu");
  await join(page, "input.image", "gaussian.image");
  await join(page, "gaussian.image", "otsu.image");
  await output(page, "otsu.mask").getByRole("button").press("Enter");
  await said(page, "node-output")
    .getByRole("button", { name: "A new output: join an output to it" })
    .press("Enter");
}

test("a graph built by hand passes check @main", async ({ page, server }) => {
  await editor(page, server.url);
  await expect(said(page, "checked")).toHaveText("One fault");
  await blurred(page);
  await expect(edges(page)).toHaveCount(3);
  // Sigma has no default: a fault on the node, until it is given.
  await expect(said(page, "checked")).toHaveText(/fault/);
  await expect(said(page, "node-gaussian").locator('[data-said="faults"]')).toContainText("sigma");
});

test("a connection between unlike ports is refused, as it is dragged and from the keyboard", async ({
  page,
  server,
}) => {
  await editor(page, server.url);
  await blurred(page);
  await expect(edges(page)).toHaveCount(3);
  // Laid out and shown whole, clear of the minimap.
  await said(page, "editor").getByRole("button", { name: "Lay it out", exact: true }).click();
  // Otsu's mask onto the blur's image input, by the mouse: no edge made.
  const from = (await output(page, "otsu.mask").locator(".vue-flow__handle").boundingBox())!;
  const to = (await input(page, "gaussian.image").locator(".vue-flow__handle").boundingBox())!;
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 8 });
  // Refused as it is dragged: the line says so before it is let go.
  await expect(said(page, "canvas").locator(".vue-flow__connection-path")).toHaveClass(/invalid/);
  await page.mouse.up();
  await expect(edges(page)).toHaveCount(3);
  // And from the keyboard: refused, saying why.
  await output(page, "otsu.mask").getByRole("button").press("Enter");
  await input(page, "gaussian.image").getByRole("button").press("Enter");
  await expect(said(page, "editor-said")).toHaveText(
    "otsu.mask can't be joined to gaussian.image: their types differ.",
  );
  await page.keyboard.press("Escape");
  await expect(said(page, "editor-said")).toHaveText("Joining stopped.");
});

test("a broken graph's faults show on the nodes and ports they are at", async ({
  page,
  server,
}) => {
  await editor(page, server.url);
  await add(page, "otsu", "threshold.otsu");
  // Otsu's image has no edge, and the recipe gives nothing.
  await expect(said(page, "checked")).toHaveText("2 faults");
  await expect(input(page, "otsu.image")).toHaveClass(/faulty/);
  await expect(said(page, "node-otsu").locator('[data-said="faults"]')).toContainText(
    "image: its input `image` has no edge",
  );
  await expect(said(page, "recipe-faults")).toContainText("the recipe gives nothing");
  await join(page, "input.image", "otsu.image");
  await output(page, "otsu.mask").getByRole("button").press("Enter");
  await said(page, "node-output")
    .getByRole("button", { name: "A new output: join an output to it" })
    .press("Enter");
  await expect(said(page, "checked")).toHaveText("It can run.");
  await expect(input(page, "otsu.image")).not.toHaveClass(/faulty/);
});

test("the keyboard's path: a node reached, deleted, and brought back @main", async ({
  page,
  server,
}) => {
  await editor(page, server.url);
  await add(page, "otsu", "threshold.otsu");
  await join(page, "input.image", "otsu.image");
  await expect(said(page, "editor-said")).toHaveText("input.image joined to otsu.image.");
  // The node, reached and selected from the keyboard, then deleted.
  const node = said(page, "canvas").locator('.vue-flow__node[data-id="otsu"]');
  await node.focus();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Delete");
  await expect(said(page, "node-otsu")).toHaveCount(0);
  await expect(edges(page)).toHaveCount(0);
  // Undone, then redone, from the keys, the focus in the editor.
  const inside = said(page, "editor").getByRole("button", { name: "All of it" });
  await inside.press("Control+z");
  await expect(said(page, "node-otsu")).toHaveCount(1);
  await expect(edges(page)).toHaveCount(1);
  await inside.press("Control+y");
  await expect(said(page, "node-otsu")).toHaveCount(0);
});

/** The recipe exported: its file's text. */
async function exported(page: Page): Promise<string> {
  const saving = page.waitForEvent("download");
  await said(page, "editor").getByRole("button", { name: "Export", exact: true }).click();
  return readFile(await (await saving).path(), "utf-8");
}

/** `text` imported, as the file `name`. */
async function imported(page: Page, name: string, text: string): Promise<void> {
  const choosing = page.waitForEvent("filechooser");
  await said(page, "editor").getByRole("button", { name: "Import…" }).click();
  const buffer = Buffer.from(text);
  await (await choosing).setFiles({ name, mimeType: "application/json", buffer });
  await expect(said(page, "editor-said")).toHaveText(`${name} imported.`);
}

test("a recipe's file is exported, and imported without loss", async ({ page, server }) => {
  await editor(page, server.url);
  await blurred(page);
  await said(page, "editor").getByRole("button", { name: "Lay it out", exact: true }).click();
  const file = await exported(page);
  const recipe = JSON.parse(file) as { nodes: unknown[]; layout: object };
  expect(recipe.nodes).toHaveLength(2);
  expect(Object.keys(recipe.layout).sort()).toEqual(["gaussian", "input", "otsu", "output"]);
  // A new recipe, then the file imported, a node moved in it: the same recipe, drawn so.
  const layout = { ...recipe.layout, gaussian: [300, 240] };
  const moved = `${JSON.stringify({ ...recipe, layout }, null, 2)}\n`;
  await said(page, "editor").getByRole("button", { name: "New", exact: true }).click();
  // Its changes aren't saved (W7.3): asked first.
  await page.getByRole("alertdialog").getByRole("button", { name: "Leave them" }).click();
  await expect(edges(page)).toHaveCount(0);
  await imported(page, "blurred.json", moved);
  await expect(edges(page)).toHaveCount(3);
  expect(await exported(page)).toBe(moved);
  // One with no places, as if written by hand, is laid out as it comes in.
  await imported(page, "bare.json", JSON.stringify({ ...recipe, layout: undefined }));
  expect((JSON.parse(await exported(page)) as typeof recipe).layout).toEqual(recipe.layout);
});

test("the editor has no violation axe can find, in either theme", async ({ page, server }) => {
  await editor(page, server.url);
  await blurred(page);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});

test("a graph looks as its baseline does @visual", async ({ page, server }) => {
  test.skip(!process.env.OSCOPE_VISUAL, "the baselines are one computer's: OSCOPE_VISUAL=1 there");
  await editor(page, server.url);
  await blurred(page);
  await said(page, "editor").getByRole("button", { name: "Lay it out", exact: true }).click();
  await expect(said(page, "checked")).toHaveText(/fault/);
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expect(page).toHaveScreenshot(`editor-${scheme}.png`);
    await shot(page, `W7.2-editor-${scheme}`);
  }
});
