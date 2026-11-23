// The session's guard (W1.3): a page without a session goes to the login,
// and so does one reached as its session ended, on its way from the login.
import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import { useSession } from "@/api/connection/session";
import { guardSession } from "@/features/login/keep";

const page = { render: () => null };

describe("the session's guard", () => {
  beforeEach(() => setActivePinia(createPinia()));

  it("sends to the login a page reached as its session ended, then back to it", async () => {
    const session = useSession();
    session.state = "out";
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: "/login", name: "login", component: page },
        { path: "/connection", name: "connection", component: page },
      ],
    });
    guardSession(router);
    await router.push("/connection");
    expect(router.currentRoute.value.fullPath).toBe("/login?then=/connection");

    session.state = "in";
    // The session ends while the page is on its way: its code loading.
    const ending = router.beforeResolve(() => {
      session.state = "out";
    });
    await router.push("/connection");
    ending();
    await vi.waitFor(() => expect(router.currentRoute.value.name).toBe("login"));
    expect(router.currentRoute.value.query["then"]).toBe("/connection");
  });
});
