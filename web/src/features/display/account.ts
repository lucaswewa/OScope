// The account's display preferences (W8.1): its theme, units, scale bar and
// colour map, which the server keeps for each account, or its defaults for
// one without its own. `display` is a property of whoever reads it, so the
// server can't tell a change to another of the account's pages: each reads
// it again every few seconds, by itself, without the `settings` Thing's
// socket, which would be one more on every page for nothing it could say.
import { defineStore } from "pinia";
import { onScopeDispose, shallowRef } from "vue";

import { call, server, type Schema } from "@/api/client";
import { ApiError } from "@/api/errors";

type Display = Schema<"settings_display_value">;

/** How often the preferences are read again, in ms. */
const EVERY = 5_000;

export const useAccountDisplay = defineStore("accountDisplay", () => {
  const display = shallowRef<Display>();
  const refused = shallowRef<ApiError>();
  let writing = false;

  async function read(): Promise<void> {
    try {
      const now = await call((signal) => server.api.GET("/api/v1/settings/display", { signal }));
      // A write under way is what the page shows until it is answered.
      if (!writing) display.value = now;
    } catch {
      // No server, or no session yet: what was read stays.
    }
  }
  void read();
  const timer = setInterval(() => void read(), EVERY);
  onScopeDispose(() => clearInterval(timer));

  /** The preferences written: shown at once; refused, the last come back, with why in `refused`. */
  async function write(value: Display): Promise<void> {
    const before = display.value;
    [display.value, refused.value, writing] = [value, undefined, true];
    try {
      await call((signal) => server.api.PUT("/api/v1/settings/display", { body: value, signal }));
    } catch (thrown) {
      display.value = before;
      refused.value = thrown instanceof ApiError ? thrown : new ApiError("server", String(thrown));
    } finally {
      writing = false;
    }
  }

  return { display, write, refused, read };
});
