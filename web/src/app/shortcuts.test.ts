// The keyboard's shortcuts (W2.1): a key runs its shortcut; a key typed in
// a field, a chord, a repeat and a key handled already are left alone; a
// key is one shortcut's; and the list follows what is added and taken away.
import { afterEach, describe, expect, it, vi } from "vitest";

import { addShortcut, listen, press, shortcuts } from "@/app/shortcuts";

/** A key pressed on `target`, as the browser sends it. */
function pressed(key: string, target: HTMLElement = document.body, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}

describe("the keyboard's shortcuts", () => {
  const stops: Array<() => void> = [];
  afterEach(() => {
    stops.splice(0).forEach((stop) => stop());
    document.body.replaceChildren();
  });

  it("run a key's shortcut, and are listed in the order they were added", () => {
    const [first, second] = [vi.fn(), vi.fn()];
    stops.push(addShortcut({ key: "1", label: () => "Open Control", run: first }));
    stops.push(addShortcut({ key: "?", label: () => "List these", run: second }));
    stops.push(listen(document));
    const event = pressed("1");
    expect([first.mock.calls.length, second.mock.calls.length, event.defaultPrevented]).toEqual([
      1,
      0,
      true,
    ]);
    expect(shortcuts.value.map((one) => [one.key, one.label()])).toEqual([
      ["1", "Open Control"],
      ["?", "List these"],
    ]);
  });

  it("leave alone a key typed in a field, a chord, a repeat and a key handled already", () => {
    const run = vi.fn();
    stops.push(addShortcut({ key: "1", label: () => "Open Control", run }));
    stops.push(listen(document));
    for (const html of [
      "<input>",
      "<textarea></textarea>",
      '<div role="combobox" tabindex="0"></div>',
    ]) {
      document.body.innerHTML = html;
      pressed("1", document.body.firstElementChild as HTMLElement);
    }
    for (const init of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }, { repeat: true }]) {
      pressed("1", document.body, init);
    }
    const handled = new KeyboardEvent("keydown", { key: "1", bubbles: true, cancelable: true });
    handled.preventDefault();
    expect(press(handled)).toBe(false);
    expect(run).not.toHaveBeenCalled();
    expect(pressed("2").defaultPrevented).toBe(false);
  });

  it("refuse a key that is a shortcut already, and take it again once it is free", () => {
    const stop = addShortcut({ key: "1", label: () => "Open Control", run: vi.fn() });
    expect(() => addShortcut({ key: "1", label: () => "Open Scan", run: vi.fn() })).toThrow(
      "the key 1 is a shortcut already",
    );
    stop();
    expect(shortcuts.value).toEqual([]);
    stops.push(addShortcut({ key: "1", label: () => "Open Scan", run: vi.fn() }));
    expect(shortcuts.value.map((one) => one.label())).toEqual(["Open Scan"]);
  });

  it("are heard until their listener stops", () => {
    const run = vi.fn();
    stops.push(addShortcut({ key: "1", label: () => "Open Control", run }));
    const stop = listen(document);
    pressed("1");
    stop();
    pressed("1");
    expect(run).toHaveBeenCalledTimes(1);
  });
});
