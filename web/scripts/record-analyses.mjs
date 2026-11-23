// Two results compared, recorded for the Workspace's unit tests (W7.4), from
// a server of the script's own (tests/e2e/server.ts): the discs counted from
// 10 µm², then from 300 µm², and the first made again; the differences of
// the first with each, and the three's summaries.
//
//   node --experimental-transform-types scripts/record-analyses.mjs
//
// after `cargo build -p oscope-server`. The client's TypeScript isn't only
// types, which Node's default reads.
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server as Api } from "../src/api/client.ts";
import { finished } from "../src/api/jobs.ts";
import { analyse, difference, rerun, summaryOf } from "../src/api/workspace.ts";
import { counting, prepared } from "../tests/e2e/analyses.ts";
import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(
  new URL("../src/features/workspace/analyses/recorded-analyses.json", import.meta.url),
);

const server = new Server();
await server.start();
try {
  const on = new Api(server.url);
  const { project, acquisition } = await prepared(server.url);
  /** @type {{ kind: "acquisition", id: string }} */
  const input = { kind: "acquisition", id: acquisition };
  /**
   * A job's result's ID.
   * @param {import("../src/api/jobs.ts").JobRef} job
   * @returns {Promise<string>}
   */
  const made = async (job) => {
    const [path] = (await finished(job, { on, timeout: 120_000 })).results;
    if (!path) throw new Error("the job made no result");
    return path.slice(path.lastIndexOf("/") + 1);
  };
  const a = await made(await analyse(project, { recipe: counting(10), input }, { on }));
  const b = await made(await analyse(project, { recipe: counting(300), input }, { on }));
  const again = await made(await rerun(project, a, {}, { on }));
  const recorded = {
    changed: await difference(project, a, b, { on }),
    again: await difference(project, a, again, { on }),
    summaries: {
      a: await summaryOf(project, a, { on }),
      b: await summaryOf(project, b, { on }),
      again: await summaryOf(project, again, { on }),
    },
  };
  // Generated, on one line: Prettier leaves it be (.prettierignore).
  writeFileSync(OUT, `${JSON.stringify(recorded)}\n`);
  console.log(`record-analyses: wrote ${OUT}`);
} finally {
  await server.dispose();
}
