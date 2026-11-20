// The browser's host (W1.4, ADR-0046): its storage; a file saved as a
// download, and opened through its file chooser; its clipboard and its
// notifications; the document's title and full screen; and a new tab for a
// link outside the app. It is given its window, so that a test can give it
// a fake one.
import type { Choosing, HostAdapter, Store } from "./adapter";

/** What the browser's host uses of its window. */
export interface BrowserWindow {
  readonly document: Document;
  /** The clipboard is there only for a page served securely: by HTTPS, or from this computer. */
  readonly navigator: { readonly clipboard?: Pick<Clipboard, "writeText"> };
  readonly localStorage: Storage;
  readonly sessionStorage: Storage;
  readonly Notification?: typeof Notification;
  readonly URL: Pick<typeof URL, "createObjectURL" | "revokeObjectURL">;
  open(url: string, target: string, features: string): unknown;
  setTimeout(run: () => void, delay: number): unknown;
}

/**
 * How long a saved file's address outlives its download's start, in
 * milliseconds: the browser reads it after the click, in its own time.
 */
export const SAVED_FOR = 60_000;

/** The kinds of address a link outside the app may have. */
const OUTSIDE = ["http:", "https:", "mailto:"];

/**
 * A store in one of the browser's storage areas, read afresh each time: a
 * browser that blocks storage throws on reaching the area itself.
 */
function store(area: () => Storage): Store {
  return {
    read(key) {
      try {
        return area().getItem(key) ?? undefined;
      } catch {
        return undefined;
      }
    },
    write(key, value) {
      try {
        if (value === undefined) area().removeItem(key);
        else area().setItem(key, value);
      } catch {
        // Refused, as a private window or a full disk may: not kept.
      }
    },
  };
}

/** The browser's host, on `win`. */
export function browserHost(win: BrowserWindow): HostAdapter {
  const doc = (): Document => win.document;
  const full = (): boolean => Boolean(doc().fullscreenElement);

  return {
    platform: "browser",
    can: { sharedMemoryPreview: false, tilesFromDisk: false },
    kept: store(() => win.localStorage),
    session: store(() => win.sessionStorage),

    files: {
      async save(name, data) {
        const address = win.URL.createObjectURL(data);
        const link = doc().createElement("a");
        link.href = address;
        link.download = name;
        link.hidden = true;
        doc().body.append(link);
        link.click();
        link.remove();
        win.setTimeout(() => win.URL.revokeObjectURL(address), SAVED_FOR);
      },

      open(choosing: Choosing = {}) {
        const input = doc().createElement("input");
        input.type = "file";
        input.multiple = choosing.multiple ?? false;
        input.accept = (choosing.accept ?? []).join(",");
        input.hidden = true;
        return new Promise<File[]>((resolve) => {
          const chosen = (files: File[]): void => {
            input.remove();
            resolve(files);
          };
          input.addEventListener("change", () => chosen([...(input.files ?? [])]));
          input.addEventListener("cancel", () => chosen([]));
          doc().body.append(input);
          input.click();
        });
      },
    },

    async copy(text) {
      const clipboard = win.navigator.clipboard;
      if (clipboard) return clipboard.writeText(text);
      // Served by plain HTTP to another computer, the page has no clipboard:
      // the older way copies a selection, from a field of its own.
      const before = doc().activeElement;
      const field = doc().createElement("textarea");
      field.value = text;
      field.readOnly = true;
      field.style.position = "fixed";
      field.style.opacity = "0";
      doc().body.append(field);
      field.focus();
      field.select();
      const copied = doc().execCommand("copy");
      field.remove();
      if (before instanceof HTMLElement) before.focus();
      if (!copied) throw new Error("the browser didn't copy the text");
    },

    async notify(title, body) {
      const Shown = win.Notification;
      if (!Shown) return false;
      // Asked unless given: a browser asked again after a refusal answers at
      // once, without asking its user. Headless Chromium says "denied" here
      // for permission it was given, and answers a request truly.
      const answer = Shown.permission === "granted" ? "granted" : await Shown.requestPermission();
      if (answer !== "granted") return false;
      try {
        new Shown(title, body === undefined ? {} : { body });
        return true;
      } catch {
        // A browser that shows them only from a service worker refuses this one.
        return false;
      }
    },

    window: {
      title(text) {
        doc().title = text;
      },
      async enterFullScreen(element) {
        await (element ?? doc().documentElement).requestFullscreen();
      },
      async leaveFullScreen() {
        if (full()) await doc().exitFullscreen();
      },
      isFullScreen: full,
      onFullScreen(listener) {
        const changed = (): void => listener(full());
        doc().addEventListener("fullscreenchange", changed);
        return () => doc().removeEventListener("fullscreenchange", changed);
      },
      isHidden: () => doc().visibilityState === "hidden",
    },

    openOutside(address) {
      const link = new URL(address, doc().baseURI);
      if (!OUTSIDE.includes(link.protocol)) {
        throw new Error(`a ${link.protocol} address isn't opened outside the app`);
      }
      win.open(link.href, "_blank", "noopener,noreferrer");
    },
  };
}
