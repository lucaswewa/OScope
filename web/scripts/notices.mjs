// The app's third-party notices (W4.3): each package its bundle holds, with
// its version, its licence, and its licence's text, as About lists them.
// The build writes them beside the app as `notices.json` (vite.config.ts).
import { licenceText } from "./packages.mjs";

/** The names a package's licence file goes by. */
const LICENCE_FILE = /^(licen[cs]e|copying|notice)(\.(md|txt))?$/i;

/**
 * @typedef {{ name: string, version: string, licence: string, text: string }} Notice
 */

/**
 * The notices of the packages in `folders`, by name: in order of name, each
 * with the licence its manifest says and the text of its licence files.
 * A package without one has an empty text.
 * @param {Map<string, string>} folders each package's folder, by its name
 * @param {(path: string) => string} read a file's text
 * @param {(folder: string) => string[]} list a folder's entries
 * @returns {Notice[]}
 */
export function noticesOf(folders, read, list) {
  return [...folders]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([name, folder]) => {
      const manifest = JSON.parse(read(`${folder}/package.json`));
      const files = list(folder)
        .filter((entry) => LICENCE_FILE.test(entry))
        .sort();
      return {
        name,
        version: manifest.version ?? "?",
        licence: licenceText(manifest.license ?? manifest.licenses) ?? "UNKNOWN",
        text: files.map((file) => read(`${folder}/${file}`).trim()).join("\n\n"),
      };
    });
}
