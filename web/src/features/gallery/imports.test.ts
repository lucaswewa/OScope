// Files into a project and out of it (W6.3): the files the importers read,
// and the chooser's types; the record a job made, and the file an export
// wrote; and an import's way, uploaded, imported, refused, stopped or
// failed, on a stand-in server.
import { beforeEach, describe, expect, it, vi } from "vitest";

import { ApiError } from "@/api/errors";
import type { Format } from "@/api/projects";

const server = vi.hoisted(() => ({
  upload: vi.fn(),
  act: vi.fn(),
  finished: vi.fn(),
}));
vi.mock("@/api/projects", () => ({ upload: server.upload, projectFormats: vi.fn() }));
vi.mock("@/api/actions", () => ({ act: server.act }));
vi.mock("@/api/jobs", () => ({ finished: server.finished }));

const {
  acceptOf,
  clearImports,
  exportedOf,
  exportersOf,
  importFiles,
  importsOf,
  madeBy,
  readable,
  stopImport,
} = await import("./imports");

const importers: Format[] = [
  { id: "tiff", name: "TIFF", extensions: ["tif", "tiff"] },
  { id: "png", name: "PNG", extensions: ["png"] },
  { id: "an extension's", name: "Also TIFF", extensions: ["TIF"] },
];
const file = (name: string, size = 10) => new File([new Uint8Array(size)], name);

describe("the files an import takes", () => {
  it("are the importers' extensions, each once, as a chooser takes them", () => {
    expect(acceptOf(importers)).toEqual([".tif", ".tiff", ".png"]);
    expect(acceptOf([])).toEqual([]);
  });

  it("are those whose names end in one, in any case", () => {
    expect(readable("Slide 1.TIF", importers)).toBe(true);
    expect(readable("stack.ome.tiff", importers)).toBe(true);
    expect(readable("notes.txt", importers)).toBe(false);
    expect(readable("tif", importers)).toBe(false);
    expect(readable("photo.png", [])).toBe(false);
  });
});

describe("what a job made", () => {
  it("is the record its results name, of the kind asked", () => {
    const job = { results: ["projects/p/results/r1", "projects/p/acquisitions/a1"] };
    expect(madeBy(job)).toBe("a1");
    expect(madeBy(job, "results")).toBe("r1");
    expect(madeBy({ results: ["data/p/exports/x.png"] })).toBeUndefined();
  });

  it("is, for an export, the file in its project", () => {
    const job = { results: ["data/p/acquisitions/a1/exports/a1.ome.tif"] };
    expect(exportedOf(job, "p")).toBe("acquisitions/a1/exports/a1.ome.tif");
    expect(exportedOf(job, "q")).toBeUndefined();
  });

  it("is by an exporter that writes a file: a deep zoom folder isn't one", () => {
    const formats = {
      importers: [],
      exporters: [
        { id: "ome-tiff", name: "OME-TIFF", extensions: ["ome.tif"] },
        { id: "dzi", name: "Deep Zoom folder (DZI)", extensions: ["dzi"] },
      ],
    };
    expect(exportersOf(formats).map((one) => one.id)).toEqual(["ome-tiff"]);
    expect(exportersOf(undefined)).toEqual([]);
  });
});

describe("an import", () => {
  beforeEach(() => {
    server.upload.mockReset();
    server.act.mockReset().mockResolvedValue({ id: "job", href: "/api/v1/jobs/job/job" });
    server.finished.mockReset().mockResolvedValue({ results: ["projects/p/acquisitions/a1"] });
  });

  it("is uploaded with its progress, then imported by a job, which made its acquisition", async () => {
    server.upload.mockImplementation(
      async (_file: File, options: { progress: (now: { done: number; of: number }) => void }) => {
        options.progress({ done: 4, of: 10 });
        options.progress({ done: 10, of: 10 });
        return { id: "staged" };
      },
    );
    importFiles("uploaded", [file("slide.tif")], importers);
    const [one] = importsOf("uploaded");
    expect(one).toMatchObject({ name: "slide.tif", state: "uploading", of: 10 });
    await vi.waitFor(() => expect(importsOf("uploaded")[0]!.state).toBe("imported"));
    expect(importsOf("uploaded")[0]).toMatchObject({ done: 10, of: 10, acquisition: "a1" });
    expect(server.act).toHaveBeenCalledWith("/api/v1/jobs/submit", {
      kind: "projects.import",
      params: { project: "uploaded", upload: "staged" },
    });
  });

  it("is refused, never sent, when no importer reads its file", () => {
    importFiles("refused", [file("notes.txt")], importers);
    expect(importsOf("refused")[0]!.state).toBe("refused");
    expect(server.upload).not.toHaveBeenCalled();
  });

  it("stops when its upload is stopped", async () => {
    server.upload.mockImplementation(
      (_file: File, options: { signal: AbortSignal }) =>
        new Promise((_done, failed) =>
          options.signal.addEventListener("abort", () =>
            failed(new ApiError("aborted", "the call was stopped")),
          ),
        ),
    );
    importFiles("stopped", [file("slide.tif")], importers);
    stopImport(importsOf("stopped")[0]!.key);
    await vi.waitFor(() => expect(importsOf("stopped")[0]!.state).toBe("stopped"));
    expect(server.act).not.toHaveBeenCalled();
  });

  it("fails, saying why, when its job does", async () => {
    server.upload.mockResolvedValue({ id: "staged" });
    server.finished.mockRejectedValue(new ApiError("failed", "not a TIFF the importer reads"));
    importFiles("failed", [file("slide.tif")], importers);
    await vi.waitFor(() => expect(importsOf("failed")[0]!.state).toBe("failed"));
    expect(importsOf("failed")[0]!.why).toBe("not a TIFF the importer reads");
  });

  it("is forgotten once it ended, if its user clears them, but not while it goes", async () => {
    server.upload.mockReturnValue(new Promise(() => {}));
    importFiles("cleared", [file("going.tif"), file("notes.txt")], importers);
    clearImports("cleared");
    expect(importsOf("cleared").map((one) => one.name)).toEqual(["going.tif"]);
    expect(importsOf("uploaded")).toHaveLength(1);
  });
});
