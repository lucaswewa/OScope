// ex-live-view's benchmark: opens the page in headless Chrome and records
// each step's times, the server's and the page's, over three windows of
// 30 s: from the start, while the camera's tiles are first rendered; with
// the stage still; and jogging along x.
//
//   cargo run --release -p ex-live-view            # in one terminal
//   node examples/ex-live-view/bench/bench.mjs --label cpu --out bench-cpu.json
//
// Playwright comes from web/'s `npm ci`. Options: --url, --label, --out,
// --start, --still and --jog (seconds), --speed (µm/s), --channel (chrome,
// msedge or chromium) and --headed.

import { writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, "../../../web/package.json"));
const { chromium } = require("playwright");

const options = {
  url: "http://127.0.0.1:8218/",
  label: "run",
  out: null,
  start: 30,
  still: 30,
  jog: 30,
  speed: 500,
  channel: "chrome",
  headed: false,
};
const args = process.argv.slice(2);
while (args.length) {
  const name = args.shift().replace(/^--/, "");
  if (!(name in options)) throw new Error(`--${name}: the options are ${Object.keys(options).join(", ")}`);
  options[name] = typeof options[name] === "boolean" ? true : args.shift();
}
for (const name of ["start", "still", "jog", "speed"]) options[name] = Number(options[name]);

const api = (route) => new URL(`/api/v1/${route}`, options.url);
const post = async (action, input = {}) => {
  const answer = await fetch(api(action), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  if (answer.status !== 201) throw new Error(`${action}: ${answer.status} ${await answer.text()}`);
};
const position = async () => (await (await fetch(api("stage/position"))).json()).position;
const pause = (seconds) => new Promise((done) => setTimeout(done, seconds * 1000));

// The steps of a window, as a short table.
function show(name, measured) {
  console.log(`\n${name}: ${measured.webgl}`);
  for (const [step, s] of Object.entries(measured.steps)) {
    const ms = (v) => v.toFixed(1).padStart(7);
    console.log(`  ${step.padEnd(28)} ${s.per_s.toFixed(1).padStart(5)}/s ${ms(s.mean_ms)} ${ms(s.p50_ms)} ${ms(s.p95_ms)} ${ms(s.max_ms)}`);
  }
}

const browser = await chromium.launch({
  channel: options.channel === "chromium" ? undefined : options.channel,
  headless: !options.headed,
  args: ["--ignore-gpu-blocklist", "--enable-gpu"],
});
try {
  const page = await browser.newPage({ viewport: { width: 1200, height: 1000 } });
  page.on("console", (message) => message.type() === "error" && console.error(`page: ${message.text()}`));
  await page.goto(options.url);
  await page.waitForFunction(() => document.getElementById("stats").textContent.includes("fps"), null, {
    timeout: 120_000,
  });
  const measure = () => page.evaluate(() => window.bench());
  const windows = {};
  // The server's file is up to a second old: each window waits a little
  // longer, so that it holds no more than a second of the window before.
  await pause(options.start);
  windows.start = await measure();
  show("start", windows.start);
  await pause(options.still + 1.5);
  windows.still = await measure();
  show("still", windows.still);
  const from = await position();
  await post("stage/jog", { axis: "x", towards_max: true, speed: options.speed });
  await pause(options.jog + 1.5);
  windows.jog = await measure();
  await post("stage/halt");
  windows.jog.stage = { from, to: await position(), speed: options.speed };
  show(`jog at ${options.speed} µm/s`, windows.jog);
  const report = {
    label: options.label,
    url: options.url,
    at: new Date().toISOString(),
    machine: {
      cpu: os.cpus()[0].model,
      threads: os.cpus().length,
      memory_gb: Math.round(os.totalmem() / 2 ** 30),
      os: `${os.type()} ${os.release()}`,
    },
    browser: `${options.channel} ${browser.version()}${options.headed ? "" : ", headless"}`,
    windows,
  };
  const out = options.out ?? `bench-${options.label}.json`;
  writeFileSync(out, `${JSON.stringify(report, null, 2)}\n`);
  console.log(`\nwrote ${out}`);
} finally {
  await browser.close();
}
