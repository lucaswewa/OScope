import { expect, test } from "./fixtures.ts";

// The server's version, which the first page showed (W0.1), is About's now
// (W2.1): its screenshots in W0.1's notes are of that first page.
test("About shows the server's version, loses it, and finds it again @main", async ({
  page,
  server,
}) => {
  await page.goto(`${server.url}/#/about`);
  await expect(page.getByRole("heading", { name: "About", level: 1 })).toBeVisible();
  const status = page.getByRole("main").getByRole("status");
  await expect(status).toHaveText(`Server ${server.version}`);

  await server.stop();
  await expect(status).toHaveText("The server can't be reached.");

  // The same server, back at the same address.
  await server.start();
  await expect(status).toHaveText(`Server ${server.version}`);
});
