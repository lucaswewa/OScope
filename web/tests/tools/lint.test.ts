// The lint rules that keep the app's layers and its two doors to the platform
// (ADR-0039). Each is tried on a few lines that break it, and on the place
// where the same lines are allowed.
import { fileURLToPath } from "node:url";

import { ESLint } from "eslint";
import { describe, expect, it } from "vitest";

const eslint = new ESLint({ cwd: fileURLToPath(new URL("../..", import.meta.url)) });

/** The rules that `code` breaks, were it the file at `path`. */
async function broken(path: string, code: string): Promise<string[]> {
  const [result] = await eslint.lintText(code, { filePath: path });
  return (result?.messages ?? []).map((message) => `${message.ruleId}: ${message.message}`);
}

describe("the layers", () => {
  it.each([
    [
      "src/ui/Button.ts",
      'import { serverVersion } from "@/api/system";',
      "src/ui/ may not import from api/",
    ],
    [
      "src/ui/Button.ts",
      'import { router } from "@/app/router";',
      "src/ui/ may not import from app/",
    ],
    [
      "src/api/system.ts",
      'import Button from "@/ui/Button.vue";',
      "src/api/ may not import from ui/",
    ],
    [
      "src/host/storage.ts",
      'import { x } from "@/features/home/x";',
      "src/host/ may not import from features/",
    ],
    [
      "src/features/home/x.ts",
      'import { router } from "@/app/router";',
      "src/features/ may not import from app/",
    ],
  ])("%s may not hold `%s`", async (path, code, why) => {
    const found = await broken(path, `${code}\nexport {};\n`);
    expect(found.join("\n")).toContain(why);
  });

  it("lets only src/ui/ import reka-ui", async () => {
    const code = 'import { DialogRoot } from "reka-ui";\nexport { DialogRoot };\n';
    for (const path of [
      "src/features/home/x.ts",
      "src/app/shell.ts",
      "src/api/x.ts",
      "src/host/x.ts",
    ]) {
      expect((await broken(path, code)).join("\n"), path).toContain("Only src/ui/ imports reka-ui");
    }
    expect(await broken("src/ui/Dialog.ts", code)).toEqual([]);
  });

  it("lets a feature use the server's client, the platform and the components", async () => {
    const code = [
      'import { serverVersion } from "@/api/system";',
      'import { storage } from "@/host/storage";',
      'import Button from "@/ui/Button.vue";',
      "export { Button, serverVersion, storage };",
      "",
    ].join("\n");
    expect(await broken("src/features/home/x.ts", code)).toEqual([]);
  });
});

describe("the platform's two doors", () => {
  const cases: Array<[string, string]> = [
    ['await fetch("api/v1/system/version");', "Only src/api/ talks to the server"],
    ['new WebSocket("api/v1/stage/ws");', "Only src/api/ talks to the server"],
    ['new EventSource("api/v1/stage/moved");', "Only src/api/ talks to the server"],
    ['localStorage.setItem("theme", "dark");', "Only src/host/ keeps state"],
    ['sessionStorage.getItem("token");', "Only src/host/ keeps state"],
    ['window.open("docs");', "window.open is the platform's"],
    ["window.localStorage.clear();", "window.localStorage is the platform's"],
    ['const server = "http://127.0.0.1:8210/";\nexport { server };', "No absolute URLs"],
    ['const stream = "wss://example.org/preview";\nexport { stream };', "No absolute URLs"],
  ];

  it.each(cases)("a feature, the shell and a component may not hold `%s`", async (code, why) => {
    for (const path of ["src/features/home/x.ts", "src/app/shell.ts", "src/ui/Button.ts"]) {
      expect((await broken(path, `${code}\nexport {};\n`)).join("\n"), path).toContain(why);
    }
  });

  it("opens the network to src/api/ and the platform to src/host/", async () => {
    const network = 'export const answer = await fetch("api/v1/system/version");\n';
    expect(await broken("src/api/system.ts", network)).toEqual([]);
    const state = 'localStorage.setItem("theme", "dark");\nexport {};\n';
    expect(await broken("src/host/storage.ts", state)).toEqual([]);
  });

  it("holds a Vue component to the same rules", async () => {
    const code = [
      '<script setup lang="ts">',
      'const answer = await fetch("api/v1/x");',
      "defineExpose({ answer });",
      "</script>",
      "",
    ].join("\n");
    const found = await broken("src/features/home/Page.vue", code);
    expect(found.join("\n")).toContain("Only src/api/ talks to the server");
  });
});
