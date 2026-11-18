# The app's look

OScope takes its look from `microscope`, the owner's earlier app (plan D1): its palette, its magenta accent, its compact density, and its rail → control pane → view layout. This page says what that look is, in our own words. [W0.2](phases.md#w0-2) turns it into tokens ([ADR-0041](../adr/0041-design-tokens-and-themes.md)), and the phases after it build their components from it.

## How it was taken

- **From the running app, not from its source** ([ADR-0002](../adr/0002-clean-room-and-dependency-licences.md)). `microscope` 0.2.0 was built and run on 2026-11-17, on its simulated instrument, and looked at in a browser.
- **What was looked at:** every tab, in light and in dark; the settings' pages; a dialog and an alert dialog; an open list, a tooltip and a notification; a control under the pointer, held down, and with the keyboard's focus; each of its nine accent colours.
- **How it was measured:** from the screenshots' pixels, at 1440 × 900 with one device pixel to a CSS pixel, in Chromium on Windows 11.
  - Colours are the pixels' own values.
  - Sizes are counted between edges.
  - Type sizes are worked out from the height of capital letters.
- **Nothing was read from its styles, its markup or its code.** The app was driven as a user drives it, by the names on its controls. Its screenshots aren't kept in this repository.
- **What couldn't be seen:** motion; scrollbars; a control that is switched off. Its pages have no slider, tabs, table or progress bar. Those are ours to design, in the same manner.

## In short

- **Flat and quiet.** Grey surfaces a shade apart, hairline borders, almost no shadow.
- **One accent,** magenta, for what is chosen, what acts, and what has the focus.
- **Compact.** Small type, close rows, controls that fill their pane's width.
- **Nearly square.** Fields have sharp corners, buttons barely rounded ones.
- **Thin headings** in the accent's colour, over regular body text.

## The layout

Three columns, each the window's full height:

| Part | Width | What it holds |
|---|---|---|
| **The rail** | 84 px | The tabs: an icon over a short label. The working tabs are at the top, the instrument's own (settings, logs, about, power) at the bottom, and the connection's state below them |
| **The control pane** | 400 px | The chosen tab's controls, in one column |
| **The view** | The rest | The live image, fitted in |

- **A hairline** separates each column from the next.
- **A tab without a live image** uses the pane's and the view's space as one page. Settings puts a 180 px list of its sections where the pane was.
- **A sidebar,** where a page has one, is a second pane of 400 px on the right.
- **The rail's items** are 64 px tall and as wide as the rail. The chosen one is filled with the accent, its icon and label in white.
- **Padding:** 12 px inside a pane; 24 px around a page's content; 48 px beside the settings' content.

## Colours

Each is a role, with a light and a dark value. A theme changes the values and nothing else.

### Surfaces

| Role | Light | Dark |
|---|---|---|
| The page, and the view behind the image | `#ffffff` | `#1f1f1f` |
| A pane or a sidebar | `#fafafa` | `#232323` |
| The rail | `#f2f2f2` | `#2b2b2b` |
| What is raised: a card, a dialog, a list, a notification | `#ffffff` | `#2a2a2a` |
| A field | `#ffffff` | `#2a2a2a` |
| An open section's header | `#ededed` | `#2e2e2e` |
| Under the pointer, for a control with no fill of its own | `#e3e3e3` | `#373737` |
| A tooltip, which is the theme turned over | `#1a1a1a` | `#e8e8e8` |

- **In light,** the rail is the darkest surface and the page the lightest. **In dark,** it is the other way round: the rail is the lightest.
- **The dimming behind a dialog** is black at 40 % in light, and at 60 % in dark.

### Text

| Role | Light | Dark |
|---|---|---|
| Text | `#1a1a1a` | `#e8e8e8` |
| Quieter text: hints, captions, a list's terms, the rail's labels | `#5c5c5c` | `#a8a8a8` |
| Text on a tooltip | `#ffffff` | `#1f1f1f` |

### Borders

