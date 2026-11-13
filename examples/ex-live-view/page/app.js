// The live view: the preview Thing's stream, decoded with createImageBitmap
// and drawn with WebGL. Scrolling asks the server for a region of the
// sensor; each frame drawn is acknowledged, so the server never sends far
// ahead of what is shown (ADR-0014).
"use strict";

const api = (path) => `/api/v1/${path}`;
const $ = (id) => document.getElementById(id);
const canvas = $("view");

function say(text, bad = false) {
  $("message").textContent = text;
  $("message").className = bad ? "bad" : "muted";
}

async function read(path) {
  const answer = await fetch(api(path));
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

async function post(action, input = {}) {
  const answer = await fetch(api(action), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  if (answer.status !== 201) say(`${action}: ${answer.status} ${await answer.text()}`, true);
}

// ADR-0014's header: little-endian; the payload starts at its length.
function readHeader(buffer) {
  const v = new DataView(buffer);
  const magic = String.fromCharCode(...new Uint8Array(buffer, 0, 4));
  if (magic !== "OSPV" || v.getUint8(4) !== 1) throw new Error("not a version 1 preview");
  const u32 = (at) => v.getUint32(at, true);
  const ns = (at) => Number(v.getBigUint64(at, true));
  return {
    quality: v.getUint8(7),
    length: u32(8),
    sequence: u32(12),
    frame: ns(16),
    exposureEnd: ns(32),
    ready: ns(40),
    roi: { x: u32(48), y: u32(52), width: u32(56), height: u32(60) },
    width: u32(64),
    height: u32(68),
  };
}

// WebGL: one textured quad, placed to fit the canvas.
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
  uniform sampler2D image;
  varying vec2 uv;
  void main() { gl_FragColor = texture2D(image, uv); }`));
gl.linkProgram(shaders);
gl.useProgram(shaders);
gl.bindBuffer(gl.ARRAY_BUFFER, gl.createBuffer());
gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([0, 0, 1, 0, 0, 1, 1, 1]), gl.STATIC_DRAW);
const corner = gl.getAttribLocation(shaders, "corner");
gl.enableVertexAttribArray(corner);
gl.vertexAttribPointer(corner, 2, gl.FLOAT, false, 0, 0);
gl.bindTexture(gl.TEXTURE_2D, gl.createTexture());
for (const name of ["TEXTURE_MIN_FILTER", "TEXTURE_MAG_FILTER"]) gl.texParameteri(gl.TEXTURE_2D, gl[name], gl.LINEAR);
for (const name of ["TEXTURE_WRAP_S", "TEXTURE_WRAP_T"]) gl.texParameteri(gl.TEXTURE_2D, gl[name], gl.CLAMP_TO_EDGE);
const place = gl.getUniformLocation(shaders, "place");

// What was drawn last: its header, and where, in canvas pixels.
let shown = null;

function draw(bitmap, header) {
  const [w, h] = viewport();
  if (canvas.width !== w || canvas.height !== h) [canvas.width, canvas.height] = [w, h];
  gl.viewport(0, 0, w, h);
  gl.clearColor(0, 0, 0, 1);
  gl.clear(gl.COLOR_BUFFER_BIT);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB, gl.RGB, gl.UNSIGNED_BYTE, bitmap);
  const r = header.roi;
  const scale = Math.min(w / r.width, h / r.height);
  const [qw, qh] = [(2 * r.width * scale) / w, (2 * r.height * scale) / h];
  gl.uniform4f(place, -qw / 2, -qh / 2, qw, qh);
  gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
  shown = { header, scale, left: (w - r.width * scale) / 2, top: (h - r.height * scale) / 2 };
}

const viewport = () => [canvas.clientWidth, canvas.clientHeight].map((n) => Math.round(n * devicePixelRatio));

// What the page asks for: the viewport, the region, the quality, and the
// next message it's ready for. Each message gives them all.
let sensor = null;
let roi = null;
let ready = 0;
let socket = null;

function ask() {
  if (socket?.readyState !== WebSocket.OPEN) return;
  const [width, height] = viewport();
  const params = { width, height, quality: Number($("quality").value) };
  if (roi) params.roi = roi;
  params.ack = ready;
  socket.send(JSON.stringify(params));
}

// Scrolling zooms about the pointer: a region of the sensor, kept inside it.
canvas.addEventListener("wheel", (event) => {
  event.preventDefault();
  if (!shown || !sensor) return;
  const r = shown.header.roi;
  const at = (offset, edge, origin) => origin + (offset * devicePixelRatio - edge) / shown.scale;
  const [sx, sy] = [at(event.offsetX, shown.left, r.x), at(event.offsetY, shown.top, r.y)];
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
$("quality").addEventListener("change", ask);
window.addEventListener("resize", ask);

// The last second's frames: when each arrived and was drawn, the server's
// latency (from the end of the exposure to the message being ready), and
// bytes.
let second = [];

function count(header, arrived, bytes) {
  const now = performance.now();
  second.push({ now, local: now - arrived, server: (header.ready - header.exposureEnd) / 1e6, bytes });
  second = second.filter((f) => now - f.now <= 1000);
  const mean = (key) => second.reduce((sum, f) => sum + f[key], 0) / second.length;
  $("stats").textContent =
    `${second.length} fps   ${header.width} × ${header.height} of ${header.roi.width} × ${header.roi.height} at (${header.roi.x}, ${header.roi.y})   ` +
    `q ${header.quality}   latency ${mean("server").toFixed(0)} ms in the server + ${mean("local").toFixed(0)} ms here   ` +
    `${(mean("bytes") * second.length / 1024).toFixed(0)} KB/s`;
}

function connect() {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  socket = new WebSocket(`${scheme}://${location.host}${api("preview/stream")}`);
  socket.binaryType = "arraybuffer";
  socket.onopen = () => { ready = 0; ask(); };
  socket.onclose = () => say("The stream closed: reload the page to reconnect.", true);
  socket.onmessage = async (event) => {
    if (typeof event.data === "string") return say(event.data, true);
    const arrived = performance.now();
    const header = readHeader(event.data);
    const jpeg = new Blob([new Uint8Array(event.data, header.length)], { type: "image/jpeg" });
    const bitmap = await createImageBitmap(jpeg);
    if (header.sequence >= ready) {
      draw(bitmap, header);
      ready = header.sequence + 1;
      ask();
      count(header, arrived, event.data.byteLength);
    }
    bitmap.close();
  };
}

// Jogs: held down, the stage jogs; let go, it halts.
for (const button of document.querySelectorAll("button[data-axis]")) {
  const { axis, sign } = button.dataset;
  button.addEventListener("pointerdown", () =>
    post("stage/jog", { axis, towards_max: sign === "+", speed: Number($("speed").value) }));
  for (const end of ["pointerup", "pointerleave"]) button.addEventListener(end, () => post("stage/halt"));
}

let pixel = null;

function expect() {
  if (!pixel) return;
  const rate = Number($("speed").value) / pixel;
  $("expected").textContent =
    `At ${$("speed").value} µm/s the image should slide ${rate.toFixed(0)} sensor pixels a second: ` +
    `left while jogging x +, up while jogging y + (the camera's mounting is neither turned nor mirrored).`;
}
$("speed").addEventListener("input", expect);

async function refresh() {
  try {
    const p = (await read("stage/position")).position;
    $("position").textContent = `stage at x ${p.x.toFixed(1)}   y ${p.y.toFixed(1)}   z ${p.z.toFixed(1)} µm`;
  } catch (error) {
    $("position").textContent = error.message;
  }
}

async function start() {
  const capabilities = await read("camera/capabilities");
  sensor = { width: capabilities.sensor_width, height: capabilities.sensor_height };
  pixel = await read("objective/pixel_at_sample");
  expect();
  connect();
  refresh();
  setInterval(refresh, 250);
}

start().catch((error) => say(error.message, true));
