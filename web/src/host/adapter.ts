// The platform's services, behind one interface (W1.4, ADR-0046): what the
// app keeps on the computer, the files it saves and opens, the clipboard,
// notifications and its window. The browser's are in browser.ts; the
// desktop app installs its own (W9.1), and nothing else in the app knows
// which it has. Outside src/host/, lint refuses the platform's own calls,
// naming these in their place (ADR-0039).

/** A place the app keeps text by key. */
export interface Store {
  /** What is kept under `key`, if anything. */
  read(key: string): string | undefined;
  /**
   * Keeps `value` under `key`, or, given nothing, forgets the key. What the
   * platform refuses to keep, as a private window may, isn't kept: the
   * page's own state holds it for as long as the page lasts.
   */
  write(key: string, value: string | undefined): void;
}

/** The files a user may choose to open. */
export interface Choosing {
  /** Their types, as a file input's `accept` takes them: `.txt`, `image/png`, `image/*`. Any, without. */
  accept?: readonly string[];
  /** Whether more than one may be chosen. */
  multiple?: boolean;
}

/** Files saved on the computer, and opened from it. */
export interface Files {
  /**
   * Saves `data` as a file named `name`, or as the user names it: the
   * browser's download, or the desktop's dialog. Done when the platform has
   * the file.
   */
  save(name: string, data: Blob): Promise<void>;
  /** The files the user chooses: none if they think better of it. */
  open(choosing?: Choosing): Promise<File[]>;
}

/** The app's window. */
export interface AppWindow {
  /** Names the window, as its title bar, its tab and the task bar show it. */
  title(text: string): void;
  /** Fills the screen with `element`, or with the whole app. */
  enterFullScreen(element?: Element): Promise<void>;
  /** Gives the screen back, if anything fills it. */
  leaveFullScreen(): Promise<void>;
  /** Whether anything fills the screen. */
  isFullScreen(): boolean;
  /**
   * Calls `listener` each time that changes, the user's Escape among the
   * ways it may. Gives the means to stop.
   */
  onFullScreen(listener: (full: boolean) => void): () => void;
  /** Whether the app can't be seen now: its page hidden, or its window minimised (W4.1). */
  isHidden(): boolean;
}

/** What a platform can do beyond a browser. */
export interface Abilities {
  /** Frames read from the server's shared memory, on its own computer (W9.3). */
  readonly sharedMemoryPreview: boolean;
  /** A record's tiles read from the disk, not over HTTP (W9.4). */
  readonly tilesFromDisk: boolean;
}

/** Everything the platform gives the app (ADR-0046). */
export interface HostAdapter {
  /** Which platform it is. */
  readonly platform: "browser" | "desktop";
  /** What it can do beyond a browser. */
  readonly can: Abilities;
  /** What is kept between visits: the theme, a pane's size. */
  readonly kept: Store;
  /** What is kept until the window is closed: the session's token. */
  readonly session: Store;
  /** Files saved and opened. */
  readonly files: Files;
  /** Puts `text` on the clipboard. Rejects where the platform won't. */
  copy(text: string): Promise<void>;
  /**
   * Tells the user something outside the app's window: `title`, and under
   * it `body`. Whether it was shown: not where the platform can't, nor once
   * the user has said no. The app then says it its own way.
   */
  notify(title: string, body?: string): Promise<boolean>;
  /** The app's window. */
  readonly window: AppWindow;
  /**
   * Opens `address` outside the app: in a new tab, or in the system's
   * browser. A web page's or an email's only: any other throws.
   */
  openOutside(address: string): void;
}
