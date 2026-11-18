import { mkdirSync, writeFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

import vue from "@vitejs/plugin-vue";
import { defineConfig, type Plugin } from "vitest/config";

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
 * stricter list (ADR-0002, `scripts/packages.mjs`).
 */
function bundledPackages(): Plugin {
  return {
    name: "oscope:bundled-packages",
    apply: "build",
    generateBundle(_options, bundle) {
      const names = new Set<string>();
      for (const chunk of Object.values(bundle)) {
        if (chunk.type !== "chunk") continue;
        for (const id of chunk.moduleIds) {
          const name = packageOf(id);
          if (name) names.add(name);
        }
      }
      mkdirSync(".bundle", { recursive: true });
      writeFileSync(".bundle/packages.json", JSON.stringify([...names].sort(), null, 2) + "\n");
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
  build: { outDir: "dist", emptyOutDir: true, sourcemap: true },
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
