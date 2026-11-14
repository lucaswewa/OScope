// Shows the example's filters.json: the field, then each stock node's
// output beside it, with its times on the CPU and the GPU.
"use strict";

function card(title, file, lines) {
  const figure = document.createElement("figure");
  const link = document.createElement("a");
  link.href = file;
  const img = document.createElement("img");
  img.src = file;
  img.alt = title;
  img.loading = "lazy";
  link.append(img);
  const caption = document.createElement("figcaption");
  const name = document.createElement("div");
  name.textContent = title;
  caption.append(name);
  for (const line of lines) {
    const text = document.createElement("div");
    text.className = "times";
    text.textContent = line;
    caption.append(text);
  }
  figure.append(link, caption);
  return figure;
}

async function start() {
  const response = await fetch("filters.json");
  if (!response.ok) throw new Error(`filters.json answered ${response.status}`);
  const summary = await response.json();
  const [width, height] = summary.field;
  document.getElementById("summary").textContent =
    `A ${width} × ${height} field of the simulated H&E slide through the ${summary.objective} objective, ` +
    `through each of P23's stock nodes${summary.gpu ? ", on the CPU and the GPU" : ", on the CPU (build with --features cuda for the GPU)"}.`;
  const cards = document.getElementById("cards");
  cards.append(card("The field", "field.png", ["the input"]));
  cards.append(card("The field, 2% of its pixels black or white", "noisy.png", ["the median's input"]));
  for (const result of summary.cards) {
    const lines = [`CPU ${result.cpu_ms.toFixed(1)} ms`];
    if (result.gpu_ms !== null) lines.push(`GPU ${result.gpu_ms.toFixed(1)} ms, with its copies`);
    if (result.note) lines.push(result.note);
    cards.append(card(result.name, result.file, lines));
  }
}

start().catch((error) => {
  document.getElementById("summary").textContent = error.message;
});
