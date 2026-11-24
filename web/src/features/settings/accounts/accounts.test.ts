import { describe, expect, it } from "vitest";

import { nameWrong, passwordWrong, spanOf } from "@/features/settings/accounts/accounts";

describe("a new account", () => {
  it("has a name as the server allows: a letter first, then letters, digits, `.`, `_` or `-`", () => {
    for (const fine of ["ana", "a", "ben.smith", "c_2-d", "z".repeat(64)]) {
      expect(nameWrong(fine), fine).toBeUndefined();
    }
    expect(nameWrong("")).toBe("empty");
    expect(nameWrong("local")).toBe("local");
    for (const wrong of ["Ana", "2ana", "ana smith", "anaé", "-ana", "z".repeat(65)]) {
      expect(nameWrong(wrong), wrong).toBe("form");
    }
  });

  it("has a password of 8 characters at least, the same typed twice", () => {
    expect(passwordWrong("eight ch", "eight ch")).toBeUndefined();
    expect(passwordWrong("seven c", "seven c")).toBe("short");
    // Counted in characters, as the server counts them, not in UTF-16's units.
    expect(passwordWrong("🔑🔑🔑🔑🔑🔑🔑", "🔑🔑🔑🔑🔑🔑🔑")).toBe("short");
    expect(passwordWrong("a long password", "a long passwrd")).toBe("differ");
  });
});

describe("an idle time", () => {
  it("is seconds under a minute, else hours and minutes", () => {
    expect(spanOf(0)).toEqual({ h: 0, min: 0, s: 0 });
    expect(spanOf(59.9)).toEqual({ h: 0, min: 0, s: 59 });
    expect(spanOf(60)).toEqual({ h: 0, min: 1, s: 0 });
    expect(spanOf(3_725)).toEqual({ h: 1, min: 2, s: 0 });
    expect(spanOf(8 * 3_600)).toEqual({ h: 8, min: 0, s: 0 });
    expect(spanOf(-3)).toEqual({ h: 0, min: 0, s: 0 });
  });
});
