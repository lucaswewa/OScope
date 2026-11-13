// Live recipes: the preview Thing's stream drawn on a canvas, each frame
// acknowledged (ADR-0014); a choice of recipes, each set with
// `set_live_recipe`, which checks it against its budget; and buttons that
// capture references with `capture_reference`.
"use strict";

const api = (path) => `/api/v1/${path}`;
const $ = (id) => document.getElementById(id);
const canvas = $("view");
const context = canvas.getContext("2d");

function say(text, bad = false) {
  $("message").textContent = text;
  $("message").className = bad ? "bad" : "muted";
}

async function read(path) {
  const answer = await fetch(api(path));
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// Invokes an action and waits for it to end: its output, or its error thrown.
async function invoke(action, input = {}) {
  const started = await fetch(api(action), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  let invocation = await started.json();
  if (started.status !== 201) throw new Error(`${action}: ${started.status} ${JSON.stringify(invocation)}`);
  while (["pending", "running"].includes(invocation.status)) {
    await new Promise((done) => setTimeout(done, 50));
    invocation = await (await fetch(invocation.href)).json();
  }
  if (invocation.status === "error") throw new Error(invocation.error?.detail ?? JSON.stringify(invocation.error));
  return invocation.output;
}

// ADR-0014's header, little-endian: the fields the page uses, and where
// the JPEG is.
function readHeader(buffer) {
  const v = new DataView(buffer);
  const u32 = (at) => v.getUint32(at, true);
  const length = u32(8);
  return {
    sequence: u32(12),
    width: u32(64),
    height: u32(68),
    jpeg: [length, length >= 76 ? u32(72) : buffer.byteLength - length],
  };
}

// The stream: a viewport the canvas's size, each frame drawn, then
// acknowledged.
function connect() {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  const socket = new WebSocket(`${scheme}://${location.host}${api("preview/stream")}`);
  socket.binaryType = "arraybuffer";
  const params = () => ({ width: canvas.clientWidth, height: canvas.clientHeight, quality: 80 });
  socket.addEventListener("open", () => socket.send(JSON.stringify({ ...params(), ack: 0 })));
  socket.addEventListener("message", async (event) => {
    if (typeof event.data === "string") return;
    const header = readHeader(event.data);
    const [start, length] = header.jpeg;
    const bitmap = await createImageBitmap(new Blob([event.data.slice(start, start + length)], { type: "image/jpeg" }));
    canvas.width = header.width;
    canvas.height = header.height;
    context.drawImage(bitmap, 0, 0);
    socket.send(JSON.stringify({ ...params(), ack: header.sequence + 1 }));
  });
  socket.addEventListener("close", () => setTimeout(connect, 1000));
}

async function stats() {
  try {
    const [session] = await read("preview/sessions");
    if (!session) return;
    const recipe = session.recipe_ms === null ? "no recipe" : `the recipe ${session.recipe_ms.toFixed(1)} ms`;
    $("stats").textContent =
      `${session.fps.toFixed(0)} fps · ${session.latency_ms.toFixed(0)} ms from exposure · ${session.width} × ${session.height} · ${recipe}${session.gpu ? " · GPU" : ""}`;
  } catch (error) {
    $("stats").textContent = error.message;
  }
}

async function references() {
  const names = await read("preview/references");
  $("references").textContent = names.length ? `references: ${names.join(", ")}` : "no references yet";
}

// The recipes in recipes/index.json, each with the references it needs.
async function recipes() {
  const index = await (await fetch("recipes/index.json")).json();
  for (const { file, title, needs } of index) {
    const option = document.createElement("option");
    option.value = file;
    option.textContent = needs ? `${title} (needs ${needs.join(" and ")})` : title;
    $("recipe").append(option);
  }
}

$("recipe").addEventListener("change", async () => {
  const file = $("recipe").value;
  try {
    const recipe = file ? await (await fetch(`recipes/${file}`)).json() : null;
    const check = await invoke("preview/set_live_recipe", { recipe });
    $("check").textContent = check
      ? `checked: ${check.time_ms.toFixed(1)} ms on ${check.width} × ${check.height}, within ${check.budget_ms} ms`
      : "the preview's own look";
    say(`set: ${file || "none"}`);
  } catch (error) {
    $("check").textContent = "";
    say(error.message, true);
  }
});

$("budget").addEventListener("change", async () => {
  const answer = await fetch(api("preview/recipe_budget_ms"), {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(Number($("budget").value)),
  });
  say(answer.ok ? `budget: ${$("budget").value} ms, for the next recipe set` : `budget: ${answer.status}`, !answer.ok);
});

// Captures a reference, with `before` and `after` around it, then shows the
// references.
async function capture(name, before, after) {
  say(`capturing ${name}…`);
  try {
    const undo = await before();
    const reference = await invoke("preview/capture_reference", { name, frames: 8 });
    await after(undo);
    say(`captured ${name}: ${reference.width} × ${reference.height}, the mean of ${reference.frames} frames`);
  } catch (error) {
    say(error.message, true);
  }
  references();
}

$("flat").addEventListener("click", () =>
  capture("flat", () => invoke("simulation/unload"), (sample) => sample && invoke("simulation/load", { name: sample })));
const light = (on) => invoke("illumination/set_light", { channel: "transmitted", on, intensity: 0.6 });
$("dark").addEventListener("click", () => capture("dark", () => light(false), () => light(true)));

(async () => {
  // The camera may be streaming already.
  await invoke("camera/start_streaming").catch(() => {});
  $("budget").value = await read("preview/recipe_budget_ms");
  await recipes();
  references();
  connect();
  setInterval(stats, 1000);
})();
