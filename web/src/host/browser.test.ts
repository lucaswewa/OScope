// The browser's host (W1.4), on a window faked for each test: its storage,
// a file saved and opened, the clipboard and its fallback, notifications,
// the window's title and full screen, links outside the app, and the host
// the app has.
import { afterEach, describe, expect, it, vi } from "vitest";

import type { HostAdapter } from "@/host/adapter";
import { browserHost, SAVED_FOR, type BrowserWindow } from "@/host/browser";
import { host, installHost } from "@/host/host";

/** A storage area that keeps what it is given, or refuses all of it. */
function area(refuses = false): Storage {
  const values = new Map<string, string>();
  const refuse = (): never => {
    throw new DOMException("refused", "QuotaExceededError");
  };
  return {
    getItem: (key: string) => (refuses ? refuse() : (values.get(key) ?? null)),
    setItem: (key: string, value: string) => void (refuses ? refuse() : values.set(key, value)),
    removeItem: (key: string) => void (refuses ? refuse() : values.delete(key)),
  } as unknown as Storage;
}

/** A window of the test's own, on the test's page, and the host on it. */
function fake(changes: Partial<BrowserWindow> = {}) {
  const timers: Array<{ run: () => void; delay: number }> = [];
  const win: BrowserWindow = {
    document,
    navigator: {},
    localStorage: area(),
    sessionStorage: area(),
    URL: { createObjectURL: vi.fn(() => "blob:saved"), revokeObjectURL: vi.fn() },
    open: vi.fn(),
    setTimeout: (run, delay) => timers.push({ run, delay }),
    ...changes,
  };
  return { win, browser: browserHost(win), timers };
}

/** Notifications, already answered `permission`; asked, the user answers `answer`. */
function notifications(permission: NotificationPermission, answer = permission, throws = false) {
  const shown: Array<[string, NotificationOptions | undefined]> = [];
  const Shown = class {
    static permission = permission;
    static requestPermission = vi.fn(async () => answer);
    constructor(title: string, options?: NotificationOptions) {
      if (throws) throw new TypeError("Illegal constructor");
      shown.push([title, options]);
    }
  };
  return { Notification: Shown as unknown as typeof Notification, shown };
}

/** A document that fills the screen when asked, and says so as a browser does. */
function screen() {
  const told = () => doc.dispatchEvent(new Event("fullscreenchange"));
  const doc = Object.assign(new EventTarget(), {
    title: "",
    fullscreenElement: null as unknown,
    documentElement: {
      requestFullscreen: vi.fn(async () => {
        doc.fullscreenElement = doc.documentElement;
        told();
      }),
    },
    exitFullscreen: vi.fn(async () => {
      doc.fullscreenElement = null;
      told();
    }),
  });
  return doc;
}

