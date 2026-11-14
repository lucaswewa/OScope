// The Gallery's cards: the projects, searched and paged through the
// `projects` Thing, each with its thumbnail; a project's contents below.
const api = "/api/v1/projects";
const size = 12;
let offset = 0;
let text = "";
let tag = null;

const $ = (id) => document.getElementById(id);
const say = (message) => { $("status").textContent = message; };
const plural = (n, noun) => `${n} ${noun}${n === 1 ? "" : "s"}`;

function element(tag, text, className) {
  const e = document.createElement(tag);
  if (text !== undefined) e.textContent = text;
  if (className) e.className = className;
  return e;
}

async function json(response) {
  const body = await response.json();
  if (!response.ok) throw new Error(body.detail || response.statusText);
  return body;
}

// Invokes an action of the Thing and waits for it: its output.
async function invoke(action, input) {
  const headers = { "content-type": "application/json" };
  const init = { method: "POST", headers, body: JSON.stringify(input) };
  const started = await json(await fetch(`${api}/${action}`, init));
  for (;;) {
    const invocation = await json(await fetch(started.href));
    if (invocation.status === "completed") return invocation.output;
    if (invocation.status !== "pending" && invocation.status !== "running") {
      throw new Error(JSON.stringify(invocation.error ?? invocation));
    }
    await new Promise((done) => setTimeout(done, 50));
  }
}

function card(project) {
  const article = element("article", undefined, "card");
  const image = element("img");
  image.alt = "";
  image.src = `${api}/thumbnail/${project.id}`;
  image.addEventListener("error", () => { image.style.visibility = "hidden"; });
  const body = element("div");
  const counts = `${plural(project.acquisitions, "acquisition")} · ${plural(project.results, "result")}`;
  const created = new Date(project.created).toLocaleDateString();
  body.append(element("h2", project.name), element("p", project.sample, "muted"),
    element("p", `${counts} · made ${created}`, "muted"));
  for (const name of project.tags) {
    const chip = element("span", name, "tag");
    chip.addEventListener("click", (event) => {
      event.stopPropagation();
      tag = name;
      offset = 0;
      refresh();
    });
    body.append(chip);
  }
  article.append(image, body);
  article.addEventListener("click", () => show(project.id).catch((e) => say(e.message)));
  return article;
}

async function load() {
  let query = `?offset=${offset}&limit=${size}`;
  if (text) query += `&text=${encodeURIComponent(text)}`;
  if (tag) query += `&tag=${encodeURIComponent(tag)}`;
  const page = await json(await fetch(`${api}/list${query}`));
  $("cards").replaceChildren(...page.projects.map(card));
  const last = page.offset + page.projects.length;
  $("paging").textContent = page.total ? `${page.offset + 1}–${last} of ${page.total}` : "no projects";
  $("filter").hidden = !tag;
  $("filter").textContent = tag ? `${tag} ✕` : "";
  $("previous").disabled = offset === 0;
  $("next").disabled = last >= page.total;
  say(`${plural(page.total, "project")} found`);
}

const refresh = () => load().catch((e) => say(e.message));

async function show(id) {
  const contents = await json(await fetch(`${api}/project/${id}`));
  const { project } = contents;
  const rename = element("button", "Rename");
  rename.addEventListener("click", async () => {
    const name = prompt("New name", project.name);
    if (name) await invoke("rename", { id, name }).then(refresh).then(() => show(id)).catch((e) => say(e.message));
  });
  const remove = element("button", "Delete");
  remove.addEventListener("click", async () => {
    if (!confirm(`Delete ${project.name}, with everything in it?`)) return;
    await invoke("delete", { id }).then(() => $("details").replaceChildren()).then(refresh).catch((e) => say(e.message));
  });
  const items = element("ul");
  for (const a of contents.acquisitions) {
    const { width, height, levels } = a.image;
    const item = element("li", `${a.name}: ${width} × ${height}, ${plural(levels, "level")}, in `);
    item.append(element("code", `projects/${id}/acquisitions/${a.id}/raw.ome.zarr`));
    items.append(item);
  }
  for (const r of contents.results) {
    items.append(element("li", `${r.name}: a result from ${plural(r.provenance.inputs.length, "input")}, made by ${r.provenance.software}`));
  }
  const about = `${project.sample} · tags: ${project.tags.join(", ") || "none"} · changed ${new Date(project.modified).toLocaleString()}`;
  $("details").replaceChildren(element("h2", project.name), element("p", about, "muted"), rename, remove, items);
}

let typing;
$("search").addEventListener("input", (event) => {
  clearTimeout(typing);
  typing = setTimeout(() => { text = event.target.value.trim(); offset = 0; refresh(); }, 250);
});
$("filter").addEventListener("click", () => { tag = null; offset = 0; refresh(); });
$("previous").addEventListener("click", () => { offset = Math.max(0, offset - size); refresh(); });
$("next").addEventListener("click", () => { offset += size; refresh(); });
$("create").addEventListener("click", async () => {
  const name = prompt("The new project's name");
  if (name) await invoke("create", { name }).then(refresh).catch((e) => say(e.message));
});
refresh();
