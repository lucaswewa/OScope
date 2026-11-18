import { expect, shot, test } from "./fixtures.ts";

test("the first page shows the server's version, loses it, and finds it again @main", async ({
  page,
  server,
}) => {
  await page.goto(server.url);
  await expect(page.getByRole("heading", { name: "OScope" })).toBeVisible();
  const status = page.getByRole("main").getByRole("status");
  await expect(status).toHaveText(`Server ${server.version}`);
  await shot(page, "W0.1-connected");

  await server.stop();
  await expect(status).toHaveText("The server can't be reached.");
  await shot(page, "W0.1-unreachable");

  // The same server, back at the same address.
  await server.start();
  await expect(status).toHaveText(`Server ${server.version}`);
});