describe("the browser's host", () => {
  afterEach(() => {
    installHost(undefined);
    document.body.replaceChildren();
  });

  it("keeps what is kept between visits apart from the session's, by key", () => {
    const { win, browser } = fake();
    browser.kept.write("oscope.test", "kept");
    browser.session.write("oscope.test", "for now");
    expect(browser.kept.read("oscope.test")).toBe("kept");
    expect(browser.session.read("oscope.test")).toBe("for now");
    expect(browser.kept.read("oscope.other")).toBeUndefined();
    browser.kept.write("oscope.test", undefined);
    expect(browser.kept.read("oscope.test")).toBeUndefined();
    expect(win.sessionStorage.getItem("oscope.test")).toBe("for now");
  });

  it("keeps nothing, and says nothing, where the browser refuses", () => {
    const { win, browser } = fake({ localStorage: area(true) });
    expect(() => browser.kept.write("oscope.test", "lost")).not.toThrow();
    expect(browser.kept.read("oscope.test")).toBeUndefined();
    // A browser that blocks storage throws on reaching it at all.
    Object.defineProperty(win, "sessionStorage", {
      get: () => {
        throw new DOMException("blocked", "SecurityError");
      },
    });
    expect(() => browser.session.write("oscope.test", "lost")).not.toThrow();
    expect(browser.session.read("oscope.test")).toBeUndefined();
  });

  it("saves a file as a download of an address of its own, given up a minute later", async () => {
    const { win, browser, timers } = fake();
    const clicked: Array<{ href: string; download: string; inPage: boolean }> = [];
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (
      this: HTMLAnchorElement,
    ) {
      clicked.push({ href: this.href, download: this.download, inPage: this.isConnected });
    });
    const data = new Blob(["a note"], { type: "text/plain" });
    await browser.files.save("note.txt", data);
    expect(win.URL.createObjectURL).toHaveBeenCalledWith(data);
    expect(clicked).toEqual([{ href: "blob:saved", download: "note.txt", inPage: true }]);
    expect(document.body.children).toHaveLength(0);
    expect(timers.map((timer) => timer.delay)).toEqual([SAVED_FOR]);
    expect(win.URL.revokeObjectURL).not.toHaveBeenCalled();
    timers[0]?.run();
    expect(win.URL.revokeObjectURL).toHaveBeenCalledWith("blob:saved");
  });

  it("opens the files chosen, and none when the chooser is closed", async () => {
    const { browser } = fake();
    const inputs: HTMLInputElement[] = [];
    vi.spyOn(HTMLInputElement.prototype, "click").mockImplementation(function (
      this: HTMLInputElement,
    ) {
      inputs.push(this);
    });
    const opening = browser.files.open({ accept: [".txt", "image/*"], multiple: true });
    const chooser = inputs[0]!;
    expect([chooser.type, chooser.accept, chooser.multiple]).toEqual([
      "file",
      ".txt,image/*",
      true,
    ]);
    expect(chooser.isConnected).toBe(true);
    const file = new File(["a note"], "note.txt", { type: "text/plain" });
    Object.defineProperty(chooser, "files", { value: [file] });
    chooser.dispatchEvent(new Event("change"));
    expect(await opening).toEqual([file]);
    expect(chooser.isConnected).toBe(false);

    const cancelled = browser.files.open();
    const second = inputs[1]!;
    expect([second.accept, second.multiple]).toEqual(["", false]);
    second.dispatchEvent(new Event("cancel"));
    expect(await cancelled).toEqual([]);
    expect(document.body.children).toHaveLength(0);
  });

  it("copies with the page's clipboard, and says when it is refused", async () => {
    const writeText = vi.fn(async (text: string) => {
      if (text === "refused") throw new DOMException("not allowed", "NotAllowedError");
    });
    const { browser } = fake({ navigator: { clipboard: { writeText } } });
    await browser.copy("a note");
    expect(writeText).toHaveBeenCalledWith("a note");
    await expect(browser.copy("refused")).rejects.toThrow("not allowed");
  });

  it("copies from a selection where the page has no clipboard, and gives the focus back", async () => {
    const { browser } = fake();
    const button = document.createElement("button");
    document.body.append(button);
    button.focus();
    const copied: string[] = [];
    document.execCommand = vi.fn((command: string) => {
      const field = document.activeElement as HTMLTextAreaElement;
      copied.push(`${command}: ${field.value.slice(field.selectionStart, field.selectionEnd)}`);
      return copied.length === 1;
    });
    try {
      await browser.copy("a note");
      expect(copied).toEqual(["copy: a note"]);
      expect(document.activeElement).toBe(button);
      expect(document.querySelector("textarea")).toBeNull();
      await expect(browser.copy("again")).rejects.toThrow("didn't copy");
    } finally {
      delete (document as Partial<Document>).execCommand;
    }
  });

  it.each([
    ["granted", "granted", true, 0],
    ["default", "granted", true, 1],
    ["default", "denied", false, 1],
    ["denied", "denied", false, 1],
    // As headless Chromium is: "denied" said for permission given.
    ["denied", "granted", true, 1],
  ] as const)("notifies, where %s and then %s: %s", async (was, answer, shown, asked) => {
    const told = notifications(was, answer);
    const { browser } = fake({ Notification: told.Notification });
    expect(await browser.notify("Done", "The scan ended")).toBe(shown);
    expect(told.shown).toEqual(shown ? [["Done", { body: "The scan ended" }]] : []);
    expect(told.Notification.requestPermission).toHaveBeenCalledTimes(asked);
  });

  it("says a notification wasn't shown where the browser has none, or refuses one", async () => {
    expect(await fake().browser.notify("Done")).toBe(false);
    const refusing = notifications("granted", "granted", true);
    expect(await fake({ Notification: refusing.Notification }).browser.notify("Done")).toBe(false);
    const quiet = notifications("granted");
    await fake({ Notification: quiet.Notification }).browser.notify("Done");
    expect(quiet.shown).toEqual([["Done", {}]]);
  });

  it("names the window, and fills the screen and gives it back", async () => {
    const doc = screen();
    const { browser } = fake({ document: doc as unknown as Document });
    browser.window.title("OScope: the host");
    expect(doc.title).toBe("OScope: the host");
    const told: boolean[] = [];
    const stop = browser.window.onFullScreen((full) => told.push(full));
    expect(browser.window.isFullScreen()).toBe(false);
    await browser.window.enterFullScreen();
    expect(browser.window.isFullScreen()).toBe(true);
    await browser.window.leaveFullScreen();
    await browser.window.leaveFullScreen();
    expect(doc.exitFullscreen).toHaveBeenCalledTimes(1);
    stop();
    await browser.window.enterFullScreen();
    expect(told).toEqual([true, false]);
    const element = { requestFullscreen: vi.fn(async () => undefined) };
    await browser.window.enterFullScreen(element as unknown as Element);
    expect(element.requestFullscreen).toHaveBeenCalledTimes(1);
    expect(doc.documentElement.requestFullscreen).toHaveBeenCalledTimes(2);
  });

  it("opens a web page's or an email's address outside the app, and no other", () => {
    const { win, browser } = fake();
    browser.openOutside("https://example.org/docs");
    browser.openOutside("mailto:someone@example.org");
    browser.openOutside("#/dev/gallery");
    expect(vi.mocked(win.open).mock.calls).toEqual([
      ["https://example.org/docs", "_blank", "noopener,noreferrer"],
      ["mailto:someone@example.org", "_blank", "noopener,noreferrer"],
      [new URL("#/dev/gallery", document.baseURI).href, "_blank", "noopener,noreferrer"],
    ]);
    expect(() => browser.openOutside("javascript:alert(1)")).toThrow("a javascript: address");
    expect(() => browser.openOutside("file:///C:/notes.txt")).toThrow("a file: address");
    expect(win.open).toHaveBeenCalledTimes(3);
  });

  it("is the browser, with neither of the desktop app's abilities", () => {
    const { browser } = fake();
    expect(browser.platform).toBe("browser");
    expect(browser.can).toEqual({ sharedMemoryPreview: false, tilesFromDisk: false });
  });

  it("is the host the app has, until another is installed in its place", () => {
    const first = host();
    expect(first.platform).toBe("browser");
    expect(host()).toBe(first);
    const desktop: HostAdapter = { ...first, platform: "desktop" };
    installHost(desktop);
    expect(host()).toBe(desktop);
    installHost(undefined);
    expect(host().platform).toBe("browser");
  });
});
