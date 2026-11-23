// Reports (P47, W7.9): a report's job asked for as the server takes it; a
// template read by its name, kept, deleted and previewed, its PDF from
// base64. The server is stood in for here; the end-to-end tests
// (tests/e2e/report.spec.ts) use a real one.
import { afterEach, describe, expect, it, vi } from "vitest";

import { Server } from "./client.ts";
import {
  deleteTemplate,
  makeReport,
  previewTemplate,
  saveTemplate,
  templateNamed,
  templateNames,
} from "./reports.ts";

const on = new Server("http://127.0.0.1:8211");
const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });

/** A server whose actions complete at once with `output`, and whose reads answer `read`. */
function serving(output: unknown, read: unknown = null) {
  const made: { method: string; path: string; body: unknown }[] = [];
  vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
    const request = input instanceof Request ? input : new Request(input, init);
    const path = new URL(request.url).pathname;
    const body = request.method === "POST" ? await request.json() : undefined;
    made.push({ method: request.method, path, body });
    if (request.method !== "POST") return json(read);
    const id = "a1b2c3d4-0000-4000-8000-000000000001";
    const href = `http://127.0.0.1:8211/api/v1/action_invocations/${id}`;
    return json({ status: "completed", id, href, input: body, output, error: null, log: [] }, 201);
  });
  return made;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("reports", () => {
  it("are made by a job on the results chosen, through a template, in its forms", async () => {
    const ref = { id: "j1", href: "/api/v1/jobs/job/j1" };
    const made = serving(ref);
    const making = { results: ["r1", "r2"], template: "mine", formats: ["pdf" as const] };
    expect(await makeReport("p1", making, { on })).toEqual(ref);
    expect(made).toEqual([
      {
        method: "POST",
        path: "/api/v1/jobs/submit",
        body: {
          kind: "report.make",
          params: {
            project: "p1",
            options: { params: { results: ["r1", "r2"] }, template: "mine", formats: ["pdf"] },
          },
        },
      },
    ]);
  });
});

describe("a template", () => {
  it("is listed, and read by its name", async () => {
    const made = serving(null, ["standard", "our lab"]);
    expect(await templateNames({ on })).toEqual(["standard", "our lab"]);
    // A name the path must hold as one part: encoded, a slash and all.
    await templateNamed("our lab/2", { on });
    expect(made.map((one) => one.path)).toEqual([
      "/api/v1/projects/templates",
      "/api/v1/projects/template/our%20lab%2F2",
    ]);
  });

  it("is kept with its sources alone, and deleted by its name", async () => {
    const made = serving(null);
    const template = { name: "mine", typst: "= T", html: "<h1>T</h1>", builtin: false };
    await saveTemplate(template, { on });
    await deleteTemplate("mine", { on });
    expect(made.map(({ path, body }) => [path, body])).toEqual([
      ["/api/v1/projects/save_template", { name: "mine", typst: "= T", html: "<h1>T</h1>" }],
      ["/api/v1/projects/delete_template", { name: "mine" }],
    ]);
  });

  it("is previewed on a result or on the sample, its PDF's bytes from base64", async () => {
    // "%PDF-1" in base64.
    const made = serving({ pdf: "JVBERi0x", html: "<h1>T</h1>" });
    const sources = { typst: "= T", html: "<h1>T</h1>" };
    const shown = await previewTemplate(sources, { project: "p1", result: "r1" }, { on });
    expect(new TextDecoder().decode(shown.pdf)).toBe("%PDF-1");
    expect(shown.html).toBe("<h1>T</h1>");
    await previewTemplate(sources, undefined, { on });
    expect(made.map((one) => one.body)).toEqual([
      { ...sources, project: "p1", result: "r1" },
      sources,
    ]);
  });
});
