// The media plane (ADR-0006): OScope's own routes beside the Things, which
// the OpenAPI document doesn't describe. Each is typed here by hand, and
// checked against a real server by the contract tests
// (tests/e2e/contract.spec.ts): deep-zoom tiles (ADR-0028), the live
// mosaic's (P38), volumes (ADR-0029), a project's files (P30), and the logs'
// live tail.
import { fetched, server, type Limits, type Server } from "./client.ts";
import { ApiError } from "./errors.ts";

type On = Limits & { on?: Server | undefined };
type Triple = [number, number, number];

/** A stored image: one of a project's acquisitions or results. */
export interface StoredImage {
  project: string;
  kind: "acquisitions" | "results";
  id: string;
}

const pathOf = (image: StoredImage) =>
  `${encodeURIComponent(image.project)}/${image.kind}/${encodeURIComponent(image.id)}`;

/** A query's text, `?a=1&b=2`, from what is set. */
function queryOf(values: Record<string, string | number | undefined>): string {
  const query = new URLSearchParams();
  for (const [name, value] of Object.entries(values)) {
    if (value !== undefined) query.set(name, String(value));
  }
  const text = query.toString();
  return text === "" ? "" : `?${text}`;
}

// Deep zoom.

/** An image's Deep Zoom descriptor, `image.dzi`. */
export interface ZoomDescriptor {
  width: number;
  height: number;
  tileSize: number;
  overlap: number;
  /** The tiles' format: `jpg`. */
  format: string;
}

/** A descriptor's attributes, read without a DOM, which a worker lacks. */
export function descriptorOf(xml: string): ZoomDescriptor {
  const attribute = (element: string, name: string): string => {
    const found = new RegExp(`<${element}\\b[^>]*\\b${name}="([^"]*)"`).exec(xml);
    if (!found) throw new ApiError("server", `the descriptor has no ${element} ${name}`);
    return found[1]!;
  };
  return {
    width: Number(attribute("Size", "Width")),
    height: Number(attribute("Size", "Height")),
    tileSize: Number(attribute("Image", "TileSize")),
    overlap: Number(attribute("Image", "Overlap")),
    format: attribute("Image", "Format"),
  };
}

/**
 * How an image is drawn: its plane, `black` and `white` from 0 to 1 of its
 * samples' range, and `gamma`. Unset, as its thumbnail is.
 */
export interface Look {
  plane?: number;
  /** One channel alone, as grey: a stain's amount (W7.6). */
  channel?: number;
  black?: number;
  white?: number;
  gamma?: number;
}

/** An image's pyramid, its look and the server's caches. */
export interface ZoomStatus {
  size: [number, number];
  levels: number;
  planes: number;
  raw: boolean;
  /** A raw image's colour overview: `not built`, or being built, or built. */
  overview: string;
  coarse: boolean;
  /** The threads making tiles now. */
  threads: number;
  look: { black: number | null; white: number | null; gamma: number | null };
  chunks: { hits: number; misses: number };
  tiles: { hits: number; misses: number; kept: number; bytes: number };
}

/** An image's descriptor. */
export function zoomDescriptor(image: StoredImage, options: On = {}): Promise<ZoomDescriptor> {
  const address = (options.on ?? server).address(`/api/v1/zoom/${pathOf(image)}/image.dzi`);
  return fetched(address, async (answer) => descriptorOf(await answer.text()), options);
}

/** An image's pyramid, look and caches. */
export function zoomStatus(image: StoredImage, options: On = {}): Promise<ZoomStatus> {
  const address = (options.on ?? server).address(`/api/v1/zoom/${pathOf(image)}/status`);
  return fetched(address, (answer) => answer.json() as Promise<ZoomStatus>, options);
}

/** A tile's address, for an image element or a viewer, as Deep Zoom numbers them. */
export function tileAddress(
  image: StoredImage,
  tile: { level: number; col: number; row: number },
  look: Look = {},
  on: Server = server,
): string {
  const { level, col, row } = tile;
  const path = `/api/v1/zoom/${pathOf(image)}/image_files/${level}/${col}_${row}.jpg`;
  return on.address(path) + queryOf({ ...look });
}

/** A tile's picture. */
export function tile(
  image: StoredImage,
  which: { level: number; col: number; row: number },
  options: On & { look?: Look } = {},
): Promise<Blob> {
  const address = tileAddress(image, which, options.look, options.on);
  return fetched(address, (answer) => answer.blob(), options);
}

// Volumes.

/** How each sample is stored. */
export type Sample = "u8" | "u16" | "f32";

