// The job service's page: the queue, live from the `jobs` Thing's
// `changed` events, re-read now and then since events are hints (ADR-0016);
// the history from `jobs/list`; and buttons for what clients may ask.
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
    await new Promise((done) => setTimeout(done, 50));
    invocation = await (await fetch(invocation.href)).json();
  }
  if (invocation.status === "error") say(`${action}: ${invocation.error?.detail ?? JSON.stringify(invocation.error)}`, true);
  else say(`${action}: done`);
  return invocation.output;
}

// The jobs not yet ended, by ID.
const live = new Map();

function cell(text, className) {
  const td = document.createElement("td");
  td.textContent = text;
  if (className) td.className = className;
  return td;
}

function title(job) {
  return `${job.title} · ${job.kind} · ${job.id.slice(-8)}`;
}

function button(label, action, job, extra = {}) {
  const b = document.createElement("button");
  b.textContent = label;
  b.addEventListener("click", () => invoke(`jobs/${action}`, { id: job.id, ...extra }));
  return b;
}

function render() {
  const jobs = [...live.values()].sort((a, b) =>
    (a.position ?? -1) - (b.position ?? -1) || a.submitted.localeCompare(b.submitted));
  $("queue").replaceChildren(...jobs.map((job) => {
    const tr = document.createElement("tr");
    const where = job.position === null ? "" : `, #${job.position + 1} in the ${job.resource} queue`;
    tr.append(cell(title(job)), cell(`${job.state}${where}`));
    const progress = document.createElement("td");
    const bar = document.createElement("div");
    bar.className = "bar";
    const fill = document.createElement("div");
    fill.style.width = `${Math.round(100 * (job.progress.fraction ?? 0))}%`;
    bar.append(fill);
    const eta = job.progress.eta_s === null ? "" : `, ${job.progress.eta_s.toFixed(1)} s to go`;
    const note = document.createElement("div");
    note.className = "muted";
    note.textContent = `${job.progress.stage}${eta}`;
    progress.append(bar, note);
    const actions = document.createElement("td");
    if (job.state === "running" && job.pausable) actions.append(button("Pause", "pause", job));
    if (job.state === "paused") actions.append(button("Resume", "resume", job));
    if (job.position !== null && job.resource === "hardware" && job.position > 0) {
      actions.append(button("First", "reorder", job, { position: 0 }));
    }
    actions.append(button("Cancel", "cancel", job));
    tr.append(progress, actions);
    return tr;
  }));
}

async function history() {
  const jobs = await read("jobs/list?limit=15");
  $("history").replaceChildren(...jobs.filter((job) => job.finished).map((job) => {
    const tr = document.createElement("tr");
    const ended = new Date(job.finished).toLocaleTimeString();
    tr.append(cell(title(job)), cell(job.error ? `${job.state}: ${job.error}` : job.state, job.state === "failed" ? "bad" : ""),
      cell(ended), cell(job.results.join(", ")));
    return tr;
  }));
}

function update(job) {
  if (["completed", "failed", "cancelled", "interrupted"].includes(job.state)) {
    live.delete(job.id);
    history().catch((error) => say(error.message, true));
  } else {
    live.set(job.id, job);
  }
  render();
}

// Events are hints: the queue is read again every few seconds too.
async function refresh() {
  try {
    live.clear();
    for (const job of await read("jobs/queue")) live.set(job.id, job);
    render();
    const holder = await read("jobs/hardware");
    $("hardware").textContent = holder ? `The hardware lock: held by job ${holder.slice(-8)}` : "The hardware lock: free";
  } catch (error) {
    say(error.message, true);
  }
}

$("tour").addEventListener("click", () => invoke("jobs/submit", {
  kind: "demo.tour",
  params: { points: Number($("points").value), step_um: Number($("step").value) },
}));
$("crunch").addEventListener("click", () => invoke("jobs/submit", {
  kind: "demo.crunch",
  params: { seconds: Number($("seconds").value), gpu_mib: Number($("gpu").value) },
}));
$("jog").addEventListener("pointerdown", () => invoke("stage/jog", { axis: "x", towards_max: true, speed: 500 }));
$("jog").addEventListener("pointerup", () => invoke("stage/halt"));

new EventSource(api("jobs/changed")).addEventListener("message", (event) => update(JSON.parse(event.data)));
refresh();
history().catch((error) => say(error.message, true));
setInterval(refresh, 3000);
