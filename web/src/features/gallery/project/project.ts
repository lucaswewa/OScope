// A project's records (W6.2): what `projects/project/{id}` says of them,
// listed by kind; each one's details, its files to download, and a result's
// provenance. That route's answer isn't in the OpenAPI document: its shape
// is written here, from the store's records (P28, P36, P48).

/** A record's image, as the store writes it. */
export interface ImageInfo {
  width: number;
  height: number;
  channels: number;
  planes: number;
  sample: string;
  pixel_size?: { x: number; y: number; z?: number | null } | null;
  levels: number;
}

/** What made a result: an acquisition, or another result. */
export interface Input {
  kind: "acquisition" | "result";
  id: string;
}

/** How a result was made (P48). */
export interface Provenance {
  recipe: unknown;
  nodes: { node: string; version: string; extension?: string | null }[];
  inputs: Input[];
  /** What it read of its inputs, hashed: none for a node that hashes nothing, such as a stitch. */
  hashes?: Record<string, string>;
  software: string;
}

interface Common {
  id: string;
  project: string;
  name: string;
  created: string;
}

export interface Acquisition extends Common {
  image: ImageInfo;
  instrument: unknown;
  objective: unknown;
  calibration: unknown;
  camera: Record<string, unknown> | null;
  plan: unknown;
  frames: { index: number; position?: { x: number; y: number; z: number } | null }[];
}

export interface Result extends Common {
  image?: ImageInfo | null;
  provenance: Provenance;
}

export interface Report extends Common {
  files: string[];
}

/** A project, with its records. */
export interface Contents {
  project: { id: string; name: string; owner: string; sample: string };
  acquisitions: Acquisition[];
  results: Result[];
  reports: Report[];
}

export type Kind = "acquisition" | "result" | "report";
export type AnyRecord =
  | { kind: "acquisition"; record: Acquisition }
  | { kind: "result"; record: Result }
  | { kind: "report"; record: Report };

const FOLDERS: Record<Kind, string> = {
  acquisition: "acquisitions",
  result: "results",
  report: "reports",
};

/** The project's records, by kind, each kind's newest first. */
export function recordsOf(contents: Contents): AnyRecord[] {
  const newest = <T extends Common>(list: T[]) =>
    [...list].sort((a, b) => b.created.localeCompare(a.created));
  return [
    ...newest(contents.acquisitions).map((record) => ({ kind: "acquisition" as const, record })),
    ...newest(contents.results).map((record) => ({ kind: "result" as const, record })),
    ...newest(contents.reports).map((record) => ({ kind: "report" as const, record })),
  ];
}

/**
 * A record's files that download whole, by their paths in the project: its
 * document and its thumbnail, and a report's own files. Its image is a
 * folder, OME-Zarr's, which an export makes one file of (W6.3).
 */
export function filesOf(one: AnyRecord): string[] {
  const folder = `${FOLDERS[one.kind]}/${one.record.id}`;
  const own = one.kind === "report" ? one.record.files.map((file) => `${folder}/${file}`) : [];
  const thumbnail = one.kind === "report" ? [] : [`${folder}/thumbnail.jpg`];
  return [`${folder}/${one.kind}.json`, ...thumbnail, ...own];
}

/** A detail of a record: its term's key, and its value, as it will be said. */
export interface Detail {
  key: string;
  value: string;
}

const number = (value: unknown, digits = 0) =>
  typeof value === "number" && Number.isFinite(value) ? value.toFixed(digits) : undefined;

/** An image's size: its pixels, channels and planes, its sample, its pixel, its levels. */
function imageDetails(image: ImageInfo): Detail[] {
  const pixel = image.pixel_size;
  return [
    {
      key: "size",
      value: `${image.width} × ${image.height} × ${image.channels}${image.planes > 1 ? ` × ${image.planes}` : ""}, ${image.sample}`,
    },
    ...(pixel ? [{ key: "pixel", value: `${number(pixel.x, 3)} µm` }] : []),
    { key: "levels", value: String(image.levels) },
  ];
}

/**
 * A record's details: when it was made, and whose; its image's size and
 * levels; an acquisition's camera, objective, calibration and the stage's
 * place at its first frame; a result's software.
 */
export function detailsOf(one: AnyRecord, owner: string): Detail[] {
  const made = [
    { key: "made", value: one.record.created },
    ...(owner ? [{ key: "account", value: owner }] : []),
  ];
  switch (one.kind) {
    case "acquisition": {
      const { camera, objective, calibration, frames, image } = one.record;
      const first = frames[0]?.position;
      const settings = camera
        ? [
            ["exposure", `${number(camera.exposure_ms, 1)} ms`],
            ["gain", number(camera.gain, 1)],
            ["binning", number(camera.binning)],
            ["mode", typeof camera.mode === "string" ? camera.mode : undefined],
          ]
            .filter(([, value]) => value !== undefined && !String(value).startsWith("undefined"))
            .map(([key, value]) => ({ key: key!, value: value! }))
        : [];
      return [
        ...made,
        ...imageDetails(image),
        { key: "frames", value: String(frames.length) },
        ...settings,
        ...(typeof objective === "string" ? [{ key: "objective", value: objective }] : []),
        {
          key: "calibration",
          value: calibration && typeof calibration === "object" ? "set" : "none",
        },
        ...(first
          ? [
              {
                key: "place",
                value: `${(first.x / 1000).toFixed(3)}, ${(first.y / 1000).toFixed(3)} mm, ${first.z.toFixed(1)} µm`,
              },
            ]
          : []),
      ];
    }
    case "result":
      return [
        ...made,
        ...(one.record.image ? imageDetails(one.record.image) : []),
        { key: "software", value: one.record.provenance.software },
      ];
    case "report":
      return [...made, { key: "files", value: String(one.record.files.length) }];
  }
}

/** A result's provenance, as its page says it (P48). */
export interface ProvenanceSaid {
  /** The recipe's name, if it has one. */
  recipe?: string;
  /** Each node, its version, and its extension, if one gives it. */
  nodes: string[];
  /** What it was made from: each input, by name if the project holds it, and its hash. */
  inputs: { kind: "acquisition" | "result"; id: string; name?: string; hash?: string }[];
  software: string;
}

/** How a result was made, for its page. */
export function provenanceOf(result: Result, contents: Contents): ProvenanceSaid {
  const { recipe, nodes, inputs, hashes, software } = result.provenance;
  const named = (kind: "acquisition" | "result", id: string) =>
    (kind === "acquisition" ? contents.acquisitions : contents.results).find((r) => r.id === id)
      ?.name;
  const name =
    recipe && typeof recipe === "object" && "name" in recipe && typeof recipe.name === "string"
      ? recipe.name
      : undefined;
  return {
    ...(name ? { recipe: name } : {}),
    nodes: nodes.map((n) => `${n.node} ${n.version}${n.extension ? ` (${n.extension})` : ""}`),
    inputs: inputs.map(({ kind, id }) => {
      const name = named(kind, id);
      const hash = hashes?.[id];
      return { kind, id, ...(name ? { name } : {}), ...(hash ? { hash } : {}) };
    }),
    software,
  };
}
