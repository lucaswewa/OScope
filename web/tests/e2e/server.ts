import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

/** The source folder, mounted at the same absolute path on the Docker host. */
export const web = fileURLToPath(new URL("../..", import.meta.url));
const temporary = process.env.OSCOPE_E2E_TMP;
if (!temporary) throw new Error("Use scripts/integration.sh for container-isolated browser tests");
mkdirSync(temporary, { recursive: true });
const image = process.env.OSCOPE_BACKEND_IMAGE;
if (!image || !/^sha256:[a-f0-9]{64}$/.test(image)) {
  throw new Error("OSCOPE_BACKEND_IMAGE must identify an immutable, prepared runtime image");
}
const runtimeImage: string = image;
const temporaryRoot: string = temporary;
const runId = process.env.OSCOPE_E2E_RUN_ID;
if (!runId || runId === "manual")
  throw new Error("The integration runner must own a unique run ID");

/** Runs only the runtime image; never a host Rust executable. Passwords use stdin. */
function docker(args: string[], input?: string): string {
  const run = spawnSync("docker", args, {
    ...(input === undefined ? {} : { input }),
    encoding: "utf8",
    timeout: 45_000,
    maxBuffer: 64 * 1024 * 1024,
  });
  if (run.status !== 0)
    throw new Error("test container command failed: " + (run.stderr || run.error));
  return run.stdout;
}

/** Each test owns its container, dynamic loopback listener and synthetic files. */
export class Server {
  url = "";
  version = "";
  readonly folder = mkdtempSync(join(temporaryRoot, "oscope-e2e-"));
  readonly name = "oscope-e2e-" + randomUUID();
  restarts = 0;
  failed: string | undefined;
  private port = 0;
  private child: ChildProcess | undefined;
  private again: Promise<void> | undefined;

  constructor(options: { restarts?: boolean } = {}) {
    writeFileSync(
      join(this.folder, "oscope.toml"),
      [
        "version = 2",
        "[network]",
        'address = "127.0.0.1"',
        "port = 0",
        "[storage]",
        'data_root = "data"',
        'scratch_root = "scratch"',
        "[power]",
        "dry_run = true",
        "dry_run_restart = " + !options.restarts,
        "[web]",
        'notices = "notices"',
        "",
      ].join("\n"),
    );
  }

  private mounts(): string[] {
    return ["--mount", "type=bind,src=" + this.folder + ",dst=" + this.folder];
  }

  addAccount(name: string, password: string): void {
    docker(
      [
        "run",
        "--rm",
        "-i",
        "--network",
        "none",
        "--read-only",
        "--label",
        "oscope.e2e.run=" + runId,
        "--workdir",
        this.folder,
        ...this.mounts(),
        "-e",
        "NVIDIA_VISIBLE_DEVICES=void",
        runtimeImage,
        "--config",
        join(this.folder, "oscope.toml"),
        "account",
        "add",
        name,
      ],
      password + "\n",
    );
  }

  async start(): Promise<void> {
    const args = [
      "run",
      "--rm",
      "--init",
      "--name",
      this.name,
      "--label",
      "oscope.e2e.run=" + runId,
      "--network",
      "host",
      "--read-only",
      "--cap-drop",
      "ALL",
      "--security-opt",
      "no-new-privileges",
      "--no-healthcheck",
      "--tmpfs",
      "/tmp:rw,nosuid,nodev,size=256m,mode=1777",
      "-e",
      process.env.OSCOPE_E2E_GPU === "1"
        ? "NVIDIA_VISIBLE_DEVICES=all"
        : "NVIDIA_VISIBLE_DEVICES=void",
      ...(process.env.OSCOPE_E2E_GPU === "1" ? ["--gpus", "all"] : []),
      "--workdir",
      this.folder,
      ...this.mounts(),
      "--mount",
      "type=bind,src=" + join(web, "dist") + ",dst=" + join(web, "dist") + ",readonly",
      runtimeImage,
      "--config",
      join(this.folder, "oscope.toml"),
      "--port",
      String(this.port),
      "--web",
      join(web, "dist"),
    ];
    const child = spawn("docker", args, { stdio: ["ignore", "pipe", "pipe"] });
    this.child = child;
    let log = "";
    child.stderr!.on("data", (chunk: Buffer) => {
      log = (log + chunk.toString()).slice(-4000);
    });
    const lines = createInterface({ input: child.stdout! });
    const first = await new Promise<string>((said, failed) => {
      const timeout = setTimeout(
        () => failed(new Error("test container startup timed out: " + log)),
        60_000,
      );
      timeout.unref();
      lines.once("line", (line) => {
        clearTimeout(timeout);
        said(line);
      });
      child.once("error", (error) => {
        clearTimeout(timeout);
        failed(error);
      });
      child.once("exit", (code) => {
        clearTimeout(timeout);
        failed(new Error("test container ended with " + code + ": " + log));
      });
    });
    const serving = /^oscope-server (\S+) serving on (http:\/\/[^/\s]+:(\d+))\/$/.exec(first);
    if (!serving) throw new Error("test container said: " + first);
    this.version = serving[1]!;
    this.url = serving[2]!;
    this.port = Number(serving[3]);
    child.once("exit", (code) => {
      if (code !== 75 || this.child !== child) return;
      this.restarts += 1;
      this.again = this.start().catch((error: unknown) => {
        this.failed = String(error);
      });
    });
  }

  async stop(): Promise<void> {
    await this.again;
    const child = this.child;
    this.child = undefined;
    if (!child || child.exitCode !== null) return;
    const gone = new Promise<void>((done) => child.once("exit", () => done()));
    docker(["stop", "--time", "30", this.name]);
    await gone;
  }

  async dispose(): Promise<void> {
    await this.stop();
    rmSync(this.folder, { recursive: true, force: true, maxRetries: 20, retryDelay: 100 });
  }
}
