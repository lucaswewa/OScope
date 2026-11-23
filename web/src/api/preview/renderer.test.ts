// The preview's renderer (W1.5) without WebGL2: a 2D canvas, the frame drawn
// where the view places it, each bitmap closed once another replaces it.
// WebGL2's own is drawn in a browser, end to end.
import { describe, expect, it, vi } from "vitest";

import { createRenderer } from "./renderer.ts";

const bitmap = () => ({ close: vi.fn() }) as unknown as ImageBitmap;

describe("the preview's renderer", () => {
  it("draws on a 2D canvas where WebGL2 is missing, as the view places the frame", () => {
    const drawn: unknown[][] = [];
    const context = {
      clearRect: vi.fn(),
      drawImage: (...args: unknown[]) => drawn.push(args),
      imageSmoothingEnabled: true,
    };
    const canvas = {
      clientWidth: 400,
      clientHeight: 300,
      width: 0,
      height: 0,
      getContext: (kind: string) => (kind === "2d" ? context : null),
    } as unknown as HTMLCanvasElement;
    const renderer = createRenderer(canvas);
    expect(renderer.kind).toBe("2d");
    expect(renderer.size()).toEqual({ width: 400, height: 300 });
    expect([canvas.width, canvas.height]).toEqual([400, 300]);

    const [first, second] = [bitmap(), bitmap()];
    const frame = { x: 100, y: 100, width: 800, height: 600 };
    renderer.show(first, frame);
    renderer.show(second, frame);
    expect(first.close).toHaveBeenCalled();
    renderer.draw({ centre: [500, 400], scale: 0.5 });
    expect(context.clearRect).toHaveBeenCalledWith(0, 0, 400, 300);
    expect(drawn).toEqual([[second, 0, 0, 400, 300]]);
    // Past one to one, the sensor's pixels are squares (W3.1).
    expect(context.imageSmoothingEnabled).toBe(true);
    renderer.draw({ centre: [500, 400], scale: 4 });
    expect(context.imageSmoothingEnabled).toBe(false);
    renderer.dispose();
    expect(second.close).toHaveBeenCalled();
  });
});
