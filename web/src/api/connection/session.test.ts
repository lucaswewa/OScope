// The session (W1.3): open without accounts; out, in, and ended with them;
// its token with every call, and told to its keepers. On a stand-in
// network.
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { problem, standInNetwork } from "../../../tests/setup/stand-ins.ts";
import { call, server, session } from "../client.ts";
import { useSession } from "./session.ts";

let tokens: Set<string>;
let made: Request[];

const known = (request: Request) =>
  tokens.has(request.headers.get("authorization")?.replace("Bearer ", "") ?? "");

beforeEach(() => {
  setActivePinia(createPinia());
  tokens = new Set();
  made = standInNetwork({
    "GET /api/v1/auth/me": (request) =>
      known(request)
        ? { name: "alice", control: false }
        : problem(401, "Unauthorized", "a session's token is needed"),
    "POST /api/v1/auth/login": async (request) => {
      const { name, password } = (await request.json()) as { name: string; password: string };
      if (name !== "alice" || password !== "alice's password") {
        return problem(401, "Unauthorized", "no such name and password");
      }
      tokens.add("t1");
      return { token: "t1", name };
    },
    "POST /api/v1/auth/logout": (request) => {
      tokens.delete(request.headers.get("authorization")!.replace("Bearer ", ""));
      return new Response(null, { status: 204 });
    },
    "GET /api/v1/projects/list": (request) =>
      known(request) ? [] : problem(401, "Unauthorized", "a session's token is needed"),
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  delete session.token;
});

describe("the session", () => {
  it("is open when the server has no accounts, and no `auth` Thing", async () => {
    standInNetwork({ "GET /api/v1/auth/me": () => problem(404, "Not Found", "no such Thing") });
    expect(await useSession().probe()).toBe("open");
  });

  it("is out when the server has accounts and the app no token", async () => {
    expect(await useSession().probe()).toBe("out");
  });

  it("logs in: its token goes with every call, and its keepers are told", async () => {
    const kept: (string | undefined)[] = [];
    const sessions = useSession();
    sessions.keepWith((token) => kept.push(token));
    await sessions.login("alice", "alice's password");
    expect([sessions.state, sessions.name, kept]).toEqual(["in", "alice", ["t1"]]);
    await call((signal) => server.api.GET("/api/v1/projects/list", { signal }));
    expect(made.at(-1)!.headers.get("authorization")).toBe("Bearer t1");
  });

  it("refuses a wrong password with the server's 401, and stays out", async () => {
    const sessions = useSession();
    await sessions.probe();
    await expect(sessions.login("alice", "not her password")).rejects.toMatchObject({
      status: 401,
    });
    expect([sessions.state, session.token]).toEqual(["out", undefined]);
  });

  it("logs out: the session ends on the server, and here", async () => {
    const kept: (string | undefined)[] = [];
    const sessions = useSession();
    sessions.keepWith((token) => kept.push(token));
    await sessions.login("alice", "alice's password");
    await sessions.logout();
    expect([sessions.state, sessions.ended, session.token]).toEqual(["out", false, undefined]);
    expect(kept).toEqual(["t1", undefined]);
    expect(tokens.size).toBe(0);
  });

  it("ends when the server says so, at a call's 401 while logged in", async () => {
    const sessions = useSession();
    await sessions.login("alice", "alice's password");
    tokens.clear();
    await expect(
      call((signal) => server.api.GET("/api/v1/projects/list", { signal })),
    ).rejects.toMatchObject({ failure: "unauthorised" });
    expect([sessions.state, sessions.ended, session.token]).toEqual(["out", true, undefined]);
  });

  it("takes up a token kept from before, if the server still knows it", async () => {
    tokens.add("kept");
    expect(await useSession().resume("kept")).toBe("in");
    expect(session.token).toBe("kept");
    setActivePinia(createPinia());
    expect(await useSession().resume("forgotten")).toBe("out");
    expect(session.token).toBeUndefined();
  });

  it("isn't ended by a 401 when the server has no accounts", async () => {
    standInNetwork({ "GET /api/v1/auth/me": () => problem(404, "Not Found", "no such Thing") });
    const sessions = useSession();
    await sessions.probe();
    standInNetwork({ "GET /api/v1/projects/list": () => problem(401, "Unauthorized", "odd") });
    await call((signal) => server.api.GET("/api/v1/projects/list", { signal })).catch(() => {});
    expect(sessions.state).toBe("open");
  });
});
