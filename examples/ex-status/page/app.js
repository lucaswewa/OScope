// P50's status indicator: the instrument's state and why, each
// subsystem's health, and faults to inject.
"use strict";

const $ = (id) => document.getElementById(id);

function show(status) {
  $("dot").className = `dot ${status.state}`;
  $("message").textContent = status.message;
  const rows = status.subsystems.map((health) => {
    const row = document.createElement("tr");
    const dot = document.createElement("span");
    dot.className = `dot ${health.state}`;
    for (const content of [dot, health.subsystem, health.state, health.message]) {
      const cell = document.createElement("td");
      cell.append(content);
      row.append(cell);
    }
    return row;
  });
  $("subsystems").replaceChildren(...rows);
}

// The status now, then each change as the `status` Thing tells it.
async function watch() {
  const answer = await fetch("/api/v1/status/instrument");
  if (answer.ok) show(await answer.json());
  const changes = new EventSource("/api/v1/status/changed");
  changes.onmessage = (event) => show(JSON.parse(event.data));
}

async function act(action, input) {
  await fetch(`/api/v1/${action}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
}

for (const box of document.querySelectorAll("[data-fault]")) {
  box.onchange = () => act("simulation/inject_fault", { fault: box.dataset.fault, active: box.checked });
}
$("home").onclick = () => act("stage/home", {});
watch();