| Role | Light | Dark |
|---|---|---|
| A hairline: between columns, around a card, between sections | `#dcdcdc` | `#3a3a3a` |
| Around a list or a menu | `#c4c4c4` | `#5c5c5c` |
| Around a field, a checkbox, a switch that is off | `#8c8c8c` | `#7a7a7a` |

A field with the focus takes the quieter text's colour for its border.

### The accent

Magenta, `#ce2283`, in both themes. Around it:

| Role | Light | Dark |
|---|---|---|
| A fill: a primary button, the chosen tab, a switch that is on | `#ce2283` | `#ce2283` |
| What is written on the fill | `#ffffff` | `#ffffff` |
| The fill under the pointer | `#b41d73` | `#b41d73` |
| The fill held down | `#95185f` | `#95185f` |
| The accent as text or as a line: a link, a heading, an outlined button, the focus ring | `#b41d73` | `#ea7bba` |
| A tint, behind what is chosen in a list, and under the pointer on an outlined button | `#fbe9f3` | `#420b2a` |

- **The fill doesn't change with the theme.** The accent as text does: darker than the fill on light surfaces, lighter on dark ones.
- **The tint** is the accent at about a tenth over white, and at about a third over black.
- **Headings** take the accent's text colour in light. In dark they are plain text.

The app offers nine accents. Each has a fill, what is written on it, and its text colour in each theme:

| Accent | Fill | On it | As text, light | As text, dark |
|---|---|---|---|---|
| Magenta, the default | `#ce2283` | white | `#b41d73` | `#ea7bba` |
| Amaranth purple | `#ab274f` | white | `#952245` | `#df7595` |
| Chinese violet | `#856088` | white | `#745477` | `#bca3be` |
| Fern green | `#4f7942` | white | `#456a3a` | `#95be89` |
| Mantis | `#74c365` | `#1a1a1a` | `#37712c` | `#b0dda8` |
| Hunter green | `#355e3b` | white | `#2e5233` | `#7cb584` |
| Midnight blue | `#191970` | white | `#161662` | `#babaf0` |
| Dusk blue | `#545aa7` | white | `#494f92` | `#9da1ce` |
| Harbor blue | `#205896` | white | `#1c4d83` | `#669fde` |

Mantis is light enough to need dark writing on it.

### States

| Role | Light | Dark | Seen on |
|---|---|---|---|
| Success | `#26732b` | `#6cc070` | The connection's dot; a notification's stripe and icon |
| Danger | `#c42b2b` | `#ff7a7a` | The text and border of a button that deletes or shuts down |

The app shows no warning or information colour. OScope adds an amber and a blue, chosen to read as well as these two do.

### How the colours read

By WCAG 2.2's contrast ratio, computed from the values above:

- **Text** on every surface: 13.6 or more in light, 9.7 or more in dark.
- **Quieter text** on every surface: 5.2 or more, and 5.0 or more.
- **White on the magenta fill:** 5.0. The other accents' writing on their fills: 5.1 to 14.9.
- **Each accent as text,** on the page, a pane and the rail: 5.2 or more in light, 4.8 or more in dark.
- **A field's border** against a page, a pane or a raised surface: 3.2 or more.

Two kinds of pair fall short, and OScope's tokens don't allow them: a field's border on the grey surfaces (2.6 to 3.0, where 3 is asked), and danger's text on the surface under the pointer in light (4.4, where 4.5 is asked).

## Type

- **The typeface** is the system's own for user interfaces: Segoe UI on Windows.
- **Sizes:**

  | Use | Size | Weight |
  |---|---|---|
  | Body, labels, buttons, fields | 14 px | Regular; labels and buttons semibold |
  | The rail's labels, captions, hints, the connection's name | 13 px | Regular |
  | A dialog's or a card's title | 20 px | Light |
  | A section's heading | 24 px | Light |
  | A page's title | 28 px | Light |

- **Lines** of body text are 21 px apart: one and a half times the size.
- **A caption over a group** is 13 px, in capitals, in the quieter colour.
- **A field's label** sits above it, semibold, 4 px away.

## Density

