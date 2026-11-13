// The live view with overlays: the preview Thing's stream, with live
// statistics of the raw frames (ADR-0035). WebGL draws the image, false
// colour through a lookup table, and the saturation mask; a 2D canvas over
// it draws the crosshair, the ROI and the profile's line.
"use strict";

const api = (path) => `/api/v1/${path}`;
const $ = (id) => document.getElementById(id);
const dpr = () => window.devicePixelRatio || 1;
const COLOURS = { red: "#e5484d", green: "#30a46c", blue: "#3e63dd", grey: "#8b8d98" };

function say(text, bad = false) {
  $("message").textContent = text;
  $("message").className = bad ? "bad" : "muted";
}

async function read(path) {
  const answer = await fetch(api(path));
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

async function send(method, path, input) {
  const answer = await fetch(api(path), {
    method,
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  if (!answer.ok) say(`${path}: ${answer.status} ${await answer.text()}`, true);
}

// A message (ADR-0014, ADR-0035): the header, the JPEG, then blocks.
function readMessage(buffer) {
  const v = new DataView(buffer);
  const u32 = (at) => v.getUint32(at, true);
  const length = u32(8);
  const header = {
    sequence: u32(12),
    exposureEnd: Number(v.getBigUint64(32, true)),
    ready: Number(v.getBigUint64(40, true)),
    roi: { x: u32(48), y: u32(52), width: u32(56), height: u32(60) },
    width: u32(64),
    height: u32(68),
    quality: v.getUint8(7),
  };
  const jpegLength = length >= 76 ? u32(72) : buffer.byteLength - length;
  const message = { header, jpeg: new Uint8Array(buffer, length, jpegLength), stats: null, mask: null };
  for (let at = length + jpegLength; at + 8 <= buffer.byteLength; ) {
    const [kind, size] = [v.getUint8(at), u32(at + 4)];
    const bytes = new Uint8Array(buffer, at + 8, size);
    if (kind === 2) message.stats = JSON.parse(new TextDecoder().decode(bytes));
    if (kind === 3) message.mask = { width: u32(at + 8), height: u32(at + 12), bits: bytes.subarray(8) };
    at += 8 + size;
  }
  return message;
}

// WebGL: the image, a 256-entry lookup table, and the mask, on one quad.
const canvas = $("view");
const gl = canvas.getContext("webgl");
function compile(type, source) {
  const shader = gl.createShader(type);
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  return shader;
}
const shaders = gl.createProgram();
gl.attachShader(shaders, compile(gl.VERTEX_SHADER, `
  attribute vec2 corner;
  uniform vec4 place;
  varying vec2 uv;
  void main() {
    uv = vec2(corner.x, 1.0 - corner.y);
    gl_Position = vec4(place.xy + corner * place.zw, 0.0, 1.0);
  }`));
gl.attachShader(shaders, compile(gl.FRAGMENT_SHADER, `
  precision mediump float;
  uniform sampler2D image, lut, mask;
  uniform bool falseColour, showMask;
  varying vec2 uv;
  void main() {
    vec3 c = texture2D(image, uv).rgb;
    if (falseColour) c = texture2D(lut, vec2(dot(c, vec3(0.2126, 0.7152, 0.0722)), 0.5)).rgb;
    if (showMask && texture2D(mask, uv).r > 0.5) c = vec3(1.0, 0.1, 0.6);
    gl_FragColor = vec4(c, 1.0);
  }`));
gl.linkProgram(shaders);
gl.useProgram(shaders);
gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
const corner = gl.getAttribLocation(shaders, "corner");
gl.enableVertexAttribArray(corner);
gl.vertexAttribPointer(corner, 2, gl.FLOAT, false, 0, 0);
gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
const uniform = (name) => gl.getUniformLocation(shaders, name);

function texture(unit, name) {
  gl.activeTexture(gl.TEXTURE0 + unit);
  gl.bindTexture(gl.TEXTURE_2D, gl.createTexture());
  const filter = name === "image" ? gl.LINEAR : gl.NEAREST;
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, filter);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, filter);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
  gl.uniform1i(uniform(name), unit);
}
texture(0, "image");
texture(2, "mask");
gl.texImage2D(gl.TEXTURE_2D, 0, gl.LUMINANCE, 1, 1, 0, gl.LUMINANCE, gl.UNSIGNED_BYTE, new Uint8Array([0]));
// False colour: black through violet, red and orange to pale yellow.
texture(1, "lut");
const stops = [[0, 0, 0, 0], [0.25, 80, 0, 120], [0.5, 220, 30, 40], [0.75, 255, 160, 0], [1, 255, 255, 230]];
const lut = new Uint8Array(256 * 3);
for (let i = 0; i < 256; i++) {
  const t = i / 255;
  const k = stops.findIndex((s) => s[0] >= t);
  const [a, b] = [stops[Math.max(k - 1, 0)], stops[k]];
  const f = b[0] === a[0] ? 0 : (t - a[0]) / (b[0] - a[0]);
  for (let c = 0; c < 3; c++) lut[3 * i + c] = a[c + 1] + f * (b[c + 1] - a[c + 1]);
}
gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB, 256, 1, 0, gl.RGB, gl.UNSIGNED_BYTE, lut);

