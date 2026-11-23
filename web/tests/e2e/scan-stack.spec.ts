// Covers: web/src/features/scan
// Z-scans, stacks and positions (W5.4), on the simulator, through the
// camera's tiny region over the tissue: a z-stack's planes are at the
// heights asked for; positions added where the stage was, one renamed and
// one moved first, are visited in their order; and axe.
import type { Page } from "@playwright/test";

import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { projectPage } from "../../src/api/projects.ts";
import { expectAccessible } from "./axe.ts";
import { choose, expect, inScheme, test } from "./fixtures.ts";
import { estimate, said } from "./scanning.ts";
import { fast, overTissueTiny, TISSUE } from "./sim.ts";

const plan = (page: Page) => said(page, "plan");

/** Over the tissue, streaming, a project to scan into, and the scan tab's `pattern` chosen. */
async function ready(page: Page, url: string, pattern: string): Promise<void> {
  await overTissueTiny(url);
  const on = new Api(url);
  await act("/api/v1/camera/start_streaming", {}, { on });
  await act("/api/v1/projects/create", { name: "Stacks" }, { on });
  await page.goto(`${url}/#/scan`);
  await choose(page, "Pattern", pattern);
}

/** Runs the plan into the project; waits for its end. */
async function run(page: Page): Promise<void> {
  await choose(page, "Project", "Stacks");
  await said(page, "run").getByRole("button", { name: "Run the scan" }).click();
  await expect(said(page, "scan-tiles")).toHaveText(/^Completed: (\d+) of \1 tiles$/, {
    timeout: 150_000,
  });
}

/** The positions of the frames of the project's acquisition, in the order taken. */
async function framesAt(url: string) {
  const on = new Api(url);
  const [project] = (await projectPage({}, { on })).projects;
  // Its answer has no schema in the OpenAPI document: its shape, as read here.
  const contents = (await call((signal) =>
    on.api.GET("/api/v1/projects/project/{id}", {
      params: { path: { id: project!.id } },
      signal,
    }),
  )) as unknown as {
    acquisitions: { frames: { position: { x: number; y: number; z: number } }[] }[];
  };
  return contents.acquisitions[0]!.frames.map((frame) => frame.position);
}

test("a z-stack's planes are at the heights asked for @main", async ({ page, server }) => {
  test.setTimeout(240_000);
  await ready(page, server.url, "Z-scan");
  for (const [label, value] of [
    ["Below", "10"],
    ["Above", "10"],
    ["Step", "1"],
  ] as const) {
    await plan(page).getByLabel(label, { exact: true }).fill(value);
    await plan(page).getByLabel(label, { exact: true }).press("Tab");
  }
  await plan(page).getByRole("button", { name: "Here", exact: true }).click();
  await expect(estimate(page, "Planes")).toHaveText("21", { timeout: 30_000 });
  const start = (await call((signal) =>
    new Api(server.url).api.GET("/api/v1/stage/position", { signal }),
  ))!.position.z;
  await run(page);
  // Each plane a micrometre above the last, from ten below where the stage was to ten above.
  const zs = (await framesAt(server.url)).map((at) => at.z);
  expect(zs).toHaveLength(21);
  zs.slice(1).forEach((z, i) => expect(z - zs[i]!).toBeCloseTo(1, 0));
  expect(zs[0]! - (start - 10)).toBeLessThan(1.5);
  expect(zs[0]! - (start - 10)).toBeGreaterThan(-1.5);
  expect(zs[20]! - zs[0]!).toBeCloseTo(20, 0);
});

test("positions are visited in their order, renamed and reordered", async ({ page, server }) => {
  test.setTimeout(240_000);
  await ready(page, server.url, "Positions");
  const on = new Api(server.url);
  const z = (await call((signal) => on.api.GET("/api/v1/stage/position", { signal })))!.position.z;
  const places = [
    { x: TISSUE.x - 300, y: TISSUE.y },
    { x: TISSUE.x, y: TISSUE.y + 200 },
    { x: TISSUE.x + 300, y: TISSUE.y - 100 },
  ];
  for (const at of places) {
    await fast(server.url, () => act("/api/v1/stage/move_to", { ...at, z, speed: 25_000 }, { on }));
    // The page has read the stage there, within its play, then adds it.
    const off = async () => {
      const text = (await said(page, "here").textContent()) ?? "";
      const [, x, y] = /x (-?[\d.]+) mm, y (-?[\d.]+) mm/.exec(text) ?? [];
      return Math.hypot(Number(x) * 1000 - at.x, Number(y) * 1000 - at.y);
    };
    await expect.poll(off, { timeout: 15_000 }).toBeLessThan(5);
    await plan(page).getByRole("button", { name: "Add where the stage is" }).click();
  }
  await expect(estimate(page, "Tiles")).toHaveText("3", { timeout: 30_000 });
  const second = said(page, "position-2").getByLabel("Position 2's name");
  await second.fill("Middle");
  await second.press("Tab");
  await plan(page).getByRole("button", { name: "Move Position 3 up" }).click();
  await plan(page).getByRole("button", { name: "Move Position 3 up" }).click();
  // In their order now: the third first, then the first, then the one renamed.
  const order = [places[2]!, places[0]!, places[1]!];
  await expect(said(page, "position-1").getByRole("textbox")).toHaveValue("Position 3");
  await run(page);
  const visited = await framesAt(server.url);
  expect(visited).toHaveLength(3);
  visited.forEach((at, i) => {
    expect(Math.abs(at.x - order[i]!.x), `x of ${i}`).toBeLessThan(2.5);
    expect(Math.abs(at.y - order[i]!.y), `y of ${i}`).toBeLessThan(2.5);
  });
  const progress = (await call((signal) => on.api.GET("/api/v1/scan/progress", { signal })))!;
  expect(progress.plan.tiles.map((tile) => tile.name)).toEqual([
    "Position 3",
    "Position 1",
    "Middle",
  ]);
});

test("the positions' list has no violation axe can find, in either theme", async ({
  page,
  server,
}) => {
  test.setTimeout(120_000);
  await ready(page, server.url, "Positions");
  await plan(page).getByRole("button", { name: "Add where the stage is" }).click();
  await expect(said(page, "position-1")).toBeVisible();
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await expectAccessible(page);
  }
});
