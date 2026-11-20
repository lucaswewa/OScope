// The app's third-party notices (scripts/notices.mjs, W4.3): each package the
// bundle holds, once, in order of name, with its version, its licence and
// its licence's text.
import { describe, expect, it } from "vitest";

import { noticesOf } from "../../scripts/notices.mjs";

/** A stand-in for `node_modules`: each file's text, by its path. */
const files: Record<string, string> = {
  "nm/vue/package.json": JSON.stringify({ version: "3.5.13", license: "MIT" }),
  "nm/vue/LICENSE": "The MIT licence, as vue gives it.\n",
  "nm/vue/README.md": "Not a licence.",
  "nm/@vue/shared/package.json": JSON.stringify({ version: "3.5.13", license: "MIT" }),
  "nm/@vue/shared/LICENSE.md": "The MIT licence, as @vue/shared gives it.",
  "nm/old/package.json": JSON.stringify({
    version: "1.0.0",
    licenses: [{ type: "MIT" }, { type: "Apache-2.0" }],
  }),
  "nm/old/licence.txt": "One of its two licences.",
  "nm/old/NOTICE": "Its notice.",
  "nm/bare/package.json": JSON.stringify({ version: "0.1.0", license: "ISC" }),
};

function read(path: string): string {
  const text = files[path];
  if (text === undefined) throw new Error(`no ${path}`);
  return text;
}

/** A folder's own entries, not its subfolders'. */
const list = (folder: string) =>
  Object.keys(files)
    .filter((path) => path.startsWith(`${folder}/`))
    .map((path) => path.slice(folder.length + 1))
    .filter((entry) => !entry.includes("/"));

describe("the app's notices", () => {
  const folders = new Map([
    ["vue", "nm/vue"],
    ["old", "nm/old"],
    ["@vue/shared", "nm/@vue/shared"],
    ["bare", "nm/bare"],
  ]);
  const notices = noticesOf(folders, read, list);
  const textOf = (name: string) => notices.find((notice) => notice.name === name)?.text;

  it("lists each package once, in order of name, with its version and licence", () => {
    expect(notices.map((notice) => [notice.name, notice.version, notice.licence])).toEqual([
      ["@vue/shared", "3.5.13", "MIT"],
      ["bare", "0.1.0", "ISC"],
      ["old", "1.0.0", "MIT OR Apache-2.0"],
      ["vue", "3.5.13", "MIT"],
    ]);
  });

  it("gives each the text of its licence files, and none to a package without one", () => {
    expect(textOf("vue")).toBe("The MIT licence, as vue gives it.");
    expect(textOf("@vue/shared")).toBe("The MIT licence, as @vue/shared gives it.");
    expect(textOf("old")).toBe("Its notice.\n\nOne of its two licences.");
    expect(textOf("bare")).toBe("");
  });
});
