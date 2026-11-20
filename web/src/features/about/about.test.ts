// About (W4.3): the server's time up, said in its two largest units.
import { describe, expect, it } from "vitest";

import { upFor } from "./about";

describe("the server's time up", () => {
  it("is said in its two largest units, whole", () => {
    const seconds = [0, 59.9, 61, 3_599, 3_600, 3_660, 86_399, 90_061, 2 * 86_400 + 7_200];
    expect(seconds.map(upFor)).toEqual([
      "0 s",
      "59 s",
      "1 min 1 s",
      "59 min 59 s",
      "1 h 0 min",
      "1 h 1 min",
      "23 h 59 min",
      "1 d 1 h",
      "2 d 2 h",
    ]);
  });
});
