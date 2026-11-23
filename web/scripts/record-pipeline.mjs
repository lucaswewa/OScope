// The node catalogue and the stock recipes, recorded for the graph editor's
// unit tests (W7.2), from a server of the script's own (tests/e2e/server.ts).
//
//   node scripts/record-pipeline.mjs    (after `cargo build -p oscope-server`)
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { Server } from "../tests/e2e/server.ts";

const OUT = fileURLToPath(
  new URL("../src/features/workspace/editor/recorded-pipeline.json", import.meta.url),
);

const server = new Server();
await server.start();

/**
 * What the server answers, as JSON: anything but a success fails.
 * @param {string} path
 */
async function read(path) {
  const reply = await fetch(new URL(`/api/v1/${path}`, server.url));
  if (!reply.ok) throw new Error(`${path}: ${reply.status} ${await reply.text()}`);
  return reply.json();
}

try {
  const nodes = await read("pipeline/nodes");
  /** @type {Record<string, unknown>} */
  const recipes = {};
  for (const listed of await read("pipeline/recipes")) {
    if (listed.stock) recipes[listed.name] = (await read(`pipeline/recipe/${listed.name}`)).recipe;
  }
  // Generated, on one line: Prettier leaves it be (.prettierignore).
  writeFileSync(OUT, `${JSON.stringify({ nodes, recipes })}\n`);
  console.log(
    `record-pipeline: wrote ${OUT}: ${nodes.length} nodes, ${Object.keys(recipes).length} recipes`,
  );
} finally {
  await server.dispose();
}
