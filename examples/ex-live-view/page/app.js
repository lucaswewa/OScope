// The live view, and its benchmark: the preview Thing's stream at 1000 × 667
// of the whole sensor, decoded with createImageBitmap and drawn with WebGL.
// Each frame drawn is acknowledged, so the server never sends far ahead of
// what is shown (ADR-0014). Each frame's steps here are timed, and the
// server's are read from /bench/timings.json each second.
"use strict";

// What the page always asks for: 1000 × 667 of the whole 5472 × 3648 sensor.
const SIZE = [1000, 667];

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

// ADR-0014's header: little-endian; the JPEG starts at its length, and
// runs for the length at 72 (ADR-0035).
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
    jpegLength: u32(8) >= 76 ? u32(72) : buffer.byteLength - u32(8),
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

function draw(bitmap, header) {
  const [w, h] = SIZE;
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
}

// What the page asks for: the size, the quality, and the next message it's
// ready for. Each message gives them all.
let ready = 0;
let socket = null;

function ask() {
  if (socket?.readyState !== WebSocket.OPEN) return;
  const [width, height] = SIZE;
  socket.send(JSON.stringify({ width, height, quality: Number($("quality").value), ack: ready }));
}
$("quality").addEventListener("change", ask);

// Each step's times over the last 30 s, as the server keeps its own.
const WINDOW_MS = 30_000;
const samples = new Map();

function note(step, ms, at = performance.now()) {
  if (!Number.isFinite(ms)) return;
  if (!samples.has(step)) samples.set(step, []);
  const list = samples.get(step);
  list.push([at, ms]);
  while (at - list[0][0] > WINDOW_MS) list.shift();
}

function summary(list, now = performance.now()) {
  const kept = list.filter(([at]) => now - at <= WINDOW_MS);
  if (!kept.length) return null;
  const ms = kept.map(([, v]) => v).sort((a, b) => a - b);
  const rank = (q) => ms[Math.round((ms.length - 1) * q)];
  const span = (kept[kept.length - 1][0] - kept[0][0]) / 1000;
  return {
    count: ms.length,
    per_s: span > 0 ? (ms.length - 1) / span : 0,
    mean_ms: ms.reduce((a, b) => a + b, 0) / ms.length,
    p50_ms: rank(0.5),
    p95_ms: rank(0.95),
    max_ms: ms[ms.length - 1],
  };
}

// The server's timings, read each second: its steps, and the offset from
// its camera's clock to the epoch, which puts the headers' times on ours.
let server = null;
const epoch = (now) => performance.timeOrigin + now;
const onOurs = (ns) => (server ? ns / 1e6 + server.clock_to_epoch_ms : NaN);

// The last second's frames, for the line under the image.
let second = [];
let lastArrived = null;

function count(header, arrived, bytes) {
  const now = performance.now();
  second.push({ now, local: now - arrived, server: (header.ready - header.exposureEnd) / 1e6, bytes });
  second = second.filter((f) => now - f.now <= 1000);
  const mean = (key) => second.reduce((sum, f) => sum + f[key], 0) / second.length;
  $("stats").textContent =
    `${second.length} fps   ${header.width} × ${header.height} of ${header.roi.width} × ${header.roi.height}   ` +
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
    const jpeg = new Blob([new Uint8Array(event.data, header.length, header.jpegLength)], { type: "image/jpeg" });
    const bitmap = await createImageBitmap(jpeg);
    const decoded = performance.now();
    if (header.sequence >= ready) {
      draw(bitmap, header);
      const drawn = performance.now();
      ready = header.sequence + 1;
      ask();
      count(header, arrived, event.data.byteLength);
      note("page.transit", epoch(arrived) - onOurs(header.ready), arrived);
      note("page.decode", decoded - arrived, arrived);
      note("page.draw", drawn - decoded, arrived);
      requestAnimationFrame(() => note("page.next_frame", performance.now() - drawn, arrived));
      note("page.end_to_end", epoch(drawn) - onOurs(header.exposureEnd), arrived);
      if (lastArrived !== null) note("page.interval", arrived - lastArrived, arrived);
      lastArrived = arrived;
    } else {
      note("page.stale", decoded - arrived, arrived);
    }
    bitmap.close();
  };
}

