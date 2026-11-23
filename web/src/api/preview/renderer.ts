// The preview drawn on a canvas (W1.5, ADR-0047): with WebGL2, one textured
// quad that the view places; with a 2D canvas where WebGL2 is missing.
import type { PixelRect, SaturationMask } from "./message.ts";
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
  /**
   * Colours each pixel by `table`, 256 sRGB colours, on its luminance (W3.6):
   * the frame's own colours without. WebGL2's only.
   */
  colour(table: Uint8Array | undefined): void;
  /** Marks the pixels `mask` sets, the frame's saturated ones, from now on: none without. WebGL2's only. */
  mark(mask: SaturationMask | undefined): void;
  dispose(): void;
}

/**
 * Whether the sensor's pixels are drawn as squares: past one to one, where
 * a sensor pixel spans more than a device pixel, rather than smoothed (W3.1).
 */
export function squares(view: View): boolean {
  return view.scale > 1;
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

// False colour on the luminance, Rec. 709's weights, through a map's 256
// colours; then the saturated pixels marked in red, over any map.
const FRAGMENT = `#version 300 es
precision mediump float;
uniform sampler2D image;
uniform sampler2D map;
uniform sampler2D saturated;
uniform bool mapped;
uniform bool marking;
in vec2 uv;
out vec4 colour;
void main() {
  vec4 seen = texture(image, uv);
  if (mapped) {
    float y = dot(seen.rgb, vec3(0.2126, 0.7152, 0.0722));
    seen = vec4(texture(map, vec2((y * 255.0 + 0.5) / 256.0, 0.5)).rgb, 1.0);
  }
  if (marking && texture(saturated, uv).r > 0.5) seen = vec4(1.0, 0.0, 0.0, 1.0);
  colour = seen;
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
  const texture = imageTexture(gl);
  const extra = (unit: number, uniform: string) => {
    const made = gl.createTexture();
    gl.activeTexture(gl.TEXTURE0 + unit);
    gl.bindTexture(gl.TEXTURE_2D, made);
    for (const [name, value] of [
      [gl.TEXTURE_MIN_FILTER, gl.NEAREST],
      [gl.TEXTURE_MAG_FILTER, gl.NEAREST],
      [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE],
      [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE],
    ] as const) {
      gl.texParameteri(gl.TEXTURE_2D, name, value);
    }
    gl.uniform1i(gl.getUniformLocation(program, uniform), unit);
    gl.activeTexture(gl.TEXTURE0);
    return made;
  };
  return {
    texture,
    map: extra(1, "map"),
    saturated: extra(2, "saturated"),
    place: gl.getUniformLocation(program, "place"),
    mapped: gl.getUniformLocation(program, "mapped"),
    marking: gl.getUniformLocation(program, "marking"),
  };
}

/** A texture for the frames, smoothed and clamped, bound on the unit in use. */
function imageTexture(gl: WebGL2RenderingContext): WebGLTexture {
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
  return texture;
}

function webgl(canvas: HTMLCanvasElement, gl: WebGL2RenderingContext): Renderer {
  let parts = build(gl);
  let rect: PixelRect | undefined;
  /** The magnifying filter set: changed only when it must be, a cost on software GL. */
  let filter: number = gl.LINEAR;
  // A lost context comes back if asked to: then its program is made again,
  // and the next frame fills its texture.
  const lost = (event: Event) => event.preventDefault();
  /** The map chosen, given again to a context made again. */
  let table: Uint8Array | undefined;
  const restored = () => {
    parts = build(gl);
    rect = undefined;
    filter = gl.LINEAR;
    if (table) colour(table);
  };
  /** Gives the program a map's colours, on the texture unit 1: none for the frame's own. */
  function colour(chosen: Uint8Array | undefined): void {
    if (gl.isContextLost()) return;
    gl.uniform1i(parts.mapped, chosen ? 1 : 0);
    if (!chosen) return;
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, parts.map);
    // As RGBA: WebKit's WebGL samples a three-byte RGB texture as black.
    const rgba = new Uint8Array(256 * 4).fill(255);
    for (let i = 0; i < 256; i++) rgba.set(chosen.subarray(i * 3, i * 3 + 3), i * 4);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 256, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, rgba);
    gl.activeTexture(gl.TEXTURE0);
  }
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
      const wanted = squares(view) ? gl.NEAREST : gl.LINEAR;
      if (wanted !== filter) {
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, wanted);
        filter = wanted;
      }
      // From the canvas's pixels to clip space: x from -1 at the left, y
      // from 1 at the top, the image's first row at its top.
      gl.uniform4f(
        parts.place,
        (2 * at.x) / size.width - 1,
        1 - (2 * at.y) / size.height,
        (2 * at.width) / size.width,
        (-2 * at.height) / size.height,
      );
      // Each texture bound to its unit for each draw: WebKit's upload of a bitmap
      // may leave another unit's binding changed, and the map would read as black.
      gl.activeTexture(gl.TEXTURE1);
      gl.bindTexture(gl.TEXTURE_2D, parts.map);
      gl.activeTexture(gl.TEXTURE2);
      gl.bindTexture(gl.TEXTURE_2D, parts.saturated);
      gl.activeTexture(gl.TEXTURE0);
      gl.bindTexture(gl.TEXTURE_2D, parts.texture);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    },
    size: () => fit(canvas),
    colour(chosen) {
      table = chosen;
      colour(chosen);
    },
    mark(mask) {
      if (gl.isContextLost()) return;
      gl.uniform1i(parts.marking, mask ? 1 : 0);
      if (!mask) return;
      gl.activeTexture(gl.TEXTURE2);
      gl.bindTexture(gl.TEXTURE_2D, parts.saturated);
      gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
      gl.texImage2D(
        gl.TEXTURE_2D,
        0,
        gl.R8,
        mask.width,
        mask.height,
        0,
        gl.RED,
        gl.UNSIGNED_BYTE,
        unpacked(mask),
      );
      gl.activeTexture(gl.TEXTURE0);
    },
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
      context.imageSmoothingEnabled = !squares(view);
      context.drawImage(shown.bitmap, at.x, at.y, at.width, at.height);
    },
    size: () => fit(canvas),
    // The 2D canvas draws the frame's own colours, and marks nothing.
    colour: () => undefined,
    mark: () => undefined,
    dispose() {
      shown?.bitmap.close();
      shown = undefined;
    },
  };
}

/** The mask's bits as a byte a pixel: 255 where set, for an R8 texture. */
export function unpacked(mask: SaturationMask): Uint8Array {
  const bytes = new Uint8Array(mask.width * mask.height);
  for (let row = 0; row < mask.height; row++) {
    for (let x = 0; x < mask.width; x++) {
      const byte = mask.bits[row * mask.stride + (x >> 3)]!;
      if (byte & (0x80 >> (x & 7))) bytes[row * mask.width + x] = 255;
    }
  }
  return bytes;
}
