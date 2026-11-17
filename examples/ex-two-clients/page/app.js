// P49's two clients: log in, watch the stage and who has control, ask for
// it, hand it over, and move the stage with it.
"use strict";

const $ = (id) => document.getElementById(id);
let token = null;

async function call(method, path, body) {
  const headers = { "content-type": "application/json" };
  if (token) headers.authorization = `Bearer ${token}`;
  const answer = await fetch(path, { method, headers, body: body && JSON.stringify(body) });
  if (answer.status === 401) throw new Error("logged out");
  return answer;
}

// Invokes `action`, and waits for it: a 403 says control is someone else's.
async function act(action, input = {}) {
  const started = await call("POST", `/api/v1/${action}`, input);
  if (started.status === 403) return say(": take control first");
  if (started.status !== 201) return say(`: ${action}: ${started.status}`);
  const { href } = await started.json();
  for (;;) {
    const invocation = await (await call("GET", href)).json();
    if (invocation.status === "completed") return say("");
    if (invocation.status !== "pending" && invocation.status !== "running") {
      return say(`: ${JSON.stringify(invocation.error ?? invocation)}`);
    }
    await new Promise((done) => setTimeout(done, 50));
  }
}

function say(text) {
  $("message").textContent = text;
}

async function watch() {
  if (!token) return;
  try {
    const [lease, position] = await Promise.all([
      call("GET", "/api/v1/auth/lease").then((a) => a.json()),
      call("GET", "/api/v1/stage/position").then((a) => a.json()),
    ]);
    $("holder").textContent = lease.holder ?? "no one";
    $("requests").textContent = lease.requests.length ? `, asked for by ${lease.requests.join(", ")}` : "";
    $("position").textContent = `x ${position.position.x.toFixed(1)} µm, y ${position.position.y.toFixed(1)} µm`;
  } catch (error) {
    say(`: ${error.message}`);
  }
  setTimeout(watch, 500);
}

$("login").addEventListener("submit", async (event) => {
  event.preventDefault();
  const body = { name: $("name").value, password: $("password").value };
  const answer = await call("POST", "/api/v1/auth/login", body);
  if (answer.status !== 200) return say(": no such name and password");
  ({ token } = await answer.json());
  $("me").textContent = body.name;
  const accounts = await (await call("GET", "/api/v1/auth/accounts")).json();
  $("to").replaceChildren(...accounts.filter((a) => a !== body.name).map((a) => new Option(a, a)));
  $("login").hidden = true;
  $("session").hidden = false;
  say("");
  watch();
});
$("acquire").onclick = () => act("auth/acquire_control");
$("request").onclick = () => act("auth/request_control");
$("release").onclick = () => act("auth/release_control");
$("home").onclick = () => act("stage/home");
$("hand").onclick = () => act("auth/hand_over_control", { to: $("to").value });
// Moves by the button's dx, kept in the stage's travel.
for (const button of document.querySelectorAll("[data-dx]")) {
  button.onclick = async () => {
    const { position } = await (await call("GET", "/api/v1/stage/position")).json();
    const keep = (v, most) => Math.min(Math.max(v, 0), most);
    const x = keep(position.x + Number(button.dataset.dx), 100000);
    act("stage/move_to", { x, y: keep(position.y, 75000), z: keep(position.z, 20000) });
  };
}
$("logout").onclick = async () => {
  await call("POST", "/api/v1/auth/logout");
  token = null;
  $("session").hidden = true;
  $("login").hidden = false;
};