| Thing | Size |
|---|---|
| A control: a button, a field, a list's button | 40 px tall |
| A compact control, where several share a row | 30 px tall |
| A square button with only an icon | 40 × 40 px; 30 × 30 px without a fill |
| A list's choice | 30 px tall |
| A switch | 36 × 20 px, its knob 12 px |
| A checkbox | 16 × 16 px |
| A section's header | 40 px tall |
| An icon | 24 px in the rail; 20 px in a button |
| A notification | 48 px tall, about 350 px wide |
| A dialog | 800 px wide; an alert 400 px |

- **Gaps** come from a 4 px step: 4, 8, 12, 16, 24.
- **Rows in a pane** are 8 px apart; groups 12 to 16 px.
- **A pane's controls fill its width.** Buttons that share a row share it evenly.

## Corners, shadows and focus

- **Corners:**
  - a field: square;
  - a button, a checkbox, a tooltip: 2 px;
  - a card, a dialog, a list, a notification: 4 px;
  - a switch, a dot: round.
- **Shadows** are slight, and only on what floats:
  - a card: a pixel or two below it;
  - a list or a notification: about 6 px, soft;
  - a dialog: wide and soft, heavier below.
- **Focus** from the keyboard is a ring: 2 px of the accent's text colour, 2 px away from the control, its corners rounded. The surface shows between ring and control.

## Controls, as they look

- **Buttons:**
  - **Primary:** the accent's fill, white semibold text. One for each task.
  - **Outlined:** the accent's text colour for its text and its 1 px border, on a raised surface. Under the pointer it takes the tint.
  - **Bare:** an icon alone, no border. Under the pointer it takes the pointer's surface.
  - **Dangerous:** outlined, in the danger colour.
  - An icon sits before a button's text, 8 px away.
- **Fields:** a raised surface with a 1 px border, square. Text and numbers alike.
- **A list to choose from:** a field with a chevron at its right. Its choices open below it, on a raised surface with a border and a shadow. The chosen one is in the accent's text colour with a tick at its right. The one the keyboard is on has the tint behind it.
- **A switch:** a round-ended track. Off, it is outlined, with a grey knob at the left. On, it is filled with the accent, with a white knob at the right. Its label follows it.
- **A checkbox:** a small outlined square, its label after it.
- **A section that opens:** a header with a semibold title and a chevron at its right, hairlines above and below. Open, the header takes a grey surface.
- **A card:** a raised surface with a hairline and a slight shadow. Its title is thin, in the accent's text colour.
- **A dialog:** a raised surface, centred, over the dimmed app. A thin title in the accent's text colour, a bare close button at its top right, and its buttons at the bottom right: the primary one last.
- **A tooltip:** small text on the turned-over surface, above what it explains.
- **A notification:** at the bottom right. A raised surface with a hairline, a 4 px stripe of its state's colour down its left, the state's icon, its text, and a bare close button.
- **A list of sections,** as in Settings: plain items, 37 px apart. The chosen one has the tint behind it and semibold text in the accent's text colour.
- **Terms and their values,** as in About: the term in the quieter colour, the value beside it in text, 25 px a row.
- **Links:** the accent's text colour, underlined.

## Icons

- **Outlined and thin:** strokes about a pixel and a half wide at 24 px.
- **The rail's icons** are 24 px, in the quieter text colour; white on the chosen tab.
- **OScope uses Material Symbols,** outlined, at weight 300, which draws that stroke ([goal.md](../goal.md) names the family).

## What OScope does differently

- **Sizes are in `rem`,** from these pixel values at the usual 16 px root: the app follows the reader's own text size.
- **Every pair of text and background the tokens allow is checked** for WCAG 2.2's AA, by a test.
- **Warning and information** colours, which `microscope` doesn't show.
- **Motion** is short and plain, and off for a reader who asks for less of it.
- **A control that is switched off, scrollbars and the selection's colour** are ours.
- **The themes** are *System*, *Light* and *Dark*, as `microscope` offers them. System follows the computer as it changes.
