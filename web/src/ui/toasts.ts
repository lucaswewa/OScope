// Notifications (ADR-0042): what the app tells its user happened, a few at a
// time, the rest waiting their turn. `Toaster` draws them; anything may raise
// one with `notify`.
import { computed, ref } from "vue";

/** How much a notice matters: its colour, its icon, and how it is announced. */
export type Level = "info" | "success" | "warning" | "danger";

export interface Notice {
  level: Level;
  text: string;
  /** Something to do about it, as a button in the notice. */
  action?: { label: string; run: () => void };
  /** How long it stays, in milliseconds; 0 is until it is dismissed. Its level's time, when not given. */
  lasts?: number;
}

/** A notice that was raised, with the number it goes by. */
export interface Toast extends Notice {
  id: number;
}

/**
 * How long a notice of each level stays, in milliseconds. What went wrong
 * stays until its reader sends it away: it may be the only word of it.
 */
export const LASTS: Readonly<Record<Level, number>> = {
  info: 5000,
  success: 5000,
  warning: 10000,
  danger: 0,
};

/** How many notices show at once. */
export const AT_ONCE = 3;

/** How long this notice stays, in milliseconds: 0 is until it is dismissed. */
export function lasting(notice: Notice): number {
  return notice.lasts ?? LASTS[notice.level];
}

/** The time a queue goes by: the page's, or a test's. */
export interface Clock {
  set(run: () => void, ms: number): unknown;
  clear(timer: unknown): void;
  now(): number;
}

const page: Clock = {
  set: (run, ms) => setTimeout(run, ms),
  clear: (timer) => clearTimeout(timer as ReturnType<typeof setTimeout>),
  now: () => performance.now(),
};

/**
 * A queue of notices: the oldest `atOnce` show, and each of the others takes
 * the place of one that goes, in the order they were raised. A notice's time
 * runs only while it shows, and stops for all of them while they are paused.
 */
export function createToasts(atOnce = AT_ONCE, clock: Clock = page) {
  const raised = ref<Toast[]>([]);
  const shown = computed(() => raised.value.slice(0, atOnce));
  /** Each showing notice's time left, since when, and its running timer. */
  const times = new Map<number, { left: number; since: number; timer?: unknown }>();
  let paused = false;
  let next = 1;

  /** Gives each showing notice that has an end a running timer, unless paused. */
  function run(): void {
    for (const toast of shown.value) {
      const lasts = lasting(toast);
      if (lasts <= 0) continue;
      const time = times.get(toast.id) ?? { left: lasts, since: 0 };
      times.set(toast.id, time);
      if (paused || time.timer !== undefined) continue;
      time.since = clock.now();
      time.timer = clock.set(() => dismiss(toast.id), time.left);
    }
  }

  /** Sends a notice away, showing or waiting. One already gone is no error. */
  function dismiss(id: number): void {
    const time = times.get(id);
    if (time?.timer !== undefined) clock.clear(time.timer);
    times.delete(id);
    raised.value = raised.value.filter((toast) => toast.id !== id);
    run();
  }

  return {
    /** Those showing now, the oldest first. */
    shown,
    /** How many wait behind them. */
    waiting: computed(() => Math.max(0, raised.value.length - atOnce)),
    /** Raises a notice, and gives the number to dismiss it by. */
    notify(notice: Notice): number {
      const id = next++;
      raised.value = [...raised.value, { ...notice, id }];
      run();
      return id;
    },
    dismiss,
    /** Stops every showing notice's time, while its reader is at them. */
    pause(): void {
      if (paused) return;
      paused = true;
      const now = clock.now();
      for (const time of times.values()) {
        if (time.timer === undefined) continue;
        clock.clear(time.timer);
        time.timer = undefined;
        time.left = Math.max(0, time.left - (now - time.since));
      }
    },
    /** Lets their time run again, from where it stopped. */
    resume(): void {
      if (!paused) return;
      paused = false;
      run();
    },
  };
}

/** The app's notices. */
export const toasts = createToasts();

/** Raises a notice in the app. */
export const notify = toasts.notify;
