// The account's display preferences (W8.1): its theme, units, scale bar and
// colour map, which the server keeps for each account, or its defaults for
// one without its own. Read again every few seconds, so that a change made
// in another browser reaches this one.
import { defineStore } from "pinia";

import { useProperty } from "@/api/things/use";

export const useAccountDisplay = defineStore("accountDisplay", () => {
  const display = useProperty("/api/v1/settings/display", { every: 5_000 });
  return { display: display.value, write: display.write, refused: display.refused };
});
