// The app's lint rules (ADR-0039): the usual ones for TypeScript and Vue, the
// layers' rule, and what a Tauri-ready app may not do outside its two doors:
// the server's client, and the host adapter (ADR-0046).
import js from "@eslint/js";
import vue from "eslint-plugin-vue";
import tseslint from "typescript-eslint";

/** `src/`'s layers, and what each may not import. */
const layers = {
  ui: ["@/api", "@/host", "@/features", "@/app"],
  api: ["@/ui", "@/host", "@/features", "@/app"],
  host: ["@/ui", "@/api", "@/features", "@/app"],
  features: ["@/app"],
};

/**
 * A layer's rule: not the layers it may not know, reka-ui only in `ui/`,
 * OpenSeadragon only in `ui/tile-viewer/`, which `tiles` says it is, Vue
 * Flow and dagre only in the graph editor, which `graphs` says it is, and
 * VTK.js only in the Workspace's 3D, which `three` says it is.
 */
function layerRule(layer, forbidden, { tiles = false, graphs = false, three = false } = {}) {
  const patterns = forbidden.map((name) => ({
    group: [name, `${name}/**`],
    message: `src/${layer}/ may not import from ${name.slice(2)}/ (ADR-0039).`,
  }));
  if (layer !== "ui") {
    patterns.push({
      group: ["reka-ui", "reka-ui/**"],
      message: "Only src/ui/ imports reka-ui: use its components (ADR-0039).",
    });
  }
  if (!tiles) {
    patterns.push({
      group: ["openseadragon"],
      message: "Only src/ui/tile-viewer/ imports OpenSeadragon: use TileViewer (ADR-0050).",
    });
  }
  if (!graphs) {
    patterns.push({
      group: ["@vue-flow/*", "@vue-flow/**", "@dagrejs/*"],
      message:
        "Only src/features/workspace/editor/ imports Vue Flow and dagre: use its GraphCanvas (ADR-0053).",
    });
  }
  if (!three) {
    patterns.push({
      group: ["@kitware/vtk.js", "@kitware/vtk.js/**"],
      message: "Only src/features/workspace/three-d/ imports VTK.js (ADR-0054).",
    });
  }
  return { "no-restricted-imports": ["error", { patterns }] };
}

/**
 * What only `api/` and `host/` may touch: the network, and the platform,
 * which the host adapter gives the rest of the app (ADR-0046).
 */
const platform = {
  "no-restricted-globals": [
    "error",
    ...["fetch", "WebSocket", "EventSource", "XMLHttpRequest"].map((name) => ({
      name,
      message: `Only src/api/ talks to the server: ${name} isn't for here (ADR-0039).`,
    })),
    ...["localStorage", "sessionStorage", "indexedDB"].map((name) => ({
      name,
      message: "Only src/host/ keeps state: use host().kept or host().session (ADR-0046).",
    })),
    ...[
      ["open", "host().openOutside"],
      ["Notification", "host().notify"],
      ["showSaveFilePicker", "host().files.save"],
      ["showOpenFilePicker", "host().files.open"],
    ].map(([name, instead]) => ({
      name,
      message: `${name} is the platform's: use ${instead} from src/host/ (ADR-0046).`,
    })),
  ],
  "no-restricted-properties": [
    "error",
    {
      object: "window",
      property: "fetch",
      message: "Only src/api/ talks to the server: window.fetch isn't for here (ADR-0039).",
    },
    ...[
      ["window", "open", "host().openOutside"],
      ["window", "localStorage", "host().kept"],
      ["window", "sessionStorage", "host().session"],
      ["navigator", "clipboard", "host().copy"],
      ["document", "execCommand", "host().copy"],
      ["document", "title", "host().window.title"],
      ["document", "exitFullscreen", "host().window.leaveFullScreen"],
      ["document", "fullscreenElement", "host().window.isFullScreen"],
    ].map(([object, property, instead]) => ({
      object,
      property,
      message: `${object}.${property} is the platform's: use ${instead} from src/host/ (ADR-0046).`,
    })),
    {
      property: "requestFullscreen",
      message:
        "Full screen is the platform's: use host().window.enterFullScreen from src/host/ (ADR-0046).",
    },
  ],
  "no-restricted-syntax": [
    "error",
    {
      selector: "Literal[value=/^(https?|wss?):\\/\\//]",
      message: "No absolute URLs: the app is served by its server, or told its address (ADR-0039).",
    },
  ],
};

export default tseslint.config(
  {
    ignores: [
      "dist/",
      ".bundle/",
      "node_modules/",
      "test-results/",
      "playwright-report/",
      // Generated: data recorded from the simulated instrument (scripts/record-charts.mjs).
      "src/features/dev/charts/recorded.ts",
      // Generated: the API's types (scripts/api-types.mjs, ADR-0043).
      "src/api/generated/",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...vue.configs["flat/essential"],
  {
    files: ["**/*.vue"],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
  },
  {
    // TypeScript checks names; in scripts and configurations, Node provides them.
    rules: { "no-undef": "off" },
  },
  {
    // The app's own components are named for what they are: Icon, Button. In
    // a template, the capital sets them apart from HTML's elements.
    files: ["src/ui/**/*.vue"],
    rules: { "vue/multi-word-component-names": "off" },
  },
  ...Object.entries(layers).map(([layer, forbidden]) => ({
    files: [`src/${layer}/**`],
    rules: layerRule(layer, forbidden),
  })),
  {
    files: ["src/ui/tile-viewer/**"],
    rules: layerRule("ui", layers.ui, { tiles: true }),
  },
  {
    files: ["src/features/workspace/editor/**"],
    rules: layerRule("features", layers.features, { graphs: true }),
  },
  {
    files: ["src/features/workspace/three-d/**"],
    rules: layerRule("features", layers.features, { three: true }),
  },
  {
    files: ["src/app/**", "src/main.ts"],
    rules: layerRule("app", []),
  },
  {
    files: ["src/**"],
    ignores: ["src/api/**", "src/host/**", "src/**/*.test.ts"],
    rules: platform,
  },
);