/** A volume's size, samples and levels. */
export interface VolumeMeta {
  /** In voxels: x, y, z. */
  size: Triple;
  channels: number;
  sample: Sample;
  /** A voxel's size in µm, if known. */
  voxel_um: Triple | null;
  /** Whether its planes are a volume, rather than a stack of separate planes. */
  volume: boolean;
  raw: boolean;
  /** Its coarser levels, each half the one before. */
  levels: { level: number; size: Triple; voxel_um: Triple | null }[];
  /** The length of a slice's or a brick's header, in bytes. */
  header: number;
}

/** A slice's or a brick's voxels, x fastest, then y, then z, a voxel's channels together. */
export interface Voxels {
  level: number;
  /** In the level's voxels: x, y, z. */
  size: Triple;
  /** Where they start, in the level's voxels. */
  origin: Triple;
  /** A voxel's size in µm, if known. */
  voxelUm: Triple | null;
  channels: number;
  samples: Uint8Array | Uint16Array | Float32Array;
}

const SAMPLES = { 1: Uint8Array, 2: Uint16Array, 4: Float32Array } as const;

/**
 * A slice or a brick, from its header (ADR-0029): `OSV1`, the header's
 * length, the sample's bytes and the channels, the level, then the size,
 * the origin and the voxel's size, each x, y and z; then the voxels,
 * little-endian, as this computer's typed arrays are.
 */
export function voxelsOf(buffer: ArrayBuffer): Voxels {
  const view = new DataView(buffer);
  const magic = String.fromCharCode(...new Uint8Array(buffer, 0, Math.min(4, buffer.byteLength)));
  if (magic !== "OSV1") throw new ApiError("server", `these aren't voxels: they start ${magic}`);
  const header = view.getUint16(4, true);
  const bytes = view.getUint8(6);
  const channels = view.getUint8(7);
  const three = (at: number, read: (offset: number) => number): Triple => [
    read(at),
    read(at + 4),
    read(at + 8),
  ];
  const size = three(12, (offset) => view.getUint32(offset, true));
  const voxel = three(36, (offset) => view.getFloat32(offset, true));
  const Samples = SAMPLES[bytes as keyof typeof SAMPLES];
  if (!Samples) throw new ApiError("server", `a sample of ${bytes} bytes isn't known`);
  const count = size[0] * size[1] * size[2] * channels;
  if (buffer.byteLength < header + count * bytes) {
    throw new ApiError("server", "the voxels are cut short");
  }
  return {
    level: view.getUint32(8, true),
    size,
    origin: three(24, (offset) => view.getUint32(offset, true)),
    voxelUm: voxel.every((part) => part === 0) ? null : voxel,
    channels,
    samples: new Samples(buffer, header, count),
  };
}

const voxels = (answer: Response) => answer.arrayBuffer().then(voxelsOf);

/** A stored image's planes as a volume: its size, samples and levels. */
export function volumeMeta(image: StoredImage, options: On = {}): Promise<VolumeMeta> {
  const address = (options.on ?? server).address(`/api/v1/volume/${pathOf(image)}/meta`);
  return fetched(address, (answer) => answer.json() as Promise<VolumeMeta>, options);
}

/** An orthogonal slice, at a level, its index in that level's voxels. */
export function slice(
  image: StoredImage,
  which: { axis: "x" | "y" | "z"; index: number; level?: number; format?: Sample },
  options: On = {},
): Promise<Voxels> {
  const path = `/api/v1/volume/${pathOf(image)}/slice${queryOf(which)}`;
  return fetched((options.on ?? server).address(path), voxels, options);
}

/**
 * A box of voxels, given in full-size voxels (the whole volume by default),
 * at `level`, or at the finest whose part holds `max` voxels or fewer.
 */
export function brick(
  image: StoredImage,
  box: {
    x?: number;
    y?: number;
    z?: number;
    w?: number;
    h?: number;
    d?: number;
    level?: number;
    max?: number;
    format?: Sample;
  } = {},
  options: On = {},
): Promise<Voxels> {
  const path = `/api/v1/volume/${pathOf(image)}/brick${queryOf(box)}`;
  return fetched((options.on ?? server).address(path), voxels, options);
}

// The live mosaic's tiles (W3.8).

/**
 * A tile of the live mosaic (P38), at the server's level, 0 the finest: its
 * JPEG, or nothing where nothing has been drawn (404).
 */
export async function mosaicTile(
  level: number,
  col: number,
  row: number,
  options: On = {},
): Promise<Blob | undefined> {
  const path = `/api/v1/mosaic/tiles/${level}/${col}/${row}`;
  try {
    return await fetched((options.on ?? server).address(path), (answer) => answer.blob(), options);
  } catch (thrown) {
    if (thrown instanceof ApiError && thrown.failure === "missing") return undefined;
    throw thrown;
  }
}

// A project's files.

/** A file's address in a project: its path's parts, each a plain name. */
export function fileAddress(project: string, path: string, on: Server = server): string {
  const parts = path.split("/").filter((part) => part !== "");
  return on.address(`/api/v1/data/${[project, ...parts].map(encodeURIComponent).join("/")}`);
}