// The mask's bits as one byte a pixel, or nothing.
function uploadMask(mask) {
  gl.activeTexture(gl.TEXTURE2);
  if (!mask) {
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.LUMINANCE, 1, 1, 0, gl.LUMINANCE, gl.UNSIGNED_BYTE, new Uint8Array([0]));
    return;
  }
  const { width, height, bits } = mask;
  const stride = Math.ceil(width / 8);
  const bytes = new Uint8Array(width * height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (bits[y * stride + (x >> 3)] & (0x80 >> (x & 7))) bytes[y * width + x] = 255;
    }
  }
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.LUMINANCE, width, height, 0, gl.LUMINANCE, gl.UNSIGNED_BYTE, bytes);
}

// What was drawn last: its header, and where, in canvas pixels.
let shown = null;
const viewport = () => [canvas.clientWidth, canvas.clientHeight].map((n) => Math.round(n * dpr()));

function draw(bitmap, header) {
  const [w, h] = viewport();
  for (const c of [canvas, $("marks")]) if (c.width !== w || c.height !== h) [c.width, c.height] = [w, h];
  gl.viewport(0, 0, w, h);
  gl.clearColor(0, 0, 0, 1);
  gl.clear(gl.COLOR_BUFFER_BIT);
  gl.activeTexture(gl.TEXTURE0);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB, gl.RGB, gl.UNSIGNED_BYTE, bitmap);
  const r = header.roi;
  const scale = Math.min(w / r.width, h / r.height);
  const [qw, qh] = [(2 * r.width * scale) / w, (2 * r.height * scale) / h];
  gl.uniform4f(uniform("place"), -qw / 2, -qh / 2, qw, qh);
  gl.uniform1i(uniform("falseColour"), $("false-colour").checked);
  gl.uniform1i(uniform("showMask"), $("saturation").checked);
  gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  shown = { header, scale, left: (w - r.width * scale) / 2, top: (h - r.height * scale) / 2 };
  drawMarks();
}

// Between the canvas's pixels and the sensor's.
const toSensor = (cx, cy) => [shown.header.roi.x + (cx - shown.left) / shown.scale, shown.header.roi.y + (cy - shown.top) / shown.scale];
const toCanvas = (sx, sy) => [shown.left + (sx - shown.header.roi.x) * shown.scale, shown.top + (sy - shown.header.roi.y) * shown.scale];

// The client's marks, in sensor pixels.
let statsRoi = null;
let line = null;
let dragging = null;

