// P52's settings: your display preferences, saved as you change them; the
// configuration file's; and the document of them all, to import.
"use strict";

const $ = (id) => document.getElementById(id);
const at = (name) => `/api/v1/settings/${name}`;
const json = { "content-type": "application/json" };

async function read(name) {
  return (await fetch(at(name))).json();
}

async function show() {
  const display = await read("display");
  for (const key of ["theme", "units", "colormap"]) $(key).value = display[key];
  $("scale_bar").checked = display.scale_bar;
  const file = { system: await read("system"), restart_needed: await read("restart_needed") };
  $("file").textContent = JSON.stringify(file, null, 2);
  $("document").value = JSON.stringify(await read("document"), null, 2);
}

async function save() {
  const keys = ["theme", "units", "colormap"];
  const display = Object.fromEntries(keys.map((key) => [key, $(key).value]));
  display.scale_bar = $("scale_bar").checked;
  const body = JSON.stringify(display);
  const answer = await fetch(at("display"), { method: "PUT", headers: json, body });
  $("said").textContent = answer.ok ? "saved" : `refused: ${(await answer.json()).detail}`;
}

async function importIt() {
  let document;
  try {
    document = JSON.parse($("document").value);
  } catch (error) {
    $("said").textContent = `not JSON: ${error.message}`;
    return;
  }
  const body = JSON.stringify({ document });
  const started = await fetch(at("import"), { method: "POST", headers: json, body });
  const { href } = await started.json();
  for (;;) {
    const invocation = await (await fetch(href)).json();
    if (invocation.status !== "pending" && invocation.status !== "running") {
      const done = invocation.output && `imported: ${invocation.output.join(", ")}`;
      $("said").textContent = done || invocation.error?.detail || "refused";
      return show();
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

for (const id of ["theme", "units", "colormap", "scale_bar"]) $(id).onchange = save;
$("import").onclick = importIt;
show();
