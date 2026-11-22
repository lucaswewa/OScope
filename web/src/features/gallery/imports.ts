// Files into a project, and out of it (W6.3). An import is each file
// uploaded with its progress, staged by the server (ADR-0051), then read
// into an acquisition by the `projects.import` job, followed to its end.
// They are kept here, not in a page, so an import goes on while its user
// looks elsewhere: the project's card and its page both say it. And what an
// export's job wrote, from its result.
import { reactive } from "vue";

import { act } from "@/api/actions";
import { ApiError } from "@/api/errors";
import { finished, type Job } from "@/api/jobs";
import { projectFormats, upload, type Format, type Formats } from "@/api/projects";

/** Where an import is: refused when no importer reads its file. */
export type ImportState = "uploading" | "importing" | "imported" | "stopped" | "failed" | "refused";

/** A file on its way into a project. */
export interface Import {
  key: number;
  project: string;
  name: string;
  state: ImportState;
  /** Its bytes sent, of its size. */
  done: number;
  of: number;
  /** The acquisition it made. */
  acquisition?: string;
  /** Why it failed. */
  why?: string;
}

/** The states an import doesn't leave. */
export const IMPORT_ENDED: ReadonlySet<ImportState> = new Set([
  "imported",
  "stopped",
  "failed",
  "refused",
]);

/** The files the importers read, as a file chooser takes them: `.tif`. */
export function acceptOf(importers: readonly Format[]): string[] {
  const extensions = importers.flatMap((one) => one.extensions);
  return [...new Set(extensions.map((extension) => `.${extension.toLowerCase()}`))];
}

/** Whether an importer reads the file `name`, by how its name ends. */
export function readable(name: string, importers: readonly Format[]): boolean {
  const lower = name.toLowerCase();
  return importers.some((one) => one.extensions.some((e) => lower.endsWith(`.${e.toLowerCase()}`)));
}

/** The record a job made in `project`, of `kind`, from its results. */
export function madeBy(
  job: Pick<Job, "results">,
  kind: "acquisitions" | "results" = "acquisitions",
): string | undefined {
  for (const result of job.results) {
    const found = /^projects\/[^/]+\/([^/]+)\/([^/]+)$/.exec(result);
    if (found?.[1] === kind) return found[2];
  }
  return undefined;
}

/** The file an export wrote, within its project, from its job's result `data/{project}/{path}`. */
export function exportedOf(job: Pick<Job, "results">, project: string): string | undefined {
  const prefix = `data/${project}/`;
  return job.results.find((result) => result.startsWith(prefix))?.slice(prefix.length);
}

/** The exporters a record's image is written by: each one a file, which a deep zoom folder isn't. */
export function exportersOf(formats: Formats | undefined): Format[] {
  return (formats?.exporters ?? []).filter((one) => one.id !== "dzi");
}

let formats: Promise<Formats> | undefined;
/** The server's formats, read once; again after a failure. */
export function formatsOnce(): Promise<Formats> {
  formats ??= projectFormats().catch((thrown: unknown) => {
    formats = undefined;
    throw thrown;
  });
  return formats;
}

const imports = reactive<Import[]>([]);
const stops = new Map<number, AbortController>();
let next = 0;

/** The imports, under way and ended, the oldest first: `project`'s, or all. */
export function importsOf(project?: string): Import[] {
  return imports.filter((one) => project === undefined || one.project === project);
}

/** Imports `files` into `project`: each uploaded, then read by the import job. */
export function importFiles(
  project: string,
  files: readonly File[],
  importers: readonly Format[],
): void {
  for (const file of files) void importOne(project, file, importers);
}

async function importOne(project: string, file: File, importers: readonly Format[]): Promise<void> {
  const key = next++;
  imports.push({ key, project, name: file.name, state: "uploading", done: 0, of: file.size });
  const one = imports.find((each) => each.key === key)!;
  if (!readable(file.name, importers)) {
    one.state = "refused";
    return;
  }
  const stop = new AbortController();
  stops.set(key, stop);
  try {
    const staged = await upload(file, {
      signal: stop.signal,
      progress: (now) => {
        one.done = now.done;
        one.of = now.of ?? file.size;
      },
    });
    stops.delete(key);
    one.state = "importing";
    const params = { project, upload: staged.id };
    const job = await act("/api/v1/jobs/submit", { kind: "projects.import", params });
    one.acquisition = madeBy(await finished(job));
    one.state = "imported";
  } catch (thrown) {
    const stopped = thrown instanceof ApiError && thrown.failure === "aborted";
    one.state = stopped ? "stopped" : "failed";
    if (!stopped) one.why = thrown instanceof Error ? thrown.message : String(thrown);
  } finally {
    stops.delete(key);
  }
}

/** Stops an upload under way; an import, once its job has begun, runs to its end. */
export function stopImport(key: number): void {
  stops.get(key)?.abort();
}

/** Forgets `project`'s imports that ended. */
export function clearImports(project: string): void {
  for (let i = imports.length - 1; i >= 0; i--) {
    const one = imports[i]!;
    if (one.project === project && IMPORT_ENDED.has(one.state)) imports.splice(i, 1);
  }
}

/** Imports `files` into `project`, once the server's importers are known. */
export async function importInto(project: string, files: readonly File[]): Promise<void> {
  let importers: readonly Format[] = [];
  try {
    importers = (await formatsOnce()).importers;
  } catch {
    // Unknown, each file is refused: no importer is known to read it.
  }
  importFiles(project, files, importers);
}
