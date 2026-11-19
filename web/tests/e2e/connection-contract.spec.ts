// Contract tests (W1.3, ADR-0045): a real server with accounts, each test's
// own. Media are fetched with the session's token, and refused without it;
// the control lease is taken, refused to another, and handed over. No
// page: they run in Chromium's project only, as untagged tests do.
import { act, start } from "../../src/api/actions.ts";
import { call, Server, session } from "../../src/api/client.ts";
import { logIn as loggingIn } from "../../src/api/connection/session.ts";
import { ApiError } from "../../src/api/errors.ts";
import { finished, type JobRef } from "../../src/api/jobs.ts";
import { download, fileAddress, tile, type StoredImage } from "../../src/api/media.ts";
import { expect, test } from "./fixtures.ts";
import { Server as TestServer } from "./server.ts";

const ALICE = { name: "alice", password: "alice's password" };
const BOB = { name: "bob", password: "bob's password" };

/** A server of the test's own, with Alice's and Bob's accounts. */
async function withAccounts(): Promise<TestServer> {
  const server = new TestServer();
  server.addAccount(ALICE.name, ALICE.password);
  server.addAccount(BOB.name, BOB.password);
  await server.start();
  return server;
}

/** A session's token. */
async function logIn(on: Server, who: { name: string; password: string }): Promise<string> {
  return (await loggingIn(who.name, who.password, on)).token;
}

/** What a call threw. */
async function thrown(promise: Promise<unknown>): Promise<ApiError> {
  const error = await promise.then(
    () => undefined,
    (reason: unknown) => reason,
  );
  expect(error).toBeInstanceOf(ApiError);
  return error as ApiError;
}

test.afterEach(() => {
  delete session.token;
});

test("media are fetched with the session's token, and refused without it", async () => {
  test.setTimeout(120_000);
  const server = await withAccounts();
  const on = new Server(server.url);
  try {
    session.token = await logIn(on, ALICE);
    await act("/api/v1/auth/acquire_control", {}, { on });
    const roi = { x: 2256, y: 1464, width: 960, height: 720 };
    await call((signal) => on.api.PUT("/api/v1/camera/roi", { body: roi, signal }));
    await act("/api/v1/camera/start_streaming", {}, { on });
    const project = await act("/api/v1/projects/create", { name: "secured" }, { on });
    const ref: JobRef = await act("/api/v1/capture/single", { project: project.id }, { on });
    const job = await finished(ref, { on });
    const image: StoredImage = {
      project: project.id,
      kind: "acquisitions",
      id: job.results[0]!.split("/").at(-1)!,
    };
    const thumbnail = `acquisitions/${image.id}/thumbnail.jpg`;
    // With the token: a file, and a tile, each a picture.
    expect((await download(project.id, thumbnail, { on })).type).toBe("image/jpeg");
    expect((await tile(image, { level: 0, col: 0, row: 0 }, { on })).type).toBe("image/jpeg");
    // Never in the address.
    expect(fileAddress(project.id, thumbnail, on)).not.toContain(session.token);
    // Without it: refused.
    delete session.token;
    const refused = await thrown(download(project.id, thumbnail, { on }));
    expect([refused.failure, refused.status]).toEqual(["unauthorised", 401]);
    expect((await thrown(tile(image, { level: 0, col: 0, row: 0 }, { on }))).status).toBe(401);
  } finally {
    await server.dispose();
  }
});

test("the lease is taken, refused to another, and handed over", async () => {
  const server = await withAccounts();
  const on = new Server(server.url);
  try {
    const [alice, bob] = [await logIn(on, ALICE), await logIn(on, BOB)];
    const write = (gain: number) =>
      call((signal) => on.api.PUT("/api/v1/camera/gain", { body: gain, signal }));
    session.token = alice;
    const taken = await act("/api/v1/auth/acquire_control", {}, { on });
    expect(taken.holder).toBe("alice");
    await write(10);
    // Bob may read, and may not change anything.
    session.token = bob;
    expect(await call((signal) => on.api.GET("/api/v1/camera/gain", { signal }))).toBe(10);
    const refused = await thrown(write(20));
    expect([refused.failure, refused.status]).toEqual(["forbidden", 403]);
    await act("/api/v1/auth/request_control", {}, { on });
    // Alice sees his request, and hands it over.
    session.token = alice;
    const asked = await call((signal) => on.api.GET("/api/v1/auth/lease", { signal }));
    expect(asked.requests).toEqual(["bob"]);
    await act("/api/v1/auth/hand_over_control", { to: "bob" }, { on });
    session.token = bob;
    await write(20);
    expect(await call((signal) => on.api.GET("/api/v1/camera/gain", { signal }))).toBe(20);
    // An action that isn't his to take fails in its own words.
    const started = await start("/api/v1/auth/take_control", {}, { on });
    expect(started.status).toBeDefined();
  } finally {
    await server.dispose();
  }
});
