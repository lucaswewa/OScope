// A record's annotations (P44, W7.5): its layers of GeoJSON in µm, what
// they measure, and one kept or removed. The actions go through the
// generated types; the endpoints, which the OpenAPI document doesn't
// describe, are typed here from the store's records, and checked against a
// real server by the contract tests.
import { act, type Following } from "./actions.ts";
import { fetched, server, type Limits, type Schema, type Server } from "./client.ts";

type On = Limits & { on?: Server | undefined };

/** A record: an acquisition or a result of a project. */
export interface RecordRef {
  project: string;
  kind: "acquisition" | "result";
  id: string;
}

/** A feature, as the server keeps it. */
export type KeptFeature = Schema<"workspace_annotate_input">["feature"];

/** A record's annotations: its layers by name, each a FeatureCollection. */
export interface Annotations {
  layers: Record<string, { type: "FeatureCollection"; features: KeptFeature[] }>;
  last_id: number;
}

/** What an annotation measures, as the server says it. */
export interface Measurement {
  layer: string;
  id: string;
  name: string | null;
  geometry: "Point" | "LineString" | "Polygon";
  /** `µm`, or `px` on an image of unknown pixel size. */
  unit: string;
  /** A line's length, or a polygon's perimeter. */
  length: number;
  area: number;
  centroid: [number, number];
  channels: { name: string; pixels: number; mean: number; sd: number; min: number; max: number }[];
}

const path = (what: string, record: RecordRef) =>
  `/api/v1/workspace/${what}/${encodeURIComponent(record.project)}/${record.kind}s/${encodeURIComponent(record.id)}`;

/** `record`'s annotations. */
export function annotationsOf(record: RecordRef, options: On = {}): Promise<Annotations> {
  const address = (options.on ?? server).address(path("annotations", record));
  return fetched(address, (answer) => answer.json() as Promise<Annotations>, options);
}

/** The address of what `layer` of `record` measures: as JSON, or its CSV. */
export function measurementsAddress(
  record: RecordRef,
  layer: string,
  csv = false,
  on: Server = server,
): string {
  const query = `?layer=${encodeURIComponent(layer)}${csv ? "&format=csv" : ""}`;
  return on.address(`${path("measurements", record)}${query}`);
}

/** What `layer` of `record` measures. */
export function measurementsOf(record: RecordRef, layer: string, options: On = {}) {
  const address = measurementsAddress(record, layer, false, options.on ?? server);
  return fetched(address, (answer) => answer.json() as Promise<Measurement[]>, options);
}

/** `feature` kept in `layer` of `record`: new, or in place of the one of its ID. As kept, with its ID. */
export function annotate(
  record: RecordRef,
  layer: string,
  feature: KeptFeature,
  options: Following = {},
): Promise<KeptFeature> {
  const { project, kind, id } = record;
  return act("/api/v1/workspace/annotate", { project, kind, record: id, layer, feature }, options);
}

/** Feature `feature` of `layer` of `record` removed. */
export async function removeAnnotation(
  record: RecordRef,
  layer: string,
  feature: string,
  options: Following = {},
): Promise<void> {
  const { project, kind, id } = record;
  const input = { project, kind, record: id, layer, feature };
  await act("/api/v1/workspace/remove_annotation", input, options);
}
