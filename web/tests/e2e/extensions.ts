// The sample Rust extension, packed for the end-to-end tests (W8.5): its
// library and `ex-rust-ext` built where the tests' server is, as Cargo
// built it, then `ex-rust-ext --pack` making the package, with its panel.
import { spawnSync } from "node:child_process";
import { join, resolve } from "node:path";

import { web } from "./server.ts";

/** The workspace's build folder: where the tests' server is built too. */
const target = () => process.env.CARGO_TARGET_DIR ?? resolve(web, "..", "target");

/** The sample, `org.oscope.examples.emboss`, packed into `out`: its path. */
export function embossPackage(out: string): string {
  const built = spawnSync("cargo", ["build", "-p", "ext-rust-node", "-p", "ex-rust-ext"], {
    cwd: resolve(web, ".."),
    encoding: "utf8",
    env: { ...process.env, CARGO_TARGET_DIR: target() },
  });
  if (built.status !== 0) throw new Error(`cargo build: ${built.stderr}`);
  const program = process.platform === "win32" ? "ex-rust-ext.exe" : "ex-rust-ext";
  const file = join(out, "org.oscope.examples.emboss-0.1.0.omext");
  const packed = spawnSync(join(target(), "debug", program), ["--pack", file], {
    encoding: "utf8",
  });
  if (packed.status !== 0) throw new Error(`ex-rust-ext --pack: ${packed.stderr}`);
  return file;
}
