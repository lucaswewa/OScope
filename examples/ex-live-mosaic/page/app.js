// The live view beside the live mosaic (P38; ADR-0026). The preview stream
// brings the frames, each with a block of the mosaic's tiles changed since
// the last (kind 4); OpenSeadragon shows the mosaic's tiles, fetching those
// changed again. Level 0 is the mosaic's finest; OpenSeadragon counts the
// other way.
"use strict";

const api = (path) => `/api/v1/${path}`;
const $ = (id) => document.getElementById(id);
const TILES = 4;
const VIEW = { width: 640, height: 480, mosaic: true };

function say(text, bad = false) {
  $("message").textContent = text;
  $("message").className = bad ? "bad" : "muted";
}

async function read(path) {
  const answer = await fetch(api(path));
  if (!answer.ok) throw new Error(`${path}: ${answer.status}`);
  return answer.json();
}

// Invokes `action`, and waits for its output.
async function invoke(action, input = {}) {
  const answer = await fetch(api(action), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  if (answer.status !== 201) throw new Error(`${action}: ${answer.status} ${await answer.text()}`);
  const { href } = await answer.json();
  for (;;) {
    const invocation = await (await fetch(href)).json();
    if (invocation.status === "completed") return invocation.output;
    if (invocation.status !== "pending" && invocation.status !== "running") {
      throw new Error(`${action}: ${JSON.stringify(invocation.error ?? invocation)}`);
    }
    await new Promise((done) => setTimeout(done, 100));
  }
}

// The mosaic: a tile source from its state, each tile's URL carrying its
// version, so that a tile changed is fetched again.
let viewer = null;
let shown = 0;
let epoch = 0;
const versions = new Map();

function open(state) {
  if (!state.canvas || state.mosaic === shown) return;
  shown = state.mosaic;
  versions.clear();
  const { width, height } = state.canvas;
  const top = Math.ceil(Math.log2(Math.max(width, height)));
  const source = {
    width,
    height,
    tileSize: state.tile,
    tileOverlap: 0,
    minLevel: top - (state.levels - 1),
    maxLevel: top,
    getTileUrl: (level, x, y) => {
      const key = `${top - level}/${x}/${y}`;
      return api(`mosaic/tiles/${key}?v=${epoch}.${versions.get(key) ?? 0}`);
    },
  };
  if (viewer) {
    viewer.open(source);
  } else {
    viewer = OpenSeadragon({
      element: $("mosaic"),
      tileSources: source,
      showNavigationControl: false,
      showNavigator: true,
      navigatorPosition: "BOTTOM_RIGHT",
      immediateRender: true,
    });
  }
}

let refreshing = false;
function changed(block) {
  if (block.mosaic !== shown) return refresh();
  if (block.all) epoch += 1;
  for (const [level, col, row] of block.tiles) {
    const key = `${level}/${col}/${row}`;
    versions.set(key, (versions.get(key) ?? 0) + 1);
  }
  // At most three times a second: OpenSeadragon fetches what it shows
  // again, the changed tiles under new URLs.
  if (refreshing) return;
  refreshing = true;
  setTimeout(() => {
    refreshing = false;
    viewer?.world.getItemAt(0)?.reset();
  }, 300);
}

async function refresh() {
  try {
    const state = await read("live_mosaic/state");
    open(state);
    const c = state.counts;
    const doing = state.drawing ? (state.roaming ? "roaming" : "scans only") : "halted";
    $("counts").textContent = state.mosaic
      ? `${doing}: ${c.drawn} drawn (${c.scans} a scan's), ${c.blurred} blurred, ${c.same} of a place drawn; ${state.tiles} tiles`
      : "no mosaic yet";
  } catch (error) {
    say(error.message, true);
  }
}

// The live view: each frame's JPEG, and its blocks after it (ADR-0035).
let url = null;
function stream() {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  const socket = new WebSocket(`${scheme}://${location.host}${api("preview/stream")}`);
  socket.binaryType = "arraybuffer";
  socket.onopen = () => socket.send(JSON.stringify({ ...VIEW, ack: 0 }));
  socket.onmessage = (event) => {
    const buffer = event.data;
    const v = new DataView(buffer);
    const start = v.getUint32(8, true);
    const length = v.getUint32(72, true);
    if (url) URL.revokeObjectURL(url);
    url = URL.createObjectURL(new Blob([new Uint8Array(buffer, start, length)], { type: "image/jpeg" }));
    $("live").src = url;
    for (let at = start + length; at + 8 <= buffer.byteLength; ) {
      const kind = v.getUint8(at);
      const size = v.getUint32(at + 4, true);
      if (kind === TILES) changed(JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, at + 8, size))));
      at += 8 + size;
    }
    socket.send(JSON.stringify({ ...VIEW, ack: v.getUint32(12, true) + 1 }));
  };
  socket.onclose = () => setTimeout(stream, 1000);
}

function act(id, work) {
  $(id).onclick = async () => {
    try {
      say("");
      await work();
      await refresh();
    } catch (error) {
      say(error.message, true);
    }
  };
}

act("begin", async () => {
  // 10 × 8 mm about where the stage is.
  const { position: p } = await read("stage/position");
  const region = { origin: { x: p.x - 5000, y: p.y - 4000 }, size: { width: 10000, height: 8000 } };
  await invoke("live_mosaic/begin", { request: { region } });
});
act("halt", () => invoke("live_mosaic/halt"));
act("clear", () => invoke("live_mosaic/clear"));
act("scan", async () => {
  const { position: p } = await read("stage/position");
  const project = await invoke("projects/create", { name: "Scanned from the page" });
  const region = { rect: { origin: { x: p.x - 1000, y: p.y - 1000 }, size: { width: 2000, height: 2000 } } };
  await invoke("scan/run", { request: { pattern: "snake", region }, project: project.id });
  say("scanning: its tiles are drawn as they are taken");
});

document.addEventListener("keydown", (event) => {
  const step = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[event.key];
  if (!step) return;
  event.preventDefault();
  const um = event.shiftKey ? 1000 : 250;
  invoke("stage/move_by", { dx: step[0] * um, dy: step[1] * um, dz: 0 }).catch((error) => say(error.message, true));
});

stream();
refresh();
setInterval(refresh, 1000);
