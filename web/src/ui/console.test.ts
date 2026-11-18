import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { h } from "vue";

import { heard } from "../../tests/setup/unit";

// The unit tests' rule that a warning fails the test that caused it
// (tests/setup/unit.ts), shown to hold.
describe("the console during a test", () => {
  it("is heard: what is said there is kept for the end of the test", () => {
    console.warn("a warning");
    console.error("an error");
    expect(heard()).toEqual(["console.warn: a warning", "console.error: an error"]);
  });

  it("hears Vue say that a listener a page put on a component went nowhere", () => {
    const two = { render: () => [h("i"), h("b")] };
    mount({ render: () => h(two, { onClick: () => {} }) });
    expect(heard().join(" ")).toContain("Extraneous non-emits event listeners (click)");
  });
});
