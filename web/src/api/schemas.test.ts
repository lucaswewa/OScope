import { describe, expect, it } from "vitest";

import { publishedIn } from "@/api/schemas";

describe("the schemas in Thing Descriptions", () => {
  it("are each property's, action's and event's, by Thing and name", () => {
    const camera = {
      id: "urn:dev:camera",
      title: "Camera",
      properties: {
        exposure: {
          type: "number",
          minimum: 0,
          observable: true,
          readOnly: false,
          forms: [{ href: "/api/v1/camera/exposure", op: ["readproperty", "writeproperty"] }],
        },
      },
      actions: {
        set_mode: {
          input: { type: "object", properties: { mode: { type: "string" } } },
          output: { type: "null" },
          synchronous: true,
          forms: [
            { href: "/api/v1/camera/set_mode", op: ["invokeaction"] },
            { href: "/api/v1/action_invocations/{id}", op: ["queryaction", "cancelaction"] },
          ],
        },
        stop: { forms: [{ href: "/api/v1/camera/stop" }] },
      },
      events: { frame: { data: { type: "integer" }, forms: [] }, ended: { forms: [] } },
    };
    expect(publishedIn([camera, "junk", null])).toEqual([
      {
        id: "Camera/exposure/property",
        label: "Camera · exposure · property",
        part: "property",
        schema: { type: "number", minimum: 0, readOnly: false },
      },
      {
        id: "Camera/set_mode/input",
        label: "Camera · set_mode · input",
        part: "input",
        schema: { type: "object", properties: { mode: { type: "string" } } },
        href: "api/v1/camera/set_mode",
      },
      {
        id: "Camera/set_mode/output",
        label: "Camera · set_mode · output",
        part: "output",
        schema: { type: "null" },
      },
      {
        id: "Camera/frame/event",
        label: "Camera · frame · event",
        part: "event",
        schema: { type: "integer" },
      },
    ]);
  });

  it("go by a Thing's id where it has no title", () => {
    const found = publishedIn([{ id: "stage", properties: { x: { type: "number" } } }]);
    expect(found.map((one) => one.id)).toEqual(["stage/x/property"]);
  });
});
