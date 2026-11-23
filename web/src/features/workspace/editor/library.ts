// The library's names (W7.3, ADR-0052): 1 to 64 of `a`–`z`, `0`–`9`, `-`
// and `_`, and not a stock recipe's. The server checks them too; they are
// checked here as they are typed.
import type { ListedRecipe } from "@/api/pipeline";

/** Why `name` can't be a user's recipe's, if it can't. */
export function nameProblem(
  name: string,
  listed: readonly ListedRecipe[],
): "empty" | "long" | "characters" | "stock" | undefined {
  if (!name) return "empty";
  if (name.length > 64) return "long";
  if (!/^[a-z0-9_-]+$/.test(name)) return "characters";
  if (listed.some((one) => one.stock && one.name === name)) return "stock";
  return undefined;
}

/** Whether saving as `name` replaces one of the user's recipes. */
export const replaces = (name: string, listed: readonly ListedRecipe[]): boolean =>
  listed.some((one) => !one.stock && one.name === name);

/** A name for a recipe called `title`: its words, in lower case, joined by `-`. */
export function nameFor(title: string): string {
  const plain = title.normalize("NFKD").replace(/[̀-ͯ]/g, "").toLowerCase();
  return plain
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 64);
}
