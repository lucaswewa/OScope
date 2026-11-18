# ADR-0042: The component library on reka-ui

- Status: Accepted
- Date: 2026-11-17
- Phase: W0.3

## Context

- **The tabs are made of controls:** buttons, fields, lists, switches. Each must follow the WAI-ARIA Authoring Practices' keyboard pattern for its kind, take its look from the tokens ([ADR-0041](0041-design-tokens-and-themes.md)), and have a disabled and a read-only state: the control lease's read-only mode (W2.1) shows every control and lets none be changed.
- **The patterns are subtle.** A list that opens, a combobox, a slider and a tooltip each have a dozen rules of focus, keys and naming. goal.md names reka-ui, which implements them without styles.
- **A library used everywhere can't be changed** ([ADR-0039](0039-the-web-app-its-layers-state-and-routing.md)): only `src/ui/` imports it.
- **A library is someone else's choices.** Where one doesn't suit an instrument, the component must be able to differ, and say so.

## Decision

**1. The components are ours,** in `web/src/ui/`, one file each, named for what they are. A tab imports `Button`, never reka-ui: lint keeps it so.

**2. They are built on reka-ui's primitives where a pattern is intricate,** and on HTML's own elements where those are enough:

| On reka-ui | On HTML's elements | Our own |
|---|---|---|
| Checkbox, RadioGroup, Switch, ToggleButton, ToggleGroup, Slider, Select, Combobox, Tooltip | Button, IconButton, TextField, TextArea, Field | NumberField |

**3. The number field is written here,** on a text input that says it is a spin button. Its rules are an instrument's, not the library's:
- what is typed is taken on Enter or on leaving the field, not at each key;
- a value that isn't a number, or is outside its range, is refused, and the field says why. It is never moved into the range unasked;
- the arrow keys move it by a step, Page Up and Down by ten, Home and End to the range's ends, each taken at once;
- the wheel moves it only while it has the focus;
- a number is written with a point, and nothing else: no comma, no grouping.

**4. Every control takes the same five things:** `id`, `label`, `disabled`, `readonly` and `invalid`, with its value as `v-model`.
- **Switched off:** it can't be used, and the keyboard passes it by.
- **Read-only:** the keyboard reaches it, it shows its value, and nothing changes it. It says so (`aria-readonly`).

**5. A `Field` names the control inside it.** The control takes its id, its description (a hint, or an error in the hint's place) and its error state from the field around it, by provide and inject: a page wires nothing by hand. A group is named by the label's id.

**6. A component's own words** ("At most 0.7.", "Nothing matches.") are in `ui/words/`, and the app's messages take them in under `ui`.

**7. Styles come from the tokens,** scoped to their component. What is drawn at the page's root, a list or a tooltip, is out of a scoped style's reach: its style is a shared stylesheet in `ui/styles/`, with classes that start `ui-`. A field's box is shared the same way.

**8. Where the library falls short, the component makes up for it,** each time with a test that fails without it:
- **Radio buttons:** an arrow key moves the choice at the key itself. The library moves it only if the key is still down a moment later.
- **A list's field** writes the chosen label from its own options. The library reads it from the list's items, which go as the list closes: the field showed its placeholder for a moment after every choice.
- **A tooltip** stays while the keyboard's focus is on its control. The library hides one whose control scrolls, and Tab scrolls a control into view: an icon alone went unnamed.
- **A label** points at its control by id even when it holds it, so that a checker sees whose words a switched-off control's are.

**9. `Provider`,** at the app's root, holds what the components share: the tooltips' timing.

**10. They are tested where each thing is real:**
- **in a test's own page** (Vitest, happy-dom): rules, states and events that need no layout;
- **in a browser,** end to end, in Chromium, Firefox and WebKit: the keys of everything that opens, measures or moves the focus;
- **axe,** on each gallery page in both themes, in error, and on each open list by itself. The library hides the page behind an open list from a screen reader, as it should, so the page's own rules have nothing to judge there;
- **baselines,** of each gallery page in both themes.

**11. `exactOptionalPropertyTypes` is off for the app.** It forbids passing a value that may be absent to an optional property. In Vue an absent prop *is* `undefined`, and reka-ui's types aren't written for the flag. `strict` and `noUncheckedIndexedAccess` stay.

## Consequences

- **Gains:**
  - A tab is written with a dozen components that already know the keyboard, the themes and their states.
  - The library can be replaced, or a component rewritten, in `ui/` alone.
  - The four places where OScope differs from the library are named, and tested.
- **Costs:**
  - Ten more packages in what the app ships, all under MIT: reka-ui and what it stands on.
  - A new reka-ui may change what the four tests hold. They will say so.
  - A component's keys can't all be tested without a browser.
- **To watch:**
  - **A comma for a decimal point** is refused. When the app is given other languages (D29), the number field's reading must follow.
  - **The page behind an open list** is hidden from a screen reader by `aria-hidden`, not made inert. Tab can't reach it while the list is open.

## Alternatives considered

- **Writing every component ourselves:** months of the patterns' corner cases, which reka-ui has already met.
- **A library that brings its own look** (PrimeVue, Vuetify): the look is another's, and harder to bend to the tokens than a library with none.
- **The browser's own `<select>`:** its list can't be styled, and wouldn't look like the app.
- **reka-ui's own number field:** it moves a value into its range without a word, and reads numbers by locale.
- **Leaving the library's four behaviours as they are:** each was seen by a test or a measurement before it was changed.
