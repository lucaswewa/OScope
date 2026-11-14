// The extensions' page: what is installed, what the loaded extensions
// provide, and buttons to install, enable, disable and uninstall, each for
// the next start, and to start the server again. A package's text is the
// uploader's, so it is shown as text, never as HTML.
"use strict";

const api = (path) => `/api/v1/${path}`;
const $ = (id) => document.getElementById(id);
const pause = (ms) => new Promise((done) => setTimeout(done, ms));

function say(text, bad = false) {
  $("message").textContent = text;
  $("message").className = bad ? "bad" : "muted";
}

async function read(path) {
  const answer = await fetch(api(path));
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// Invokes an action, and waits for its invocation to end: its output, or
// its error said.
async function invoke(action, input = {}) {
  const started = await fetch(api(action), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  let invocation = await started.json();
  if (started.status !== 201) return say(`${action}: ${started.status} ${JSON.stringify(invocation)}`, true);
  while (["pending", "running"].includes(invocation.status)) {
    await pause(50);
    invocation = await (await fetch(invocation.href)).json();
  }
  if (invocation.status === "error") say(`${action}: ${invocation.error?.detail ?? JSON.stringify(invocation.error)}`, true);
  else say(`${action}: done, for the next start`);
  return invocation.output;
}

function cell(...lines) {
  const td = document.createElement("td");
  lines.forEach((line, i) => {
    const div = document.createElement("div");
    div.textContent = line;
    if (i > 0) div.className = "muted";
    td.append(div);
  });
  return td;
}

// A row of one cell, saying `text`.
function note(text) {
  const tr = document.createElement("tr");
  tr.append(cell(text));
  return tr;
}

function button(label, action, id) {
  const b = document.createElement("button");
  b.textContent = label;
  b.addEventListener("click", async () => {
    await invoke(`extensions/${action}`, { id });
    await refresh();
  });
  return b;
}

// Where an extension stands: this start, and the next.
function state(extension) {
  const { loaded, next, enabled, uninstalled, problem } = extension;
  const now = loaded ? `loaded ${loaded}` : "not loaded";
  let then = "";
  if (uninstalled) then = "removed at the next start";
  else if (!enabled) then = "disabled";
  else if (problem) then = problem;
  else if (next !== loaded) then = `the next start loads ${next}`;
  return [now, then].filter(Boolean);
}

function row(extension) {
  const m = extension.manifest;
  const tr = document.createElement("tr");
  const provides = Object.entries(m.provides).map(([kind, names]) => `${kind}: ${names.join(", ")}`);
  const sha = cell(`${extension.sha256.slice(0, 16)}…`, `${m.kind}, ${m.license}`);
  sha.title = `SHA-256 ${extension.sha256}`;
  const actions = document.createElement("td");
  if (!extension.uninstalled) {
    actions.append(extension.enabled ? button("Disable", "disable", m.id) : button("Enable", "enable", m.id));
    actions.append(button("Uninstall", "uninstall", m.id));
  }
  tr.append(cell(`${m.name} ${m.version}`, m.id, m.description), cell(m.publisher), cell(...provides), sha, cell(...state(extension)), actions);
  return tr;
}

async function refresh() {
  try {
    const [host, installed, provided, restart] = await Promise.all(
      ["host", "installed", "provided", "restart_needed"].map((name) => read(`extensions/${name}`)));
    $("host").textContent = `This OScope offers the extension API ${host.api}, on ${host.platform ?? "a platform it doesn't support"}.`;
    $("restart").hidden = !restart;
    $("installed").replaceChildren(...(installed.length ? installed.map(row) : [note("Nothing is installed.")]));
    const rows = Object.entries(provided).flatMap(([kind, names]) =>
      Object.entries(names).map(([name, id]) => {
        const tr = document.createElement("tr");
        tr.append(cell(kind), cell(name), cell(id));
        return tr;
      }));
    $("provided").replaceChildren(...(rows.length ? rows : [note("Nothing: no extension is loaded.")]));
  } catch (error) {
    say(`Couldn't read the extensions: ${error.message}`, true);
  }
}

$("install").addEventListener("click", async () => {
  const file = $("file").files[0];
  if (!file) return say("Choose an .omext file first.", true);
  const answer = await fetch(api("extensions/install"), {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: file,
  });
  const body = await answer.json().catch(() => ({}));
  if (answer.ok) {
    const done = answer.status === 201 ? "Installed" : "Installed already:";
    say(`${done} ${body.manifest.id} ${body.manifest.version}, for the next start.`);
  } else {
    say(`Refused (${answer.status}): ${body.detail ?? answer.statusText}`, true);
  }
  await refresh();
});

// Asks the server to start again, then waits until the new start answers:
// it needs no restart then.
$("again").addEventListener("click", async () => {
  say("Starting again…");
  await fetch(api("system/restart"), { method: "POST", headers: { "Content-Type": "application/json" }, body: "{}" });
  for (let tries = 0; tries < 150; tries += 1) {
    await pause(200);
    try {
      if ((await read("extensions/restart_needed")) === false) break;
    } catch (_) {
      // Not answering yet.
    }
  }
  await refresh();
  say("Started again.");
});

// Changes from other clients, such as curl.
new EventSource(api("extensions/changed")).addEventListener("message", refresh);
refresh();
