// A project's records (W6.2), from a project recorded on the simulator
// (scripts/record-contents.mjs): an acquisition, the result stitched from
// it, and a report. Each listed by kind; its files; its details; and the
// result's provenance, as its page says it. And a download's ranges.
import { describe, expect, it } from "vitest";

import { answered, rangeFrom } from "./download";
import { detailsOf, filesOf, provenanceOf, recordsOf, type Contents, type Result } from "./project";
import recorded from "./recorded-contents.json";

const contents = recorded as unknown as Contents;
const records = recordsOf(contents);
const of = (kind: string) => records.find((one) => one.kind === kind)!;

describe("a project's records", () => {
  it("are listed by kind: acquisitions, results, then reports", () => {
    expect(records.map((one) => one.kind)).toEqual(["acquisition", "result", "report"]);
  });

  it("have their document and thumbnail to download, and a report its own files", () => {
    const a = of("acquisition").record.id;
    expect(filesOf(of("acquisition"))).toEqual([
      `acquisitions/${a}/acquisition.json`,
      `acquisitions/${a}/thumbnail.jpg`,
    ]);
    const report = of("report");
    expect(filesOf(report)).toEqual([
      `reports/${report.record.id}/report.json`,
      ...contents.reports[0]!.files.map((file) => `reports/${report.record.id}/${file}`),
    ]);
  });

  it("say an acquisition's image, camera, objective and the stage's place", () => {
    const details = Object.fromEntries(
      detailsOf(of("acquisition"), "ada").map(({ key, value }) => [key, value]),
    );
    const acquisition = contents.acquisitions[0]!;
    expect(details).toMatchObject({
      account: "ada",
      // A scan's frames are its image's planes.
      size: `${acquisition.image.width} × ${acquisition.image.height} × ${acquisition.image.channels} × ${acquisition.image.planes}, ${acquisition.image.sample}`,
      levels: String(acquisition.image.levels),
      frames: String(acquisition.frames.length),
      calibration: "none",
    });
    expect(details.exposure).toMatch(/^\d+\.\d ms$/);
    expect(details.place).toMatch(/^39\.\d{3}, 11\.\d{3} mm, \d+\.\d µm$/);
  });

  it("leave the owner out when there is none", () => {
    expect(detailsOf(of("report"), "").map(({ key }) => key)).toEqual(["made", "files"]);
  });
});

describe("a result's provenance", () => {
  const result = contents.results[0] as Result;

  it("says its steps, and its input by name, a stitch hashing nothing", () => {
    const said = provenanceOf(result, contents);
    const acquisition = contents.acquisitions[0]!;
    expect(said.inputs).toEqual([
      { kind: "acquisition", id: acquisition.id, name: acquisition.name },
    ]);
    expect(said.nodes).toEqual([`stitch.mosaic ${result.provenance.nodes[0]!.version}`]);
    expect(said.recipe).toBeUndefined();
    expect(said.software).toMatch(/^OScope /);
  });

  it("says an input's hash, where one was read", () => {
    const hashed = {
      ...result,
      provenance: { ...result.provenance, hashes: { [contents.acquisitions[0]!.id]: "sha256:ab" } },
    };
    expect(provenanceOf(hashed, contents).inputs[0]!.hash).toBe("sha256:ab");
  });

  it("says an input the project no longer holds by its ID", () => {
    const gone = { ...contents, acquisitions: [] };
    const [input] = provenanceOf(result, gone).inputs;
    expect(input!.name).toBeUndefined();
    expect(input!.id).toBe(contents.acquisitions[0]!.id);
  });
});

describe("a download", () => {
  it("asks for the rest of a file, once some came", () => {
    expect(rangeFrom(0)).toEqual({});
    expect(rangeFrom(1234)).toEqual({ range: "bytes=1234-" });
  });

  it("reads the whole length from the range sent, or else the length", () => {
    const headers = (entries: Record<string, string>) => new Headers(entries);
    expect(
      answered(206, headers({ "content-range": "bytes 100-199/200", "content-length": "100" })),
    ).toEqual({
      ranged: true,
      of: 200,
    });
    // The whole file again, the range ignored.
    expect(answered(200, headers({ "content-length": "200" }))).toEqual({ ranged: false, of: 200 });
    expect(answered(200, headers({}))).toEqual({ ranged: false });
    // A 200 is the whole file, whatever range it says.
    expect(
      answered(200, headers({ "content-range": "bytes 0-199/200", "content-length": "200" })),
    ).toEqual({
      ranged: false,
      of: 200,
    });
  });
});
