// The control page: reads the Things' properties, and invokes their actions
// as the README does with curl.
"use strict";

const api = (path) => `/api/v1/${path}`;
const $ = (id) => document.getElementById(id);

function say(text, bad = false) {
  $("message").textContent = text;
  $("message").className = bad ? "bad" : "muted";
}

async function read(path) {
  const answer = await fetch(api(path));
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// Invokes an action, and follows its invocation until it ends.
async function invoke(action, input = {}) {
  const started = await fetch(api(action), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  if (started.status !== 201) {
    say(`${action}: ${started.status} ${await started.text()}`, true);
    return null;
  }
  let invocation = await started.json();
  while (["pending", "running"].includes(invocation.status)) {
    await new Promise((done) => setTimeout(done, 100));
    invocation = await (await fetch(invocation.href)).json();
  }
  if (invocation.status === "error") {
    say(`${action}: ${JSON.stringify(invocation.error)}`, true);
  } else {
    say(`${action}: ${invocation.status}`);
  }
  return invocation;
}

async function refresh() {
  try {
    const sample = await read("stage/position");
    const p = sample.position;
    $("position").textContent = `x ${p.x.toFixed(1)}   y ${p.y.toFixed(1)}   z ${p.z.toFixed(1)} µm`;
    $("status").textContent = JSON.stringify(await read("stage/status"));
    const fitted = await read("objective/fitted");
    $("fitted").textContent = fitted ? `fitted: ${fitted.name}` : "none fitted";
    const loaded = await read("simulation/loaded");
    $("loaded").textContent = loaded ? `loaded: ${loaded}` : "the stage is empty";
  } catch (error) {
    $("status").textContent = error.message;
  }
}

// The objectives and samples to choose from. The catalogue lists its
// objectives as `objective`, the name its TOML gives each table.
async function lists() {
  for (const objective of (await read("objective/catalogue")).objective) {
    $("objective").add(new Option(objective.name, objective.name));
  }
  for (const sample of await read("simulation/samples")) {
    const truth = sample.ground_truth ? ", with ground truth" : "";
    $("sample").add(new Option(`${sample.name} (${sample.kind}${truth})`, sample.name));
  }
}

function start() {
  // Jogs: held down, the stage jogs; let go, it halts.
  for (const [axis, sign] of [["x", "−"], ["x", "+"], ["y", "−"], ["y", "+"], ["z", "−"], ["z", "+"]]) {
    const button = document.createElement("button");
    button.textContent = `${sign}${axis}`;
    button.addEventListener("pointerdown", () => invoke("stage/jog", { axis, towards_max: sign === "+", speed: 2000 }));
    for (const end of ["pointerup", "pointerleave"]) {
      button.addEventListener(end, () => invoke("stage/halt"));
    }
    $("jogs").append(button);
  }
  $("home").onclick = () => invoke("stage/home");
  $("halt").onclick = () => invoke("stage/halt");
  $("move").onclick = () => invoke("stage/move_to", { x: Number($("x").value), y: Number($("y").value), z: Number($("z").value) });
  $("fit").onclick = () => invoke("objective/fit", { name: $("objective").value });
  const light = () => invoke("illumination/set_light", { channel: "transmitted", on: $("light").checked, intensity: Number($("intensity").value) });
  $("light").onchange = light;
  $("intensity").onchange = light;
  $("load").onclick = () => invoke("simulation/load", { name: $("sample").value });
  $("unload").onclick = () => invoke("simulation/unload");
  refresh();
  setInterval(refresh, 250);
  lists().catch((error) => say(`The objectives or samples couldn't be listed: ${error.message}`, true));
}

start();
