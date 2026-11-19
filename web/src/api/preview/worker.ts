// The preview's worker (W1.5, ADR-0047): its stream, off the page's thread.
import { PreviewStream, type ToWorker } from "./stream.ts";

/** The worker's own scope, as the stream uses it. */
const scope = self as unknown as {
  onmessage: ((event: MessageEvent<ToWorker>) => void) | null;
  postMessage(message: unknown, transfer: Transferable[]): void;
};

const stream = new PreviewStream({
  socket: (url) => new WebSocket(url),
  decode: (jpeg) => createImageBitmap(jpeg),
  post: (message, transfer = []) => scope.postMessage(message, transfer),
  now: () => performance.timeOrigin + performance.now(),
});

scope.onmessage = (event) => stream.handle(event.data);
