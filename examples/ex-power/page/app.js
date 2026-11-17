// P51's power actions: each asks for a confirmation, and says what it
// would run, in a dry run, or why it was refused.
"use strict";

const $ = (id) => document.getElementById(id);

async function act(action) {
  if (!confirm(`${action}: are you sure?`)) return;
  const input = { confirm: true, force: $("force").checked };
  const started = await fetch(`/api/v1/power/${action}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(input),
  });
  const { href } = await started.json();
  for (;;) {
    const invocation = await (await fetch(href)).json();
    if (invocation.status === "completed") {
      const words = invocation.output.command;
      $("said").textContent = `${action}: ${words ? words.join(" ") : "the server itself"}`;
      return;
    }
    if (invocation.status !== "pending" && invocation.status !== "running") {
      $("said").textContent = `${action}: ${invocation.error?.detail ?? "refused"}`;
      return;
    }
    await new Promise((done) => setTimeout(done, 50));
  }
}

for (const button of document.querySelectorAll("[data-action]")) {
  button.onclick = () => act(button.dataset.action);
}
fetch("/api/v1/power/dry_run")
  .then((answer) => answer.json())
  .then((dry) => ($("dry").textContent = dry ? "(a dry run: nothing is done)" : ""));
