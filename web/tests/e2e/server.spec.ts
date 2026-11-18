import { join } from "node:path";

import { expect, test } from "./fixtures.ts";

// The tests' own footing: what lets them run side by side, on any computer.
test("a test's server keeps its files in its own folder, and can't turn the computer off", async ({
  request,
  server,
}) => {
  const ask = async (property: string): Promise<unknown> => {
    const answer = await request.get(`${server.url}/api/v1/${property}`);
    expect(answer.status(), property).toBe(200);
    return answer.json();
  };
  for (const [property, folder] of [
    ["system/data_root", "data"],
    ["system/scratch_root", "scratch"],
  ] as const) {
    expect(await ask(property), property).toMatchObject({ path: join(server.folder, folder) });
  }
  expect(await ask("power/dry_run")).toBe(true);
});
