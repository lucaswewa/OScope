// Plots the example's results.json: the stage's reported x, y and z
// against simulated time, with its motions shaded and its trigger pulses
// marked, and a table of the motions' planned and measured durations.
"use strict";

function colour(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

function plot(canvas, results) {
  const ratio = window.devicePixelRatio || 1;
  const width = canvas.clientWidth;
  const height = canvas.clientHeight;
  canvas.width = width * ratio;
  canvas.height = height * ratio;
  const g = canvas.getContext("2d");
  g.scale(ratio, ratio);

  const samples = results.samples;
  const t0 = samples[0][0];
  const t1 = samples[samples.length - 1][0];
  let lo = Infinity;
  let hi = -Infinity;
  for (const s of samples) {
    for (const v of s.slice(1)) {
      lo = Math.min(lo, v);
      hi = Math.max(hi, v);
    }
  }
  const pad = { left: 44, right: 12, top: 12, bottom: 28 };
  const x = (t) => pad.left + ((t - t0) / (t1 - t0)) * (width - pad.left - pad.right);
  const y = (v) => height - pad.bottom - ((v - lo) / (hi - lo)) * (height - pad.top - pad.bottom);

  g.fillStyle = colour("--band");
  for (const m of results.motions.concat(results.jog ? [results.jog] : [])) {
    g.fillRect(x(m.start), pad.top, x(m.end) - x(m.start), height - pad.top - pad.bottom);
  }

  g.strokeStyle = colour("--line");
  g.fillStyle = colour("--muted");
  g.font = "12px system-ui, sans-serif";
  for (let k = 0; k <= 4; k += 1) {
    const v = lo + ((hi - lo) * k) / 4;
    g.beginPath();
    g.moveTo(pad.left, y(v));
    g.lineTo(width - pad.right, y(v));
    g.stroke();
    g.fillText(v.toFixed(0), 4, y(v) + 4);
  }
  for (let k = 0; k <= 5; k += 1) {
    const t = t0 + ((t1 - t0) * k) / 5;
    g.fillText(`${t.toFixed(1)} s`, x(t) - 12, height - 8);
  }

  for (const [i, name] of ["--x", "--y", "--z"].entries()) {
    g.strokeStyle = colour(name);
    g.lineWidth = 2;
    g.beginPath();
    samples.forEach((s, k) => (k === 0 ? g.moveTo(x(s[0]), y(s[i + 1])) : g.lineTo(x(s[0]), y(s[i + 1]))));
    g.stroke();
  }

  g.fillStyle = colour("--pulse");
  for (const p of results.pulses) {
    g.beginPath();
    g.arc(x(p.time), y(p.x), 4, 0, 2 * Math.PI);
    g.fill();
  }
}

async function start() {
  const response = await fetch("results.json");
  if (!response.ok) {
    throw new Error(`results.json answered ${response.status}`);
  }
  const results = await response.json();
  document.getElementById("summary").textContent =
    `The simulated grblHAL stage, run at ${results.rate}× on the simulation clock. ` +
    `It reported its position every ${results.interval * 1000} ms, each report arriving ${results.latency * 1000} ms later.`;

  const canvas = document.getElementById("plot");
  plot(canvas, results);
  window.addEventListener("resize", () => plot(canvas, results));

  const table = document.getElementById("motions");
  for (const m of results.motions) {
    const row = table.insertRow();
    row.insertCell().textContent = m.what;
    row.insertCell().textContent = `${m.start.toFixed(3)} s`;
    row.insertCell().textContent = `${m.planned.toFixed(3)} s`;
    row.insertCell().textContent = `${m.measured.toFixed(3)} s`;
    row.insertCell().textContent = `${((m.measured - m.planned) * 1000).toFixed(1)} ms`;
  }
}

start().catch((error) => {
  const summary = document.getElementById("summary");
  summary.textContent = `The results couldn't be read: ${error.message}`;
  summary.className = "bad";
});
