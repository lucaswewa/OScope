// A project to report on (W7.9): W7.4's discs counted, and W7.6's slide's
// stains separated and DAB measured, by the API, as the Workspace would.
import { Server as Api } from "../../src/api/client.ts";
import { finished } from "../../src/api/jobs.ts";
import { recipeNamed } from "../../src/api/pipeline.ts";
import { analyse, type Analysis } from "../../src/api/workspace.ts";
import { counting, prepared, stained } from "./analyses.ts";
import { imported } from "./records.ts";

/** The project *Discs*, its discs counted and its slide's stains measured: their results' IDs. */
export async function counted(
  url: string,
): Promise<{ project: string; count: string; stains: string }> {
  const on = new Api(url);
  const { project, acquisition } = await prepared(url);
  const slide = await imported(url, project, "hdab.tif", stained());
  const run = async (recipe: Analysis["recipe"], id: string) => {
    const input = { kind: "acquisition" as const, id };
    const ended = await finished(await analyse(project, { recipe, input }, { on }), {
      on,
      timeout: 60_000,
    });
    return ended.results[0]!.split("/").pop()!;
  };
  const count = await run(counting(10), acquisition);
  const stains = await run((await recipeNamed("stains-hdab", { on })).recipe, slide);
  return { project, count, stains };
}
