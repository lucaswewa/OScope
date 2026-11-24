import { defineStore } from "pinia";
import { computed, ref, watch } from "vue";

import { host } from "@/host/host";
import {
  accents,
  apply,
  isOneOf,
  themes,
  type Accent,
  type Scheme,
  type Theme,
} from "@/ui/tokens/theme";

/** The name the host keeps the choices under, for the first paint before the account's are known. */
const KEY = "oscope.display";

/** What the host kept, as far as it is something the app offers: with the account's theme last followed. */
function kept(): { theme: Theme; accent: Accent; account: Theme | undefined } {
  let saved: unknown;
  try {
    saved = JSON.parse(host().kept.read(KEY) ?? "null");
  } catch {
    saved = null;
  }
  const { theme, accent, account } = (saved ?? {}) as Record<string, unknown>;
  return {
    theme: isOneOf(themes, theme) ? theme : "system",
    accent: isOneOf(accents, accent) ? accent : "magenta",
    account: isOneOf(themes, account) ? account : undefined,
  };
}

/**
 * How the app looks (ADR-0041): the theme and the accent. A change reaches
 * the page at once, and the host keeps it for the next visit.
 *
 * The theme has two sources (W8.1): this computer's, kept by the host, and
 * the account's, which the server keeps. The computer's is the first paint.
 * The account's is followed whenever it says something this computer hasn't
 * followed yet, so a change made in another browser, even while this one
 * was closed, is taken up; a choice made here alone stands until then.
 */
export const useDisplay = defineStore("display", () => {
  const theme = ref<Theme>("system");
  const accent = ref<Accent>("magenta");
  const computerIsDark = ref(false);
  let page: HTMLElement | undefined;
  /** The account's theme as this computer last followed it, kept between visits. */
  let account: Theme | undefined;
  const keep = () =>
    host().kept.write(KEY, JSON.stringify({ theme: theme.value, accent: accent.value, account }));

  /**
   * The scheme in use, for what the app draws itself: the theme's, or under
   * system the computer's, as it changes. The page's own colours need no
   * telling: the tokens follow the computer by themselves.
   */
  const scheme = computed<Scheme>(() => {
    if (theme.value !== "system") return theme.value;
    return computerIsDark.value ? "dark" : "light";
  });

  watch(
    [theme, accent],
    () => {
      if (!page) return;
      apply(page, theme.value, accent.value);
      keep();
    },
    { flush: "sync" },
  );

  /**
   * Takes up what the host kept and puts it on `root`, the page's root;
   * from then on each change is applied and kept, and `computer`, the
   * computer's wish for a dark scheme, is followed.
   */
  function start(
    root: HTMLElement = document.documentElement,
    computer: MediaQueryList = matchMedia("(prefers-color-scheme: dark)"),
  ): void {
    ({ theme: theme.value, accent: accent.value, account } = kept());
    page = root;
    apply(root, theme.value, accent.value);
    computerIsDark.value = computer.matches;
    computer.addEventListener("change", (event) => {
      computerIsDark.value = event.matches;
    });
  }

  /** The account's theme, as the server says it now: followed if this computer hasn't followed it yet. */
  function follow(now: Theme): void {
    if (now === account) return;
    account = now;
    if (theme.value === now) keep();
    else theme.value = now;
  }

  return { theme, accent, scheme, start, follow };
});