function drawMarks() {
  const marks = $("marks");
  const g = marks.getContext("2d");
  g.clearRect(0, 0, marks.width, marks.height);
  if (!shown) return;
  g.lineWidth = 1.5 * dpr();
  if ($("crosshair").checked) {
    const [cx, cy] = [shown.left + (shown.header.roi.width * shown.scale) / 2, shown.top + (shown.header.roi.height * shown.scale) / 2];
    g.strokeStyle = "rgba(255, 255, 255, 0.7)";
    g.beginPath();
    g.moveTo(cx - 20 * dpr(), cy); g.lineTo(cx + 20 * dpr(), cy);
    g.moveTo(cx, cy - 20 * dpr()); g.lineTo(cx, cy + 20 * dpr());
    g.stroke();
  }
  const rect = dragging?.rect ?? statsRoi;
  if (rect) {
    const [x, y] = toCanvas(rect.x, rect.y);
    g.strokeStyle = "#ffd60a";
    g.strokeRect(x, y, rect.width * shown.scale, rect.height * shown.scale);
  }
  const l = dragging?.line ?? line;
  if (l) {
    g.strokeStyle = "#00e5ff";
    g.beginPath();
    g.moveTo(...toCanvas(...l.from));
    g.lineTo(...toCanvas(...l.to));
    g.stroke();
  }
}

const marks = $("marks");
const at = (event) => toSensor(event.offsetX * dpr(), event.offsetY * dpr());
marks.addEventListener("pointerdown", (event) => {
  if (!shown) return;
  marks.setPointerCapture(event.pointerId);
  dragging = { from: at(event), profile: event.shiftKey };
});
marks.addEventListener("pointermove", (event) => {
  if (!dragging) return;
  const [a, b] = [dragging.from, at(event)];
  if (dragging.profile) {
    dragging.line = { from: a, to: b };
  } else {
    const [x0, y0] = [Math.min(a[0], b[0]), Math.min(a[1], b[1])].map((v) => Math.max(0, Math.round(v)));
    dragging.rect = { x: x0, y: y0, width: Math.round(Math.abs(b[0] - a[0])), height: Math.round(Math.abs(b[1] - a[1])) };
  }
  drawMarks();
});
marks.addEventListener("pointerup", () => {
  if (dragging?.line) line = dragging.line;
  if (dragging?.rect && dragging.rect.width > 1 && dragging.rect.height > 1) statsRoi = dragging.rect;
  dragging = null;
  ask();
  drawMarks();
});
$("clear").addEventListener("click", () => { statsRoi = null; line = null; ask(); drawMarks(); });

// Scrolling zooms about the pointer: a region of the sensor, kept inside it.
let sensor = null;
let roi = null;
marks.addEventListener("wheel", (event) => {
  event.preventDefault();
  if (!shown || !sensor) return;
  const r = shown.header.roi;
  const [sx, sy] = at(event);
  const width = Math.min(sensor.width, Math.max(48, r.width * (event.deltaY > 0 ? 1.25 : 0.8)));
  const height = Math.min(sensor.height, (width * r.height) / r.width);
  const clamp = (v, most) => Math.round(Math.min(Math.max(v, 0), most));
  roi = width >= sensor.width ? null : {
    x: clamp(sx - ((sx - r.x) * width) / r.width, sensor.width - width),
    y: clamp(sy - ((sy - r.y) * height) / r.height, sensor.height - height),
    width: Math.round(width),
    height: Math.round(height),
  };
  ask();
}, { passive: false });
$("whole").addEventListener("click", () => { roi = null; ask(); });

// What the page asks for, all of it in each message.
let socket = null;
let ready = 0;

function ask() {
  if (socket?.readyState !== WebSocket.OPEN) return;
  const [width, height] = viewport();
  const stats = { bins: 256, mask: true };
  if (statsRoi) stats.roi = statsRoi;
  if (line) stats.profile = line;
  const params = { width, height, ack: ready, stats };
  if (roi) params.roi = roi;
  socket.send(JSON.stringify(params));
}
for (const id of ["crosshair", "false-colour", "saturation"]) $(id).addEventListener("change", drawMarks);
window.addEventListener("resize", ask);

// Charts: a line for each channel.
function chart(id, series, colours, { log = false, top = null } = {}) {
  const c = $(id);
  const [w, h] = [Math.round(c.clientWidth * dpr()), Math.round(c.clientHeight * dpr())];
  [c.width, c.height] = [w, h];
  const g = c.getContext("2d");
  g.clearRect(0, 0, w, h);
  const scale = (v) => (log ? Math.log10(1 + v) : v);
  const most = top ?? Math.max(1, ...series.flat().filter((v) => v !== null).map(scale));
  series.forEach((values, s) => {
    g.strokeStyle = colours[s];
    g.lineWidth = dpr();
    g.beginPath();
    let pen = false;
    values.forEach((v, i) => {
      if (v === null) { pen = false; return; }
      const [x, y] = [(i / Math.max(1, values.length - 1)) * w, h - (scale(v) / most) * (h - 4) - 2];
      if (pen) g.lineTo(x, y); else g.moveTo(x, y);
      pen = true;
    });
    g.stroke();
  });
}

