// Checkpoint I (W5.4's notes): a slide scanned from the app, on the
// simulator's default slide. Its overview through the 5x, its tissue, the
// regions proposed, one narrowed and the rest dropped, a focus map, the run
// through the 10x, paused and resumed, the map said, and the scan stitched.
// The overview is the tissue's edge and glass, through the camera's small
// region, and the scan its tiny region: a debug server renders each frame.
import { act } from "../../src/api/actions.ts";
import { Server as Api, call } from "../../src/api/client.ts";
import { choose, expect, inScheme, shot, test } from "../e2e/fixtures.ts";
import { typeRegion } from "../e2e/scanning.ts";
import { TINY } from "../e2e/sim.ts";
import { homeAndMove, makeProject, said, stream } from "./steps.ts";

/** The tissue's edge, in µm: in focus there, the overview about it, and a region in it, in mm. */
const AT = { x: 41_400, y: 11_200 };
const OVERVIEW = [41.3, 10.9, 1.0, 0.75];
const NARROWED = [41.35, 11.15, 0.3, 0.2];

test("Checkpoint I: an overview, its tissue and regions, a focus map, a run and its stitch @main", async ({
  page,
  server,
}) => {
  test.setTimeout(600_000);
  const on = new Api(server.url);
  await page.goto(`${server.url}/#/control`);
  // Homed, then at the tissue's edge, focused as by hand at the simulator's truth (G's suite
  // autofocuses); streaming through the 5x.
  const z = (await act("/api/v1/simulation/in_focus", AT, { on })) as number;
  await homeAndMove(page, server.url, { ...AT, z: Math.round(z * 10) / 10 });
  await stream(page, server.url);
  await choose(page, "Fitted", "5x, NA 0.14");

  // The overview of the region typed: the slide, shown.
  await page.getByRole("link", { name: "Scan", exact: true }).click();
  await page.getByRole("tab", { name: "Histo scan" }).click();
  const flow = said(page, "histo");
  await typeRegion(page, "overview-region", OVERVIEW);
  await said(page, "overview").getByRole("button", { name: "Make the overview" }).click();
  await expect(said(page, "overview-said")).toBeVisible({ timeout: 300_000 });
  const next = () => flow.getByRole("button", { name: "Next", exact: true }).click();

  // The tissue: its islands, area and threshold said.
  await next();
  const view = (await call((signal) => on.api.GET("/api/v1/scan/last_overview", { signal })))!;
  // The islands counted in words: one, or a number of them.
  const islands = view.kept === 1 ? "One island" : `${view.kept} islands`;
  const area = view.kept === 1 ? String.raw`\d+\.\d mm²` : String.raw`\d+\.\d mm² in all`;
  await expect(said(page, "tissue")).toHaveText(
    new RegExp(
      String.raw`^${islands} of tissue, ${area}, found at a saturation of 0\.\d\d; \d+ too small, dropped\.$`,
    ),
  );

  // The regions: one about each island; the first narrowed, the rest dropped, drawn fainter.
  await next();
  const regions = said(page, "regions").getByRole("group", { name: /^Region \d+$/ });
  await expect(regions.first()).toBeVisible();
  const count = await regions.count();
  for (let n = 2; n <= count; n++) {
    await said(page, `region-${n}`).getByRole("checkbox", { name: "Keep it" }).click();
  }
  await typeRegion(page, "region-1", NARROWED);
  await expect(said(page, "shapes").locator("polygon.region")).toHaveCount(1);
  await expect(said(page, "shapes").locator("polygon.dropped")).toHaveCount(count - 1);

  // The focus map: 2 by 2; the 10x fitted for the run, and the map's points on the view.
  await next();
  const focus = said(page, "focus");
  for (const label of ["The map's columns", "Its rows"]) {
    await focus.getByLabel(label, { exact: true }).fill("2");
    await focus.getByLabel(label, { exact: true }).press("Tab");
  }
  await page.getByRole("link", { name: "Control", exact: true }).click();
  await choose(page, "Fitted", "10x, NA 0.28");
  // The scan through the camera's tiny region, set while it doesn't stream.
  await act("/api/v1/camera/stop_streaming", {}, { on });
  await stream(page, server.url, TINY);
  await page.getByRole("link", { name: "Scan", exact: true }).click();
  await expect(said(page, "shapes").locator("polygon.point")).toHaveCount(4);

  // The run, into a project made there: the map measured, then the tiles, each marked as
  // taken; paused, the stage held; resumed to its end.
  await next();
  await makeProject(page, "Checkpoint I", "run");
  await said(page, "run").getByRole("button", { name: "Run the scan" }).click();
  const tiles = said(page, "scan-tiles");
  await expect(tiles).toHaveText(/^Running: [1-9]\d* of \d+ tiles$/, { timeout: 300_000 });
  await said(page, "scan").getByRole("button", { name: "Pause" }).click();
  await expect(tiles).toHaveText(/^Paused: /, { timeout: 60_000 });
  const held = (await call((signal) => on.api.GET("/api/v1/scan/progress", { signal })))!;
  await said(page, "scan").getByRole("button", { name: "Resume" }).click();
  await expect(tiles).toHaveText(/^Completed: (\d+) of \1 tiles$/, { timeout: 300_000 });
  const total = held.plan.tiles.length;
  expect(held.done.length).toBeLessThan(total);
  await expect(said(page, "shapes").locator("polygon.done")).toHaveCount(total);

  // The map said: its surface, how well it fits, each point's focus.
  await expect(said(page, "map-said")).toContainText(/A plane through \d+ points/);
  await expect(said(page, "map-said").getByRole("row")).toHaveCount(5);

  // The stitch: its state, then the stitched image to open.
  await said(page, "made").getByRole("button", { name: "Stitch it" }).click();
  await expect(said(page, "stitch")).toContainText("Completed", { timeout: 300_000 });
  await expect(said(page, "stitch").getByRole("link")).toHaveAttribute("href", /#\/gallery\?/);
  await page.mouse.move(5, 5);
  for (const scheme of ["light", "dark"] as const) {
    await inScheme(page, scheme);
    await shot(page, `W10.1-histo-${scheme}`);
  }
});
