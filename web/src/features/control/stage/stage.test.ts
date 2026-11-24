// The stage's arithmetic (W3.3): the limits' check, the keys' map, lengths
// in the display's unit, and pointing through a calibration's matrix.
import { describe, expect, it } from "vitest";

import {
  beyond,
  geometryOf,
  keyStep,
  moveFor,
  sideOf,
  movedBy,
  plus,
  stepBy,
  waitsForHoming,
  within,
  written,
  type Geometry,
  type Limits,
} from "./stage";

/** The simulated stage's travel: 100 × 75 × 20 mm. */
const TRAVEL: Limits = {
  x: { min: 0, max: 100_000 },
  y: { min: 0, max: 75_000 },
  z: { min: 0, max: 20_000 },
};

describe("the limits", () => {
  it("take a place within, or on, them", () => {
    expect(beyond({ x: 50_000, y: 37_500, z: 10_000 }, TRAVEL)).toEqual([]);
    expect(beyond({ x: 0, y: 75_000, z: 20_000 }, TRAVEL)).toEqual([]);
  });

  it("name each axis a place is past them on", () => {
    expect(beyond({ x: 100_000.1, y: 37_500, z: -0.1 }, TRAVEL)).toEqual(["x", "z"]);
    expect(beyond(plus({ x: 99_950, y: 10, z: 5 }, { x: 100, y: -20 }), TRAVEL)).toEqual([
      "x",
      "y",
    ]);
  });

  it("check only the axes a move moves along, and bring a place within them", () => {
    // Homed, the stage is a micrometre past x's least and z's greatest.
    const homed = { x: -1, y: 37_500, z: 20_001 };
    expect(beyond(plus(homed, { y: 100 }), TRAVEL, movedBy({ y: 100 }))).toEqual([]);
    expect(beyond(plus(homed, { x: -100 }), TRAVEL, movedBy({ x: -100 }))).toEqual(["x"]);
    expect(movedBy({ x: 0, y: -5 })).toEqual(["y"]);
    expect([within(-1, TRAVEL.x), within(20_001, TRAVEL.z), within(5, TRAVEL.x)]).toEqual([
      0, 20_000, 5,
    ]);
  });
});

describe("the keys on the image", () => {
  it("step along x and y with the arrows, and along z with the page keys", () => {
    const keys = ["ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp", "PageUp", "PageDown"];
    expect(keys.map((key) => keyStep(key))).toEqual([
      { axis: "x", towardsMax: true },
      { axis: "x", towardsMax: false },
      { axis: "y", towardsMax: true },
      { axis: "y", towardsMax: false },
      { axis: "z", towardsMax: true },
      { axis: "z", towardsMax: false },
    ]);
  });

  it("leave every other key alone, the object's own names too", () => {
    for (const key of ["+", "0", "f", "Home", "Escape", " ", "toString", "constructor"]) {
      expect(keyStep(key)).toBeUndefined();
    }
  });

  it("move by the step, its sign the way they go", () => {
    expect(stepBy({ axis: "y", towardsMax: false }, 100)).toEqual({ y: -100 });
    expect(stepBy({ axis: "z", towardsMax: true }, 0.5)).toEqual({ z: 0.5 });
  });
});

describe("lengths in the display's unit", () => {
  it("are written to a tenth of a µm", () => {
    expect(written(12_345.67, "um")).toBe("12345.7");
    expect(written(12_345.67, "mm")).toBe("12.3457");
    expect(written(0, "mm")).toBe("0.0000");
  });
});

/** A camera turned by 20° on the stage, 0.98 µm a pixel, as P31's test finds the simulator's. */
function turned(binning = 1): Geometry {
  const angle = (-20 * Math.PI) / 180;
  const [cos, sin, side] = [Math.cos(angle), Math.sin(angle), 0.98 * binning];
  return {
    objective: "10x",
    binning,
    matrix: [
      [side * cos, -side * sin],
      [side * sin, side * cos],
    ],
    residual_px: 0.1,
  };
}

describe("pointing at the image", () => {
  it("moves the stage by the matrix times the pixels between the two places", () => {
    const geometry = turned();
    const move = moveFor(geometry, [2736 + 100, 1824], [2736, 1824]);
    const [[a], [c]] = geometry.matrix;
    expect(move.x).toBeCloseTo(100 * a, 9);
    expect(move.y).toBeCloseTo(100 * c, 9);
    // As long as the pixels are apart, whatever the turn.
    expect(Math.hypot(move.x!, move.y!)).toBeCloseTo(98, 9);
  });

  it("gives a sensor pixel's side at the sample, turned, mirrored or binned", () => {
    expect(sideOf(turned())).toBeCloseTo(0.98, 12);
    expect(sideOf(turned(2))).toBeCloseTo(0.98, 12);
    const mirrored = turned();
    mirrored.matrix = [
      [-mirrored.matrix[0][0], mirrored.matrix[0][1]],
      [-mirrored.matrix[1][0], mirrored.matrix[1][1]],
    ];
    expect(sideOf(mirrored)).toBeCloseTo(0.98, 12);
  });

  it("counts the image's pixels, binning of the sensor's each", () => {
    // Binned 2 × 2, a pixel of the image is twice as wide at the sample, and two of the sensor's.
    const one = moveFor(turned(1), [300, 400], [100, 100]);
    const two = moveFor(turned(2), [300, 400], [100, 100]);
    expect(two.x).toBeCloseTo(one.x!, 9);
    expect(two.y).toBeCloseTo(one.y!, 9);
  });

  it("brings a place to where it was asked to: the matrix's inverse takes the move back", () => {
    const geometry = turned();
    const from: [number, number] = [1000, 2500];
    const to: [number, number] = [2736, 1824];
    const { x, y } = moveFor(geometry, from, to) as { x: number; y: number };
    const [[a, b], [c, d]] = geometry.matrix;
    const det = a * d - b * c;
    // The content shifts by −d pixels for the move M · d.
    const shift = [-(d * x - b * y) / det, -(-c * x + a * y) / det];
    expect(from[0] + shift[0]!).toBeCloseTo(to[0], 6);
    expect(from[1] + shift[1]!).toBeCloseTo(to[1], 6);
  });

  it("takes the geometry from a set's values, and nothing else", () => {
    const values = { kind: "geometry", ...turned(), pixel_um: 0.98, rotation_deg: -20 };
    expect(geometryOf(values)).toEqual(turned());
    expect(geometryOf({ ...values, kind: "stage" })).toBeUndefined();
    expect(geometryOf({ ...values, matrix: [[1, 0]] })).toBeUndefined();
    expect(geometryOf({ ...values, binning: 0 })).toBeUndefined();
    expect(geometryOf(null)).toBeUndefined();
  });
});

describe("an alarm", () => {
  it("is waiting to be homed when its reason says homing is needed", () => {
    expect(waitsForHoming("homing required")).toBe(true);
    expect(waitsForHoming("Homing required. Execute homing command ($H) to continue.")).toBe(true);
    expect(waitsForHoming("hard limit")).toBe(false);
    expect(waitsForHoming("the stage is not homed")).toBe(true);
    // Homing that failed is an alarm of its own, its reason said.
    expect(waitsForHoming("Homing fail. Could not find the limit switch.")).toBe(false);
  });
});
