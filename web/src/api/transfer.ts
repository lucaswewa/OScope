// Uploads and downloads with their progress (W1.3), for the phases that move
// files: a download read as it comes, and an upload sent by
// XMLHttpRequest, the only way a browser tells an upload's progress. Both
// with the session's token, the time limit and the caller's abort.
import { fetched, session, type Limits } from "./client.ts";
import { answerError, ApiError } from "./errors.ts";

/** How far a transfer has come, in bytes. */
export interface Progress {
  done: number;
  /** Its whole, when the server says it. */
  of?: number | undefined;
}

type Moving = Limits & { progress?: (now: Progress) => void };

/** A download, told as it comes: its bytes, as a blob of its type. */
export function receive(address: string, options: Moving = {}): Promise<Blob> {
  return fetched(
    address,
    async (answer) => {
      const of = Number(answer.headers.get("content-length")) || undefined;
      const type = answer.headers.get("content-type") ?? "";
      if (!answer.body) return new Blob([], { type });
      const reader = answer.body.getReader();
      const parts: Uint8Array[] = [];
      let done = 0;
      for (;;) {
        const chunk = await reader.read();
        if (chunk.done) break;
        parts.push(chunk.value);
        done += chunk.value.byteLength;
        options.progress?.({ done, of });
      }
      return new Blob(parts as BlobPart[], { type });
    },
    options,
  );
}

/** An upload of `body`, told as it goes: the server's answer, as JSON if it is. */
export function send(
  address: string,
  body: Blob,
  options: Moving & { method?: string } = {},
): Promise<unknown> {
  return new Promise((done, failed) => {
    const request = new XMLHttpRequest();
    request.open(options.method ?? "POST", address);
    if (session.token) request.setRequestHeader("authorization", `Bearer ${session.token}`);
    if (body.type) request.setRequestHeader("content-type", body.type);
    request.timeout = options.timeout ?? 0;
    request.upload.addEventListener("progress", (event) =>
      options.progress?.({
        done: event.loaded,
        of: event.lengthComputable ? event.total : undefined,
      }),
    );
    const answer = () => {
      try {
        return JSON.parse(request.responseText) as unknown;
      } catch {
        return request.responseText;
      }
    };
    request.addEventListener("load", () => {
      if (request.status >= 200 && request.status < 300) done(answer());
      else failed(answerError(request.status, answer(), request.statusText));
    });
    request.addEventListener("error", () =>
      failed(new ApiError("unreachable", "the upload had no answer")),
    );
    request.addEventListener("timeout", () =>
      failed(new ApiError("timeout", "the server didn't answer in time")),
    );
    request.addEventListener("abort", () =>
      failed(new ApiError("aborted", "the call was stopped")),
    );
    options.signal?.addEventListener("abort", () => request.abort(), { once: true });
    request.send(body);
  });
}
