# ADR-0048: The shell, and the lease's read-only mode

- Status: Accepted
- Date: 2026-11-19
- Phase: W2.1

## Context

- **goal.md names the app's tabs:** Control and Live View, Scan Workflow, Workspace, Gallery, Settings, Jobs, Logging, About and Power, with the instrument's status indicator. **look.md lays them out:** a rail of 84 px, the working tabs at the top and the instrument's own at the bottom, then a control pane of 400 px and the view.
- **Routes go by hash** ([ADR-0039](0039-the-web-app-its-layers-state-and-routing.md)), each page a chunk loaded when first opened.
- **With accounts, one account holds the control lease** ([ADR-0031](0031-authentication-and-the-control-lease.md)). W1.3's `useCanControl()` says whether this app may change the instrument: always without accounts, and with them while it holds the lease.
- **Every control is one of the app's components** ([ADR-0042](0042-the-component-library-on-reka-ui.md)). Each takes `disabled`, and most go through `useControl`.

## Decision

**1. The shell** (`web/src/app/AppShell.vue`) is the frame of every tab:
- **the rail** (`ui/Rail.vue`): the nine tabs, an icon over a short label, as one toolbar, with the place of the status indicator below them (W2.2);
- **the tab,** in the main region, its route a child of the shell's (`app/tabs.ts`). The first is Control. A tab whose stage is still to come says which;
- **the component gallery** (`#/dev/gallery/…`) stays in every build, at its address, out of the rail. So does the login.

**2. The read-only mode locks a part of the page,** not each control by hand:
- **`ControlScope`** (`ui/ControlScope.vue`) locks, or frees, the controls inside it. `useDisabled`, through `useControl` and in each button, switches a control off while its part of the page is locked.
- **The shell locks the tab** while `useCanControl()` says no.
- **A tab frees what changes nothing on the instrument,** with a scope of its own: the display's theme and accent; a log's filters, in W4.2.
- **One line says why,** above the tab, and offers the way to control: asking whoever holds the lease, or taking it while nobody does. It is outside the locked part.

**3. Keys:**
- **the rail** is one stop for Tab, with the arrow keys, Home and End from tab to tab;
- **the shortcuts** are a registry (`app/shortcuts.ts`): a key, its words and what it runs. The keys 1 to 9 open the tabs in the rail's order, and `?` lists every shortcut. A key typed in a field, or pressed with Ctrl, Alt or the command key, is not a shortcut;
- **a change of route takes the focus to the tab's heading;**
- **a link skips to the content.** It is a button, since a link to `#content` would be a route.

**4. Failures stay in their tab.** A tab's boundary (`app/TabBoundary.vue`) catches what its page throws, and a page that can't load. It says what went wrong, and offers to load the tab again. **An action the server refuses or fails** is raised as a notice in the server's words, by the shell, from `onActionFailed` (`api/things/use.ts`). A cancellation, an action left, an ended session and a lost server aren't: each is told its own way.

**5. The window's title** is the tab's: "Control and Live View · OScope".

## Consequences

- **Gains:**
  - Every control a later phase adds is locked with the tab, by being one of the app's components; nothing is wired by hand.
  - A tab that fails can be loaded again without reloading the app.
  - Keyboard users reach every tab in one stop, or a digit.
- **Costs:**
  - A control drawn without the app's components isn't locked: W3's viewer must ask `useCanControl()` itself where it acts on the instrument by pointing.
  - The digits are the tabs': a later shortcut takes another key.
- **To watch:**
  - **A tab with its own keys,** the live viewer's arrows: they are handled where the viewer has the focus, and the registry's keys stay global.

## Alternatives considered

- **Each control asking `useCanControl()`:** every control in every later phase would need the same line, and one forgotten would act.
- **Read-only by CSS,** a veil over the tab: it hides nothing from the keyboard or a screen reader.
- **Shortcuts with a prefix,** `g` then a letter: no letter suits all nine tabs, and the digits follow the rail.
