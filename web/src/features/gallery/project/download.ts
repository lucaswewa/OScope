// A project's files downloaded (W6.2), through the host adapter (ADR-0046):
// each read as it comes, its bytes kept as they arrive, so that one stopped,
// by its user or by a lost connection, resumes from where it was, by a
// range of the rest (RFC 9110 §14), from the data route (P30).
import { shallowRef } from "vue";

import { fetched } from "@/api/client";
import { ApiError } from "@/api/errors";
import { host } from "@/host/host";

/** The header asking for the rest of a file, from `done` bytes; none from its start. */
export function rangeFrom(done: number): Record<string, string> {
  return done > 0 ? { range: `bytes=${done}-` } : {};
}

/**
 * What an answer says of the file: its whole length, if known, and whether
 * it sent the range asked for, or the whole file again from its start.
 */
export function answered(
  status: number,
  headers: { get(name: string): string | null },
): { ranged: boolean; of?: number } {
  const range = /\/(\d+)$/.exec(headers.get("content-range") ?? "");
  const length = Number(headers.get("content-length")) || undefined;
  if (status === 206 && range) return { ranged: true, of: Number(range[1]) };
  return { ranged: false, ...(length === undefined ? {} : { of: length }) };
}

/** Where a download is. */
export type DownloadState = "idle" | "running" | "stopped" | "saved" | "failed";

/** A file of a project, downloaded, stopped and resumed, then saved. */
export function useDownload(address: () => string, name: () => string) {
  const state = shallowRef<DownloadState>("idle");
  const done = shallowRef(0);
  const of = shallowRef<number>();
  const why = shallowRef<string>();
  let parts: Uint8Array[] = [];
  let type = "";
  let stopping: AbortController | undefined;

  /** Reads the file, or its rest: the bytes kept as they come, a stop keeping them. */
  async function run(): Promise<void> {
    const mine = (stopping = new AbortController());
    state.value = "running";
    why.value = undefined;
    try {
      await fetched(
        address(),
        async (answer) => {
          const said = answered(answer.status, answer.headers);
          if (!said.ranged && done.value > 0) {
            // The whole file again: what was kept goes.
            parts = [];
            done.value = 0;
          }
          of.value = said.of;
          type = answer.headers.get("content-type") ?? type;
          const reader = answer.body?.getReader();
          for (;;) {
            const chunk = reader ? await reader.read() : { done: true as const };
            if (chunk.done) break;
            parts.push(chunk.value);
            done.value += chunk.value.byteLength;
          }
        },
        { signal: mine.signal },
        { headers: rangeFrom(done.value) },
      );
      if (of.value !== undefined && done.value < of.value) {
        // The connection ended before the file did.
        state.value = "stopped";
        return;
      }
      await host().files.save(name(), new Blob(parts as BlobPart[], { type }));
      state.value = "saved";
    } catch (thrown) {
      if (mine.signal.aborted) {
        state.value = "stopped";
        return;
      }
      // A connection lost part way keeps what came, to resume.
      state.value = done.value > 0 ? "stopped" : "failed";
      why.value = thrown instanceof ApiError ? thrown.message : String(thrown);
    }
  }

  return {
    state,
    done,
    of,
    why,
    /** Starts it again from nothing. */
    start: () => {
      parts = [];
      done.value = 0;
      of.value = undefined;
      return run();
    },
    stop: () => stopping?.abort(),
    /** Asks for the rest, from where it stopped. */
    resume: () => run(),
  };
}
