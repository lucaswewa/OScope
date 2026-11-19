// The preview drawn on a canvas (W1.5, ADR-0047): with WebGL2, one textured
// quad that the view places; with a 2D canvas where WebGL2 is missing.
import type { PixelRect } from "./message.ts";
import { placed, type Size, type View } from "./view.ts";

/** What draws the frames. */
export interface Renderer {
  /** How it draws. */
  readonly kind: "webgl2" | "2d";
  /** Takes `bitmap`, of the sensor's pixels `rect`, for each draw from now on; it closes the bitmap when done with it. */
  show(bitmap: ImageBitmap, rect: PixelRect): void;
  /** Draws what it was last shown, as `view` places it, at the canvas's size now. */
  draw(view: View): void;
  /** The canvas's size, in device pixels, its drawing buffer made to match. */
  size(): Size;
  dispose(): void;
}

/** The renderer for `canvas`: WebGL2's, or a 2D canvas's without it. */
export function createRenderer(canvas: HTMLCanvasElement): Renderer {
  const gl = canvas.getContext("webgl2", { alpha: false, antialias: false, depth: false });
  return gl ? webgl(canvas, gl) : flat(canvas);
}

/** The canvas's size in device pixels, its drawing buffer made to match. */
function fit(canvas: HTMLCanvasElement): Size {
  const ratio = globalThis.devicePixelRatio || 1;
  const width = Math.max(1, Math.round(canvas.clientWidth * ratio));
  const height = Math.max(1, Math.round(canvas.clientHeight * ratio));
  if (canvas.width !== width) canvas.width = width;
  if (canvas.height !== height) canvas.height = height;
  return { width, height };
}

const VERTEX = `#version 300 es
in vec2 corner;
uniform vec4 place;
out vec2 uv;
void main() {
  uv = corner;
  gl_Position = vec4(place.xy + corner * place.zw, 0.0, 1.0);
}`;

const FRAGMENT = `#version 300 es
precision mediump float;
uniform sampler2D image;
in vec2 uv;
out vec4 colour;
void main() {
  colour = texture(image, uv);
}`;

/** The quad's program, its corners and its texture: made again after the context is lost and restored. */
function build(gl: WebGL2RenderingContext) {
  const program = gl.createProgram();
  for (const [type, source] of [
    [gl.VERTEX_SHADER, VERTEX],
    [gl.FRAGMENT_SHADER, FRAGMENT],
  ] as const) {
    const shader = gl.createShader(type);
    if (!shader) throw new Error("the preview's shaders couldn't be made");
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    gl.attachShader(program, shader);
  }
  gl.linkProgram(program);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    throw new Error(`the preview's shaders didn't link: ${gl.getProgramInfoLog(program)}`);
  }
  gl.useProgram(program);
  gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
  const corner = gl.getAttribLocation(program, "corner");
  gl.enableVertexAttribArray(corner);
  gl.vertexAttribPointer(corner, 2, gl.FLOAT, false, 0, 0);
  const texture = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, texture);
  for (const [name, value] of [
    [gl.TEXTURE_MIN_FILTER, gl.LINEAR],
    [gl.TEXTURE_MAG_FILTER, gl.LINEAR],
    [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE],
    [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE],
  ] as const) {
    gl.texParameteri(gl.TEXTURE_2D, name, value);
  }
  return { texture, place: gl.getUniformLocation(program, "place") };
}

function webgl(canvas: HTMLCanvasElement, gl: WebGL2RenderingContext): Renderer {
  let parts = build(gl);
  let rect: PixelRect | undefined;
  // A lost context comes back if asked to: then its program is made again,
  // and the next frame fills its texture.
  const lost = (event: Event) => event.preventDefault();
  const restored = () => {
    parts = build(gl);
    rect = undefined;
  };
  canvas.addEventListener("webglcontextlost", lost);
  canvas.addEventListener("webglcontextrestored", restored);
  return {
    kind: "webgl2",
    show(bitmap, shows) {
      if (!gl.isContextLost()) {
        gl.bindTexture(gl.TEXTURE_2D, parts.texture);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, bitmap);
        rect = shows;
      }
      bitmap.close();
    },
    draw(view) {
      const size = fit(canvas);
      if (gl.isContextLost()) return;
      gl.viewport(0, 0, size.width, size.height);
      gl.clearColor(0, 0, 0, 1);
      gl.clear(gl.COLOR_BUFFER_BIT);
      if (!rect) return;
      const at = placed(view, rect, size);
      // From the canvas's pixels to clip space: x from -1 at the left, y
      // from 1 at the top, the image's first row at its top.
      gl.uniform4f(
        parts.place,
        (2 * at.x) / size.width - 1,
        1 - (2 * at.y) / size.height,
        (2 * at.width) / size.width,
        (-2 * at.height) / size.height,
      );
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    },
    size: () => fit(canvas),
    dispose() {
      canvas.removeEventListener("webglcontextlost", lost);
      canvas.removeEventListener("webglcontextrestored", restored);
      gl.getExtension("WEBGL_lose_context")?.loseContext();
    },
  };
}

function flat(canvas: HTMLCanvasElement): Renderer {
  const context = canvas.getContext("2d");
  let shown: { bitmap: ImageBitmap; rect: PixelRect } | undefined;
  return {
    kind: "2d",
    show(bitmap, rect) {
      shown?.bitmap.close();
      shown = { bitmap, rect };
    },
    draw(view) {
      const size = fit(canvas);
      if (!context) return;
      // Cleared, the canvas's own background shows: the page's token for it.
      context.clearRect(0, 0, size.width, size.height);
      if (!shown) return;
      const at = placed(view, shown.rect, size);
      context.drawImage(shown.bitmap, at.x, at.y, at.width, at.height);
    },
    size: () => fit(canvas),
    dispose() {
      shown?.bitmap.close();
      shown = undefined;
    },
  };
}
