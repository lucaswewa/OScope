// The live tail of OScope's log, over the media plane's WebSocket; buttons
// that write a record at each level; and the daily files.
"use strict";

const LEVELS = ["trace", "debug", "info", "warn", "error", "critical"];
const MAX_ROWS = 500;
const tail = document.getElementById("tail");
const scroll = document.getElementById("scroll");
const form = document.getElementById("filters");
const pause = document.getElementById("pause");
let socket = null;

function setStatus(text, isError = false) {
  const status = document.getElementById("status");
  status.textContent = text;
  status.className = isError ? "error" : "muted";
}

// The filter, as the tail's query: the newest 200 records that match, then
// each new one.
function query() {
  const data = new FormData(form);
  const query = { level: data.get("level"), limit: 200 };
  for (const key of ["target", "text"]) {
    const value = data.get(key).trim();
    if (value) {
      query[key] = value;
    }
  }
  return query;
}

function follow() {
  if (socket && socket.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify(query()));
  }
}

function time(text) {
  const time = new Date(text);
  const millis = String(time.getMilliseconds()).padStart(3, "0");
  return `${time.toLocaleTimeString([], { hour12: false })}.${millis}`;
}

function addRow(record) {
  const atEnd = scroll.scrollTop + scroll.clientHeight >= scroll.scrollHeight - 4;
  const row = tail.insertRow();
  row.insertCell().textContent = time(record.time);
  const level = row.insertCell();
  level.textContent = record.level;
  level.className = `level ${record.level}`;
  row.insertCell().textContent = record.target;
  const message = row.insertCell();
  message.textContent = record.message;
  const extras = Object.entries(record.fields ?? {}).map(
    ([key, value]) => `${key}=${typeof value === "string" ? value : JSON.stringify(value)}`,
  );
  if (record.job) {
    extras.unshift(`job=${record.job}`);
  }
  if (extras.length > 0) {
    const fields = document.createElement("span");
    fields.className = "fields";
    fields.textContent = `  ${extras.join("  ")}`;
    message.append(fields);
  }
  while (tail.rows.length > MAX_ROWS) {
    tail.deleteRow(0);
  }
  if (atEnd) {
    scroll.scrollTop = scroll.scrollHeight;
  }
}

function addNote(text) {
  const cell = tail.insertRow().insertCell();
  cell.colSpan = 4;
  cell.className = "note";
  cell.textContent = text;
}

function connect() {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  socket = new WebSocket(`${scheme}://${location.host}/api/v1/media/logs`);
  socket.addEventListener("open", follow);
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.query) {
      // Everything from here on matches the filter.
      tail.replaceChildren();
    } else if (message.record) {
      if (!pause.checked) {
        addRow(message.record);
      }
    } else if ("caught_up" in message) {
      setStatus(`Live: ${message.caught_up} earlier records, then each new one.`);
      scroll.scrollTop = scroll.scrollHeight;
    } else if ("skipped" in message) {
      addNote(`${message.skipped} records skipped: the page fell behind.`);
    } else if (message.error) {
      setStatus(`The server refused the filter: ${message.error}`, true);
    }
  });
  socket.addEventListener("close", () => {
    setStatus("Disconnected. Trying again in 2 s.", true);
    setTimeout(connect, 2000);
  });
}

async function emit(level) {
  const note = document.getElementById("note").value.trim();
  const body = { level, message: note || `a ${level} note from the page` };
  const response = await fetch("/api/v1/logs/emit", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    setStatus(`Writing the record failed: ${response.status}`, true);
  }
}

async function loadFiles() {
  const response = await fetch("/api/v1/logs/files");
  if (!response.ok) {
    return;
  }
  const items = (await response.json()).map((file) => {
    const link = document.createElement("a");
    link.href = `/api/v1/logs/files/${encodeURIComponent(file.name)}`;
    link.textContent = file.name;
    const item = document.createElement("li");
    item.append(link, ` (${(file.bytes / 1024).toFixed(1)} KiB)`);
    return item;
  });
  document.getElementById("files").replaceChildren(...items);
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  follow();
});
// Resuming asks again, so that the table shows the newest records.
pause.addEventListener("change", () => pause.checked || follow());
for (const level of LEVELS) {
  const button = document.createElement("button");
  button.textContent = level;
  button.className = `level ${level}`;
  button.addEventListener("click", () => emit(level));
  document.getElementById("writer").append(button);
}
connect();
loadFiles();
setInterval(loadFiles, 10000);
