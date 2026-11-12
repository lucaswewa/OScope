// Reads the system Thing's Thing Description, then, every second, each
// property it lists, from the server that served this page.
"use strict";

const TD_URL = "/api/v1/system/";
const rows = new Map();

async function readJson(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url} answered ${response.status}`);
  }
  return response.json();
}

function gigabytes(bytes) {
  return `${(bytes / 1e9).toFixed(1)} GB`;
}

// A property's value as text: an object's fields one per line, sizes in GB,
// and a number with its unit.
function show(value, unit) {
  if (value !== null && typeof value === "object") {
    return Object.entries(value)
      .map(([key, v]) => `${key}: ${key.endsWith("_bytes") ? gigabytes(v) : v}`)
      .join("\n");
  }
  if (typeof value === "number" && unit) {
    return `${value.toFixed(1)} ${unit}`;
  }
  return String(value);
}

async function refresh() {
  const status = document.getElementById("status");
  try {
    for (const [name, row] of rows) {
      const value = await readJson(row.href);
      row.cell.textContent = show(value, row.unit);
      if (name === "version") {
        document.getElementById("version").textContent = value;
      }
    }
    status.textContent = `Read at ${new Date().toLocaleTimeString()}.`;
    status.className = "muted";
  } catch (error) {
    status.textContent = `The server didn't answer: ${error.message}`;
    status.className = "error";
  }
}

async function start() {
  const td = await readJson(TD_URL);
  document.getElementById("td").textContent = JSON.stringify(td, null, 2);
  const body = document.getElementById("properties");
  for (const [name, property] of Object.entries(td.properties)) {
    const row = body.insertRow();
    row.insertCell().textContent = name;
    const cell = row.insertCell();
    const description = row.insertCell();
    description.textContent = property.description ?? "";
    description.className = "muted";
    rows.set(name, { cell, href: property.forms[0].href, unit: property.unit });
  }
  await refresh();
  setInterval(refresh, 1000);
}

start().catch((error) => {
  const status = document.getElementById("status");
  status.textContent = `The Thing Description couldn't be read: ${error.message}`;
  status.className = "error";
});