function show(stats) {
  const colours = stats.channels.map((c) => COLOURS[c]);
  const fixed = (values, digits = 0) => values.map((v) => v.toFixed(digits).padStart(7)).join(" ");
  chart("histogram", stats.histogram, colours, { log: $("log").checked });
  $("levels").textContent =
    `channels       ${stats.channels.map((c) => c.padStart(7)).join(" ")}\n` +
    `1 %           ${fixed(stats.percentiles.map((p) => p[1]))}\n` +
    `median        ${fixed(stats.percentiles.map((p) => p[2]))}\n` +
    `99 %          ${fixed(stats.percentiles.map((p) => p[3]))}\n` +
    `saturated %   ${fixed(stats.saturated.map((s) => 100 * s), 2)}   of ${stats.full_scale}`;
  const r = stats.roi;
  $("roi").textContent = r
    ? `${r.rect.width} × ${r.rect.height} at (${r.rect.x}, ${r.rect.y})\n` +
      `mean          ${fixed(r.mean, 1)}\nstd           ${fixed(r.std, 1)}\n` +
      `min           ${fixed(r.min)}\nmax           ${fixed(r.max)}`
    : "Drag on the image to draw one.";
  if (stats.profile) {
    const p = stats.profile.samples;
    chart("profile", stats.channels.map((_, c) => p.map((s) => (s ? s[c] : null))), colours, { top: stats.full_scale });
  } else {
    chart("profile", [], []);
  }
  $("focus").textContent = `focus (normalised variance of green)  ${stats.focus.toFixed(2)}`;
}

// The last second's frames, for the status line.
let second = [];

function connect() {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  socket = new WebSocket(`${scheme}://${location.host}${api("preview/stream")}`);
  socket.binaryType = "arraybuffer";
  socket.onopen = () => { ready = 0; ask(); };
  socket.onclose = () => say("The stream closed: reload the page to reconnect.", true);
  socket.onmessage = async (event) => {
    if (typeof event.data === "string") return say(event.data, true);
    const message = readMessage(event.data);
    const { header } = message;
    const bitmap = await createImageBitmap(new Blob([message.jpeg], { type: "image/jpeg" }));
    if (header.sequence >= ready) {
      if (message.stats) {
        uploadMask(message.mask);
        show(message.stats);
      }
      draw(bitmap, header);
      ready = header.sequence + 1;
      ask();
      const now = performance.now();
      second = second.filter((t) => now - t <= 1000).concat(now);
      $("status").textContent =
        `${second.length} fps   ${header.width} × ${header.height} of ${header.roi.width} × ${header.roi.height}   ` +
        `q ${header.quality}   ${((header.ready - header.exposureEnd) / 1e6).toFixed(0)} ms in the server`;
    }
    bitmap.close();
  };
}

// The light, the exposure, and the stage's jogs.
$("light").addEventListener("change", (event) =>
  send("POST", "illumination/set_light", { channel: "transmitted", on: true, intensity: Number(event.target.value) }));
$("exposure").addEventListener("change", (event) => send("PUT", "camera/exposure", Number(event.target.value)));
for (const button of document.querySelectorAll("button[data-axis]")) {
  const { axis, sign } = button.dataset;
  button.addEventListener("pointerdown", () =>
    send("POST", "stage/jog", { axis, towards_max: sign === "+", speed: Number($("speed").value) }));
  for (const end of ["pointerup", "pointerleave"]) button.addEventListener(end, () => send("POST", "stage/halt", {}));
}

async function start() {
  const capabilities = await read("camera/capabilities");
  sensor = { width: capabilities.sensor_width, height: capabilities.sensor_height };
  connect();
}

start().catch((error) => say(error.message, true));
