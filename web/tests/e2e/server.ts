// A server of a test's own (ADR-0040): the workspace's `oscope-server`, with
// its own data root, on a port the system picks, serving the built app. Each
// test has one, so tests run side by side and none sees another's state.
import { spawn, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

/** The `web/` folder. */
export const web = fileURLToPath(new URL("../..", import.meta.url));

/** The server's program: `OSCOPE_SERVER_BIN`, or the workspace's debug build. */
function program(): string {
  if (process.env.OSCOPE_SERVER_BIN) return process.env.OSCOPE_SERVER_BIN;
  const name = process.platform === "win32" ? "oscope-server.exe" : "oscope-server";
  const target = process.env.CARGO_TARGET_DIR ?? resolve(web, "..", "target");
  return join(target, "debug", name);
}

export class Server {
  /** Where it serves, as `http://127.0.0.1:PORT`. */
  url = "";
  /** The version it says it is. */
  version = "";
  private port = 0;
  private child: ChildProcess | undefined;
  private readonly dataRoot = mkdtempSync(join(tmpdir(), "oscope-e2e-"));

  /** Starts it: on a port the system picks, or the one it had before. */
  async start(): Promise<void> {
    const args = ["--port", String(this.port), "--data-root", this.dataRoot];
    const child = spawn(program(), [...args, "--web", join(web, "dist")], {
      stdio: ["ignore", "pipe", "pipe"],
    });
    this.child = child;
    // Its log is kept, and shown only if it doesn't come up.
    let log = "";
    child.stderr!.on("data", (chunk: Buffer) => {
      log = (log + chunk.toString()).slice(-4000);
    });
    // Its first line says where it serves.
    const lines = createInterface({ input: child.stdout! });
    const first = await new Promise<string>((said, failed) => {
      lines.once("line", said);
      child.once("error", failed);
      child.once("exit", (code) => failed(new Error(`oscope-server ended with ${code}: ${log}`)));
    });
    const serving = /^oscope-server (\S+) serving on (http:\/\/[^/\s]+:(\d+))\/$/.exec(first);
    if (!serving) throw new Error(`oscope-server said: ${first}`);
    this.version = serving[1]!;
    this.url = serving[2]!;
    this.port = Number(serving[3]);
  }

  /** Stops it, and waits until it has gone. */
  async stop(): Promise<void> {
    const child = this.child;
    this.child = undefined;
    if (!child || child.exitCode !== null) return;
    const gone = new Promise<void>((done) => child.once("exit", () => done()));
    child.kill();
    await gone;
  }

  /** Stops it, and removes its data. */
  async dispose(): Promise<void> {
    await this.stop();
    rmSync(this.dataRoot, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
  }
}
