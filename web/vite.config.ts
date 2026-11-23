import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

import vue from "@vitejs/plugin-vue";
import { defineConfig, type Plugin } from "vitest/config";

import { noticesOf } from "./scripts/notices.mjs";

/** The server the dev loop proxies to: the simulated instrument's own port. */
const server = process.env.OSCOPE_SERVER ?? "http://127.0.0.1:8210";

/** The package a module's file belongs to, if it is one of `node_modules`'. */
export function packageOf(id: string): string | undefined {
  const marker = "node_modules/";
  const at = id.replaceAll("\\", "/").lastIndexOf(marker);
  if (at < 0) return undefined;
  const [first, second] = id
    .replaceAll("\\", "/")
    .slice(at + marker.length)
    .split("/");
  if (!first) return undefined;
  return first.startsWith("@") && second ? `${first}/${second}` : first;
}

/**
 * Writes the packages whose code is in the bundle to `.bundle/packages.json`:
 * what the app ships, which the licence and advisory checks hold to the
 * stricter list (ADR-0002, `scripts/packages.mjs`). And their notices,
 * beside the app as `notices.json`, for About (W4.3).
 */
function bundledPackages(): Plugin {
  return {
    name: "oscope:bundled-packages",
    apply: "build",
    generateBundle(_options, bundle) {
      const folders = new Map<string, string>();
      for (const chunk of Object.values(bundle)) {
        if (chunk.type !== "chunk") continue;
        for (const id of chunk.moduleIds) {
          const name = packageOf(id);
          if (!name) continue;
          const path = id.replaceAll("\\", "/");
          const modules = path.slice(0, path.lastIndexOf("node_modules/") + "node_modules/".length);
          folders.set(name, modules + name);
        }
      }
      mkdirSync(".bundle", { recursive: true });
      const names = [...folders.keys()].sort();
      writeFileSync(".bundle/packages.json", JSON.stringify(names, null, 2) + "\n");
      const read = (path: string) => readFileSync(path, "utf8");
      const notices = noticesOf(folders, read, (folder) => readdirSync(folder));
      this.emitFile({ type: "asset", fileName: "notices.json", source: JSON.stringify(notices) });
    },
  };
}

export default defineConfig({
  // Relative, so the same build runs at a server's root and in the desktop app.
  base: "./",
  plugins: [vue(), bundledPackages()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  server: {
    host: "127.0.0.1",
    proxy: {
      "/api": { target: server, ws: true },
      "/health": { target: server },
    },
  },
  // VTK.js's chunk, the 3D tab's alone, is near a MB: loaded only with a first surface (ADR-0054).
  build: { outDir: "dist", emptyOutDir: true, sourcemap: true, chunkSizeWarningLimit: 1_100 },
  test: {
    projects: [
      {
        // The app's own code and components, in a page's environment.
        extends: true,
        test: {
          name: "unit",
          environment: "happy-dom",
          include: ["src/**/*.test.ts"],
          // Unmounted after each test; a warning on the console fails it.
          setupFiles: ["tests/setup/unit.ts"],
        },
      },
      {
        // The workspace's tools: the lint rules and the package checks.
        extends: true,
        test: { name: "tools", environment: "node", include: ["tests/tools/**/*.test.ts"] },
      },
    ],
  },
});
