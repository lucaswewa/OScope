// The app's lint rules (ADR-0039): the usual ones for TypeScript and Vue, the
// layers' rule, and what a Tauri-ready app may not do outside its two doors.
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

/** A layer's rule: not the layers it may not know, and reka-ui only in `ui/`. */
function layerRule(layer, forbidden) {
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
  return { "no-restricted-imports": ["error", { patterns }] };
}

/** What only `api/` and `host/` may touch: the network, storage, and windows. */
const platform = {
  "no-restricted-globals": [
    "error",
    ...["fetch", "WebSocket", "EventSource", "XMLHttpRequest"].map((name) => ({
      name,
      message: `Only src/api/ talks to the server: ${name} isn't for here (ADR-0039).`,
    })),
    ...["localStorage", "sessionStorage", "indexedDB"].map((name) => ({
      name,
      message: `Only src/host/ keeps state: ${name} isn't for here (ADR-0039).`,
    })),
  ],
  "no-restricted-properties": [
    "error",
    ...["open", "localStorage", "sessionStorage", "fetch"].map((property) => ({
      object: "window",
      property,
      message: `window.${property} is the platform's: go through src/host/ or src/api/ (ADR-0039).`,
    })),
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
    files: ["src/app/**", "src/main.ts"],
    rules: layerRule("app", []),
  },
  {
    files: ["src/**"],
    ignores: ["src/api/**", "src/host/**", "src/**/*.test.ts"],
    rules: platform,
  },
);
