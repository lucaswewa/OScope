# ADR-0041: Design tokens and themes

- Status: Accepted
- Date: 2026-11-17
- Phase: W0.2

## Context

- **The look is `microscope`'s** (plan D1), taken from its running app and written down in [`look.md`](../milestone-2/look.md): quiet grey surfaces, one accent, compact, thin headings.
- **Every phase after this one builds components.** They need one vocabulary for colours, sizes and times, in a light and a dark scheme, with a choice of accents.
- **The bar is WCAG 2.2's AA.** A palette measured from another app passes or fails pair by pair, and a later change can break a pair nobody looks at.
- **Browsers can now do the work:** `light-dark()`, `color-mix()` and `color-scheme` are in Chromium 123, Firefox 120 and Safari 17.5. The desktop app's webviews are WebView2, which keeps itself current, and WebKitGTK from 2.46.
- **A picture of the app differs by system:** the typeface is the system's own, and each draws text its own way.

## Decision

**1. Tokens are CSS custom properties on the root,** in `web/src/ui/tokens/tokens.css`. That is the only file in which a colour is written: a test refuses one anywhere else in `src/`.

**2. A colour token is a role, not a shade:** a surface, text, a border, the accent, a state. Each has a light and a dark value, written `light-dark(light, dark)`.

**3. The theme is the root's `color-scheme`:**
- `light dark` by default, which is the theme *system*: the page follows the computer as it changes, with no script, from its first paint;
- one scheme where `data-theme` names it.

**4. The accent is the root's `data-accent`,** one of nine; magenta without it.
- An accent gives three values: its fill, what is written on the fill, and its colour as text in each scheme.
- The rest is made from the fill with `color-mix(in srgb, …)`: under the pointer, held down, and a tint.

**5. The tokens name the pairs they allow,** in `pairs.ts`: which token may be written or drawn on which. A test computes WCAG 2.2's contrast ratio for every pair, in each scheme, with each accent.
- Text needs 4.5 to 1. The allowance of 3 to 1 for large text isn't used: one rule, and thin titles gain from it.
- A line that marks a control (a field's border, the focus ring) needs 3 to 1.
- The test reads the stylesheet itself, and works each value out as CSS says: `var()`, `light-dark()`, `color-mix(in srgb)`. A colour with alpha can't be in a pair.
- A pair that isn't named isn't promised.

**6. Sizes:**
- type, spacing and the controls' sizes are in `rem`, and follow the reader's text size;
- hairlines, corners and the focus ring are in `px`;
- the one density is compact. Its sizes are tokens, so another can be added, and none is under the 24 px WCAG asks of a target.

**7. Focus from the keyboard** is one rule for every control: a ring 2 px wide, 2 px away, in the accent's text colour.

**8. Motion** has two durations and one curve. For a reader who asks for less, the durations are zero, and a base rule stops every animation and transition.

**9. The typeface is the system's.** No font is shipped.

**10. Icons are Material Symbols,** outlined, at weight 300, from the package `@material-symbols/svg-300`.
- Each icon the app uses is imported as its SVG file's text, into a registry, `ui/icons.ts`, under the name the app knows it by.
- `Icon` draws it inline, in the text's colour. Nothing is fetched at run time.
- The registry's names are the type of `Icon`'s `name`: one that isn't there fails the type check.

**11. The theme and the accent are a store,** `features/display`, kept in the browser's storage through `host/`. W8.1 keeps them with the account. The store also knows the scheme in use, for what the app draws itself.

**12. The gallery,** `#/dev/gallery`, shows every token, found by reading the stylesheet.

**13. Visual tests compare a page with baselines,** in Chromium, with Playwright's `toHaveScreenshot`.
- **A baseline is one system's picture.** They are made on the owner's PC, for Windows and, under WSL, for Linux, and kept in `web/tests/e2e/baselines/`, named by browser and system.
- **They run only with `OSCOPE_VISUAL=1`,** which the replays and `two-core.sh` set, as the GPU's and the hardware's tests have their switches.
- **A pixel may be off by a level or two and no more** (a threshold of 0.01). Playwright's own allowance, 0.2, is for other computers' rendering.
- **`scripts/baselines.sh` makes them again,** after a change that is meant to be seen.

**14. A component in `ui/` is named for what it is:** `Icon`, `Button`. The lint rule that asks for two words is off there.

## Consequences

- **Gains:**
  - A theme or an accent is an attribute: no component knows which is on.
  - Every allowed pair is proven to read, with every accent, and a token that breaks one fails by name.
  - A change to the look shows: in the contrast test, or in the baselines.
- **Costs:**
  - The app needs a browser from 2024 or later.
  - The test's reader must learn each CSS function the tokens come to use.
  - Baselines are made twice, once on each system, and each page's weigh about a megabyte.
  - The icons' package has 23,673 files, and adds 14 s to an install.
- **To watch:**
  - **A chosen theme's first frame.** A theme that differs from the computer's shows the computer's until the store has read what the browser kept.
  - **Thin titles need a light weight** in the system's typeface. WSL's has none, and draws them regular.
  - **GitHub's runners** have other typefaces: the visual tests don't run there.

## Alternatives considered

- **Two sets of values,** one under `[data-theme="dark"]` and again under a media query: the dark values written twice, or a script to follow the computer.
- **Tokens in TypeScript,** written out as CSS by the build: one more step, for what the cascade already does.
- **A preprocessor's variables:** fixed at build time, so no theme can change them.
- **Colours in OKLCH:** mixes that look more even, but the reader would need its arithmetic, and the look was measured in sRGB.
- **An icon font,** fetched at run time, with a moment of missing icons; or the icons' files copied into the repository: nothing to install, but their licence kept by hand.
- **A shipped typeface,** for the same picture everywhere: more to ship, and the system's typeface is the look.
- **Playwright's own allowance** for screenshots: a hairline made a shade darker passed it. Found by breaking a token on purpose.
