# ADR-0053: The graph editor on Vue Flow

- Status: Accepted
- Date: 2026-11-23
- Phase: W7.2

## Context

- **Recipes are graphs** (ADR-0017): nodes, the edges between their ports, and the recipe's own inputs and outputs. The Workspace draws and edits them (D12), through the `pipeline` Thing (ADR-0052).
- **What the editor must do** (W7.2, W7.3):
  - nodes from a searched palette;
  - ports coloured by type, and a connection between unlike types refused as it is dragged;
  - delete, duplicate, copy and paste, undo and redo;
  - an automatic layout, and a minimap;
  - the keyboard's path to everything, connections included;
  - the recipe's JSON and the graph, each loading from the other without loss;
  - `check`'s faults on their nodes and ports.
- **A recipe keeps no places for its nodes,** and its JSON refuses fields it doesn't know.
- **The app's rules** (ADR-0039): `src/api/` alone talks to the server; libraries that own part of the page are wrapped once, as OpenSeadragon is in the tile viewer (ADR-0050).

## Decision

**1. Vue Flow,** `@vue-flow/core` 1.48.2 and `@vue-flow/minimap` 1.5.4, MIT.
- Drawing, dragging, zooming, connecting by dragging, selection and the minimap are its.
- It is imported only in `web/src/features/workspace/editor/`: a lint rule refuses it anywhere else.

**2. The recipe is the graph.**
- The editor keeps a recipe, never a graph of its own.
- Every edit is a function from a recipe to the next: a node added, an edge made, nodes deleted, pasted or moved, the layout.
- The canvas draws whatever recipe it is given, and what is done to it comes back as an edit.
- Undo and redo keep recipes: a hundred, the newest last.
- The recipe's own inputs are a node, `input`, and its outputs another, `output`, whose ports are the outputs. An output joined to its *new output* port becomes an output named after the port it comes from.

**3. A recipe keeps its nodes' places,** in an optional `layout`: each node's `[x, y]`, by its ID. The engine ignores it. A recipe without one, imported or opened, is laid out automatically.

**4. A connection is refused as it is dragged** unless the two ports' types are the same, and neither is the recipe's own end the wrong way round. The same rule decides a connection made from the keyboard.

**5. The keyboard's path:**
- Tab moves between the nodes and their ports, each port a button.
- An output's button starts a connection, and an input's ends it, or says why it can't.
- Escape stops it.
- The editor's chords: undo, redo, copy, paste and duplicate. Delete deletes what is selected.
- What happens is said in a polite live region.

**6. The automatic layout** is `@dagrejs/dagre` 3.1.3, MIT: the graph in layers from left to right, from the recipe's inputs to its outputs. It too is the editor's alone.

**7. `check` runs as the recipe changes,** 300 ms after the last edit. Its faults show on the nodes and ports they name (ADR-0052), and those at no node above the canvas.

## Consequences

- **Undo, redo, import and export are simple:** a recipe is the whole state.
- **A recipe's file keeps its places,** and one written by hand, or a stock recipe, is laid out when first opened.
- **Vue Flow's own state** holds only what is being dragged and what is selected. A node is drawn anew after each edit, so the editor keeps the selection.
- **The bundle grows by Vue Flow, its d3 parts and dagre,** in the Workspace's own chunk, loaded when the tab first opens.
- **`@vueuse/core` 10, Vue Flow's,** sits beside the app's own version 14.

## Alternatives considered

- **A canvas of our own on SVG:** dragging, zoom, connection lines and a minimap to write and test. Vue Flow has them, and is the Vue form of the most used graph editor on the web.
- **Rete.js, LiteGraph:** their own rendering, their own model of a graph, kept in step with the recipe's; LiteGraph draws on a canvas, which a screen reader can't read.
- **ELK for the layout:** better routing, but under the EPL, which the app's bundle doesn't take (ADR-0002).
- **The places in the editor's own storage, by recipe:** lost with an exported file, and on another computer.
