// A server of a test's own (ADR-0040): the workspace's `oscope-server`, with
// its own configuration, data and scratch space, on a port the system picks,
// serving the built app. Each test has one, so tests run side by side and
// none sees another's state.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

/** The `web/` folder. */
export const web = fileURLToPath(new URL("../..", import.meta.url));

/**
 * The server's configuration, relative to its folder. Without one, the
 * scratch space is the working folder's: every test's server would share it,
 * and each clears it as it starts. Power actions only say what they would
 * do, unless the test lets it restart (ADR-0060). Its third-party notices
 * are its folder's `notices`, empty unless the test puts some there: not a
 * package's, if this computer has one installed.
 */
const config = (restarts: boolean, simulator: string) => `[storage]
data_root = "data"
scratch_root = "scratch"

[power]
dry_run = true
${restarts ? "dry_run_restart = false\n" : ""}
[web]
notices = "notices"
${simulator ? `\n[simulator]\n${simulator}\n` : ""}`;

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
  /** Its folder: its configuration, with `data` and `scratch` beside it. */
  readonly folder = mkdtempSync(join(tmpdir(), "oscope-e2e-"));
  /** How many times it has restarted itself, by its exit code 75 (W4.3). */
  restarts = 0;
  /** Why it couldn't start again after a restart of its own, if it couldn't. */
  failed: string | undefined;
  private port = 0;
  private child: ChildProcess | undefined;
  /** Its start after a restart of its own, while that is under way. */
  private again: Promise<void> | undefined;

  /**
   * With `restarts`, its restart is done, as its supervisor would: rebooting and shutting down
   * never are. With `simulator`, its simulator has those lines of TOML.
   */
  constructor(options: { restarts?: boolean; simulator?: string } = {}) {
    const { restarts = false, simulator = "" } = options;
    writeFileSync(join(this.folder, "oscope.toml"), config(restarts, simulator));
  }

  /**
   * Adds an account, before the server starts: with one, it has security
   * (ADR-0031), and every page asks for a login (W1.3).
   */
  addAccount(name: string, password: string): void {
    const config = join(this.folder, "oscope.toml");
    const run = spawnSync(program(), ["--config", config, "account", "add", name], {
      input: `${password}
`,
      encoding: "utf8",
    });
    if (run.status !== 0) throw new Error(`account add ${name}: ${run.stderr || run.error}`);
  }

  /** Starts it: on a port the system picks, or the one it had before. */
  async start(): Promise<void> {
    const args = ["--config", join(this.folder, "oscope.toml"), "--port", String(this.port)];
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
    // A restart it asked for itself: started again, on the same port, as its supervisor would.
    child.once("exit", (code) => {
      if (code !== 75 || this.child !== child) return;
      this.restarts += 1;
      this.again = this.start().catch((error: unknown) => {
        this.failed = String(error);
      });
    });
  }

  /** Stops it, and waits until it has gone. */
  async stop(): Promise<void> {
    await this.again;
    const child = this.child;
    this.child = undefined;
    if (!child || child.exitCode !== null) return;
    const gone = new Promise<void>((done) => child.once("exit", () => done()));
    child.kill();
    await gone;
  }

  /** Stops it, and removes its folder. */
  async dispose(): Promise<void> {
    await this.stop();
    rmSync(this.folder, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
  }
}