/** A file, or part of it. */
export interface Download {
  bytes: ArrayBuffer;
  type: string;
  /** The bytes' place in the file, when a range was asked for. */
  range?: { from: number; to: number; of: number };
}

/** A file of a project, whole, or the bytes from `from` to `to`. */
export function download(
  project: string,
  path: string,
  options: On & { range?: { from: number; to?: number } } = {},
): Promise<Download> {
  const init: RequestInit = {};
  if (options.range) {
    init.headers = { range: `bytes=${options.range.from}-${options.range.to ?? ""}` };
  }
  return fetched(
    fileAddress(project, path, options.on),
    async (answer) => {
      const got: Download = {
        bytes: await answer.arrayBuffer(),
        type: answer.headers.get("content-type") ?? "",
      };
      const range = /^bytes (\d+)-(\d+)\/(\d+)$/.exec(answer.headers.get("content-range") ?? "");
      if (answer.status === 206 && range) {
        got.range = { from: Number(range[1]), to: Number(range[2]), of: Number(range[3]) };
      }
      return got;
    },
    options,
    init,
  );
}

// The logs' live tail.

/** How serious a record is. */
export type Level = "trace" | "debug" | "info" | "warn" | "error" | "critical";

/** A record of the server's log. */
export interface LogRecord {
  /** Its number: records are numbered from 1, in the order they are made. */
  seq: number;
  time: string;
  level: Level;
  /** Where it comes from: a module's path, `oscope_api::server`. */
  target: string;
  message: string;
  fields?: Record<string, unknown>;
  /** The job it was made in. */
  job?: string;
}

/** Which records the tail sends; every condition is optional. */
export interface LogQuery {
  /** The lowest level. */
  level?: Level;
  since?: string;
  until?: string;
  /** The target starts with this. */
  target?: string;
  /** The message or a field contains this, ignoring case. */
  text?: string;
  job?: string;
  /** How many of the newest records it sends first: 100 by default. */
  limit?: number;
}

/** A query as the tail tells it back: each condition, null if unset. */
export type TailQuery = { [Condition in keyof LogQuery]?: LogQuery[Condition] | null };

/**
 * What the tail sends: the query it now follows, then the newest records
 * that match, `caught_up`, and each new one as it is made. `skipped` says
 * how many a client that fell behind missed; `error` answers a wrong query.
 */
export type TailMessage =
  | { query: TailQuery }
  | { record: LogRecord }
  | { caught_up: number }
  | { skipped: number }
  | { error: string };

/** A daily file of the logs, whole (`logs/files/{name}`), to save. */
export function logFile(name: string, options: On = {}): Promise<Blob> {
  const path = `/api/v1/logs/files/${encodeURIComponent(name)}`;
  return fetched((options.on ?? server).address(path), (answer) => answer.blob(), options);
}

/** A page of the logs' records, the newest first, and the `before` of the next, older page. */
export interface LogPage {
  records: LogRecord[];
  next?: number;
}

/** A page of the records that `query` takes, the newest first (`logs/records`). */
export function logRecords(
  query: LogQuery & { before?: number } = {},
  options: On = {},
): Promise<LogPage> {
  const search = new URLSearchParams(
    Object.entries(query).map(([key, value]) => [key, String(value)]),
  ).toString();
  const path = `/api/v1/logs/records${search ? `?${search}` : ""}`;
  return fetched((options.on ?? server).address(path), (answer) => answer.json(), options);
}

/**
 * The logs' live tail: each message, to `told`, until `signal` aborts or
 * the server ends it. `ask` changes the query; the tail then starts again
 * from the records that match it.
 */
export function logTail(
  told: (message: TailMessage) => void,
  options: { on?: Server; signal?: AbortSignal; query?: LogQuery } = {},
): { ask: (query: LogQuery) => void; ended: Promise<void> } {
  const socket = new WebSocket((options.on ?? server).socketAddress("/api/v1/media/logs"));
  const ask = (query: LogQuery) => socket.send(JSON.stringify(query));
  const ended = new Promise<void>((done, failed) => {
    socket.addEventListener("open", () => {
      if (options.query) ask(options.query);
    });
    socket.addEventListener("message", (event: MessageEvent) => {
      if (typeof event.data === "string") told(JSON.parse(event.data) as TailMessage);
    });
    socket.addEventListener("close", () => done());
    socket.addEventListener("error", () => {
      // Closed by its caller: the end it asked for.
      if (options.signal?.aborted) done();
      else failed(new ApiError("unreachable", "the logs' tail closed with an error"));
    });
    options.signal?.addEventListener("abort", () => socket.close(), { once: true });
  });
  return { ask, ended };
}
