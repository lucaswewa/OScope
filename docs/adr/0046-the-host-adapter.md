# ADR-0046: The host adapter

- Status: Accepted
- Date: 2026-11-18
- Phase: W1.4

## Context

- **The app runs on two platforms** ([ADR-0039](0039-the-web-app-its-layers-state-and-routing.md)): a browser, and from W9 a Tauri 2 desktop app. Each gives the same services its own way: storage, files, the clipboard, notifications and the window.
- **ADR-0039 kept the platform in `host/`** by lint. W0.2, W0.5 and W1.3 put three helpers there: the browser's storage, a value kept between visits, and the session's token. Each was the browser's own, so the desktop app had no one place to give its own.
- **Two services will be the desktop app's alone:** frames from the server's shared memory (W9.3) and tiles read from the disk (W9.4). A page has to know whether it has them before it uses them.
- **A browser's services depend on how its page is served.** The clipboard and notifications need a secure context: HTTPS, or a page from the same computer. The instrument's server is often reached over plain HTTP, from another computer on the lab's network.
- **`microscope` had a host adapter.** Its idea may be reused ([ADR-0002](0002-clean-room-and-dependency-licences.md)), but not its code.

## Decision

**1. One interface, `HostAdapter`** (`web/src/host/adapter.ts`):
- **storage by key:** `kept`, between visits, and `session`, until the window is closed. Both hold text, and are read at once, not awaited;
- **files:** `save(name, data)`, and `open(choosing)`, which gives the files chosen;
- **`copy(text)`, and `notify(title, body)`,** which says whether the notification was shown;
- **the window:** its title; full screen, for the app or one element, and whether it is on, with a listener for its changes;
- **`openOutside(address)`:** a web page's or an email's address. Any other is refused;
- **`platform` and `can`:** which platform it is, and what it can do beyond a browser: the shared-memory preview, and tiles from the disk.

**2. `host()` gives the adapter** (`host/host.ts`). It is the browser's, built on `window`, unless another was installed with `installHost`. The desktop app installs its own before the app starts (W9.1), and a test installs a fake.

**3. The browser's implementation** (`host/browser.ts`) is given its window, so that a test can fake it:
- **storage:** `localStorage` and `sessionStorage`, each reached afresh. A refusal keeps nothing and throws nothing: a private window, a full disk, or storage blocked altogether;
- **a file saved** is a download of an object URL, revoked a minute later. The browser decides where it goes, and asks if its user set it to;
- **a file opened** is chosen through a file input. A chooser that is cancelled gives none;
- **the clipboard:** the Clipboard API where the page is served securely. Elsewhere, a selection is copied from a field of the host's own, and the focus given back. Either rejects when refused;
- **notifications:** permission is asked for unless it was given. Asked again after a refusal, a browser answers at once without asking its user, and nothing is shown;
- **the window:** the document's title, and the Fullscreen API;
- **a link outside** opens in a new tab, with no opener and no referrer.

**4. What the app keeps moves onto it,** each under a key of its own:
- the theme and the accent (`features/display`, `oscope.display`);
- the session's token (`features/login`, `oscope.session`);
- the values kept with `kept()`.

The browser's storage helpers of W0.2 and W1.3 are gone.

**5. ADR-0039's lint rules name the adapter.** Outside `api/` and `host/`, each of these is refused, with the adapter's call to use in its place:
- the browser's storage, `window.open` and `open`;
- the clipboard, and `document.execCommand`;
- `Notification`, and the file pickers;
- `document.title`, and full screen.

## Consequences

- **Gains:**
  - The desktop app gives its services in one place (W9.1), and no page changes.
  - A test can give the app a platform of its own.
  - A page served over plain HTTP can still copy.
  - A page asks `can` before a desktop app's service, instead of trying it.
- **Costs:**
  - **Storage is read at once,** since the theme must be known before the first paint. A desktop store that isn't the webview's own must be read into memory before the app starts.
  - **A file saved in a browser goes where its downloads go,** with no dialog unless the user asked the browser for one.
  - **The clipboard's fallback uses `document.execCommand`,** which browsers have deprecated, and still support.
- **To watch:**
  - **Browsers dropping `execCommand("copy")`:** a page served over plain HTTP then can't copy, and says so.
  - **The file chooser's `cancel` event** is in Chromium 113, Firefox 91 and Safari 16.4 onwards. In an older browser, a cancelled choice is left waiting.
  - **Images on the clipboard** (`ClipboardItem`), when a phase needs them.

## Alternatives considered

- **The File System Access API** (`showSaveFilePicker`), for a real dialog to save in the browser: only Chromium has it, and Playwright can't drive its dialog, so a file's arrival on the disk couldn't be tested.
- **Vue's `provide` and `inject` in place of `host()`:** stores and the router's guards run outside components, where `inject` isn't available.
- **Storage read by awaiting,** as Tauri's store plugin is: everything that reads the theme would wait for it, and the page would first be drawn in the wrong scheme.
- **The platform's calls left where they are,** each with a test for the desktop beside it: what ADR-0039 was written against.