// The steps in the order a frame goes through them: the name, what it is,
// and whether it is part of the step above.
const ORDER = [
  ["camera.wake_late", "camera: wakes after the exposure ends"],
  ["camera.render", "camera: renders the frame"],
  ["camera.field", "the world's tiles, rendered when not cached", true],
  ["camera.motion", "the stage's places during each row", true],
  ["camera.expose", "each pixel through the optics and sensor (CPU)", true],
  ["camera.expose_gpu", "each pixel through the optics and sensor (GPU)", true],
  ["camera.pack", "camera: counts packed into the frame's buffer"],
  ["acquisition.position_wait", "acquisition: waits for the stage's position"],
  ["acquisition.publish", "acquisition: seals and publishes the frame"],
  ["stream.frame_age", "session: the frame's age when taken"],
  ["stream.ack_wait", "session: waits for the page's acknowledgement"],
  ["stream.spawn", "session: waits for a blocking thread"],
  ["preview.render", "preview: raw frame to JPEG"],
  ["preview.to_f32", "samples to floats (CPU)", true],
  ["preview.demosaic", "demosaic (CPU)", true],
  ["preview.bin", "bin (CPU)", true],
  ["preview.black", "black level (CPU)", true],
  ["preview.correct", "white balance (CPU)", true],
  ["preview.to_display", "display transform to 8 bits (CPU)", true],
  ["preview.gpu.to_f32", "upload, samples to floats (GPU)", true],
  ["preview.gpu.demosaic", "demosaic (GPU)", true],
  ["preview.gpu.bin", "bin (GPU)", true],
  ["preview.gpu.black", "black level (GPU)", true],
  ["preview.gpu.correct", "white balance (GPU)", true],
  ["preview.gpu.to_display", "display transform to 8 bits, download (GPU)", true],
  ["preview.resize", "resize to the size asked for", true],
  ["preview.jpeg", "JPEG", true],
  ["stream.message", "session: header and JPEG made one message"],
  ["stream.send", "session: message handed to the WebSocket"],
  ["stream.latency", "server latency: exposure end to message ready"],
  ["page.transit", "page: message ready to its arrival here"],
  ["page.decode", "page: JPEG decoded (createImageBitmap)"],
  ["page.draw", "page: texture upload and draw (WebGL)"],
  ["page.next_frame", "page: drawn to the next animation frame"],
  ["page.end_to_end", "end to end: exposure end to drawn here"],
  ["page.interval", "page: between frames drawn"],
];

// Everything measured: the server's steps, the page's, and the sessions.
function steps() {
  const page = Object.fromEntries([...samples].map(([step, list]) => [step, summary(list)]).filter(([, s]) => s));
  const theirs = Object.fromEntries((server?.steps ?? []).map((s) => [s.step, s]));
  return { ...theirs, ...page };
}

function table() {
  const all = steps();
  const ms = (v) => v.toFixed(1);
  const rows = ORDER.filter(([step]) => all[step]).map(([step, what, part]) => {
    const s = all[step];
    return `<tr${part ? ' class="part"' : ""}><td>${what}</td><td><code>${step}</code></td>` +
      `<td>${s.per_s.toFixed(1)}</td><td>${ms(s.mean_ms)}</td><td>${ms(s.p50_ms)}</td>` +
      `<td>${ms(s.p95_ms)}</td><td>${ms(s.max_ms)}</td></tr>`;
  });
  $("steps").tBodies[0].innerHTML = rows.join("");
  const tiles = server?.camera?.tiles;
  $("tiles").textContent = tiles
    ? `The camera's tiles: ${tiles.held} held, ${(tiles.bytes / 2 ** 20).toFixed(0)} MB; ` +
      `${tiles.hits} found and ${tiles.misses} rendered since it started. Frames published: ${server.frames.published}.`
    : "";
}

async function refreshTimings() {
  try {
    const answer = await fetch("/bench/timings.json", { cache: "no-store" });
    if (answer.ok) server = await answer.json();
  } catch {
    // The next second's read will do.
  }
  table();
}

// For a benchmark's script, and the Copy button: everything measured.
window.bench = async () => ({
  at: new Date().toISOString(),
  size: SIZE,
  userAgent: navigator.userAgent,
  webgl: (() => {
    const info = gl.getExtension("WEBGL_debug_renderer_info");
    return info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
  })(),
  steps: steps(),
  server,
  sessions: await read("preview/sessions"),
});
$("copy").addEventListener("click", async () => {
  await navigator.clipboard.writeText(JSON.stringify(await window.bench(), null, 2));
  say("Copied the timings as JSON.");
});

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
  pixel = await read("objective/pixel_at_sample");
  expect();
  await refreshTimings();
  connect();
  refresh();
  setInterval(refresh, 250);
  setInterval(refreshTimings, 1000);
}

start().catch((error) => say(error.message, true));
