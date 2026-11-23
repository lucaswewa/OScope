# ADR-0052: Recipes over the API: the `pipeline` Thing

- Status: Accepted
- Date: 2026-11-23
- Phase: W7.1

## Context

- **The graph editor needs a server to talk to** (W7.2, W7.3): the nodes it may draw, whether a recipe can run and why not, recipes kept by name, and a recipe run on a project's records.
- **Plan §5.4 lists a `pipeline` Thing:** the node catalogue, recipes kept, and runs as jobs. P21 left it to the editor's phase.
- **What the server has already:**
  - **Recipes** (ADR-0017) are checked by `Plan::new`, which finds every problem at once, each placed by a string: `nodes[1] (blur)`, `edges[0]`.
  - **`preview/nodes`** lists the nodes live recipes may use. It needs the preview, and so the devices.
  - **`workspace/run`** (P48) runs a recipe on one record of a project, as a `workspace.run` job.
  - **Report templates** (P47) are a library of the same shape: OScope's own, which can't be changed, and the user's, in the data root.
- **Every action needs the lease** (ADR-0031): teta-wot counts an action as a change. OScope's own authoriser already lets one affordance through, `settings/display`.

## Decision

**1. The `pipeline` Thing**, on every server, devices or not:
- **`nodes`,** a property: the stock nodes and the extensions', each with its ports and its parameters' schema. `preview/nodes` stays, and gives the same.
- **`check`,** an action: every fault of a recipe at once.
- **The library:** `recipes` and `recipe/{name}`, endpoints; `save_recipe` and `delete_recipe`, actions; and `changed`, an event, with the name.
- **`run`,** an action: a recipe on a project's records, a job each.

**2. A fault says where it is.**
- Each keeps the place `Plan::new` gives, `at`, and adds the node it is at, `node`, by its ID in the recipe, and its port, `port`, where it has them.
  - The recipe's own inputs are the node `input`.
  - An edge's fault is at its end: a type mismatch, or a second edge, at the input it goes to; a missing port at the end that names it.
  - A parameter's fault is at its node, its JSON Pointer in `at`.
  - A cycle is one fault, at the node it starts from, its message naming the rest.
- `check` takes any JSON: JSON that isn't a recipe is one fault, at `recipe`.
- **It counts a GPU as present** when the server is built with CUDA. A node that runs only on a GPU passes `check` on such a build without one, and fails when run. No stock node needs a GPU.

**3. `check` needs no lease.** It changes nothing, so the authoriser lets it through, as it does `settings/display`. A viewer may check the recipe they look at.

**4. The library:**
- **Stock recipes,** built into `oscope-pipeline`, can't be changed or deleted.
- **The user's** are kept as `<data root>/recipes/<name>.json`, each written beside its place and moved there whole.
- **A name** has 1 to 64 characters of `a`–`z`, `0`–`9`, `-` and `_`, as a template's does. It can't leave the folder, and isn't a stock recipe's.
- **Saving keeps a recipe with its faults:** an editor keeps work half made. Only `run` refuses one.
- **A list** gives each recipe's name, its title and whether it is stock, the stock ones first.

**5. `run` is the Workspace's run, for many records.**
- It takes the library's recipe by name, `{"named": …}`, or one given, `{"given": …}`; the records; and optionally a region in µm and a tile size.
- It submits a `workspace.run` job for each record, so each result has P48's provenance.
- It refuses a recipe with faults, or no records, before any job.

## Consequences

- **The editor has all it needs** without devices: the catalogue, the faults where they are, the library, and runs.
- **A fault's place is data, not text:** the editor marks the node and port without parsing `at`.
- **Two catalogues give the same,** `preview/nodes` and `pipeline/nodes`, the second without the preview.
- **A user's recipe that no longer reads,** after a format change, is listed by its name and can't be opened. It is deleted, or fixed by hand.
- **The library has no versions:** a recipe saved replaces the last.

## Alternatives considered

- **`check` as an endpoint, `POST`:** the same lease question, since a `POST` counts as a change, and no generated types. An action has both.
- **`check` needing the lease, as other actions do:** a viewer couldn't see why a recipe they look at can't run.
- **Saving only recipes that pass `check`:** an editor couldn't keep work half made, and a node's extension, missing at one start, would make a kept recipe unsaveable.
- **Recipes kept in the project store:** recipes belong to no project, and are used across them.
- **`run` as a job of its own kind,** for many records: P48's `workspace.run` already keeps a result's provenance, and its jobs are listed and followed one by one.
