// The session at the app's start and as it goes (W1.3): its token kept
// until the window is closed, by the host (ADR-0046), and taken up again
// on a reload; a session that ends returns to the login, then to where the
// user was.
import { watch } from "vue";
import type { Router } from "vue-router";

import { useSession } from "@/api/connection/session";
import { host } from "@/host/host";

/** The name the token is kept under. */
const TOKEN = "oscope.session";

/** Takes up the token kept from before, if the server still knows it, and keeps each new one. */
export async function keepSession(): Promise<void> {
  const session = useSession();
  session.keepWith((token) => host().session.write(TOKEN, token));
  const kept = host().session.read(TOKEN);
  if (kept) await session.resume(kept);
  else await session.probe().catch(() => undefined);
}

/** Sends the user to the login whenever the session is out: at the start, or when it ends. */
export function guardSession(router: Router): void {
  const session = useSession();
  router.beforeEach(async (to) => {
    if (session.state === "unknown") await session.probe().catch(() => undefined);
    if (session.state === "out" && to.name !== "login") {
      return { name: "login", query: { then: to.fullPath } };
    }
    return true;
  });
  // A session that ends while a page is on its way, its code still loading,
  // is seen at its arrival: the watch below saw the page it came from.
  router.afterEach((to) => {
    if (session.state === "out" && to.name !== "login") {
      void router.push({ name: "login", query: { then: to.fullPath } });
    }
  });
  watch(
    () => session.state,
    (state) => {
      const here = router.currentRoute.value;
      if (state === "out" && here.name !== "login") {
        void router.push({ name: "login", query: { then: here.fullPath } });
      }
    },
  );
}
