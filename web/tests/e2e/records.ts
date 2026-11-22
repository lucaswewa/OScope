// Records made for the tests by the API (W6.3, W6.4): a project, and a file
// uploaded and imported into it as an acquisition.
import { act } from "../../src/api/actions.ts";
import { Server as Api } from "../../src/api/client.ts";
import { finished } from "../../src/api/jobs.ts";

/** A project made: its ID. */
export async function made(url: string, name: string): Promise<string> {
  return (await act("/api/v1/projects/create", { name }, { on: new Api(url) })).id;
}

/** `bytes` uploaded as `name` and imported into `project`: the acquisition's ID. */
export async function imported(
  url: string,
  project: string,
  name: string,
  bytes: Buffer,
): Promise<string> {
  const answer = await fetch(new URL(`/api/v1/upload/${encodeURIComponent(name)}`, url), {
    method: "POST",
    body: new Uint8Array(bytes),
  });
  if (answer.status !== 201) throw new Error(`the upload: ${answer.status} ${await answer.text()}`);
  const staged = (await answer.json()) as { id: string };
  const on = new Api(url);
  const params = { project, upload: staged.id };
  const submitted = await act("/api/v1/jobs/submit", { kind: "projects.import", params }, { on });
  const ended = await finished(submitted, { on, timeout: 120_000 });
  return /acquisitions\/([^/]+)$/.exec(ended.results[0]!)![1]!;
}
