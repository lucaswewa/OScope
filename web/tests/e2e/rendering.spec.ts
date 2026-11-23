// Covers: web/src/api/preview
// A browser qualification: scientific colour/mask assertions require WebGL2.
import { expect, test } from "./fixtures.ts";

test("the qualified browser has WebGL2 for colour and saturation checks @main", async ({
  page,
}) => {
  const supported = await page.evaluate(
    () => !!document.createElement("canvas").getContext("webgl2"),
  );
  expect(supported, "run the browser suite through its virtual-display launcher").toBe(true);
});
