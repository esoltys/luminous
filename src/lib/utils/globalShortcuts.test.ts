import { describe, it, expect, afterEach } from "vitest";
import {
  isFallbackEscape,
  shouldSkipGlobalShortcut,
  visualizerStepForKey,
} from "./globalShortcuts";

describe("shouldSkipGlobalShortcut", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  /** Dispatch a keydown on `target` and report what the window-level check decides. */
  function skipped(target: HTMLElement, init: KeyboardEventInit = {}): boolean {
    let result: boolean | undefined;
    const listener = (e: KeyboardEvent) => (result = shouldSkipGlobalShortcut(e));
    window.addEventListener("keydown", listener);
    target.dispatchEvent(
      new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true, ...init }),
    );
    window.removeEventListener("keydown", listener);
    return result!;
  }

  it("handles an arrow key on a plain element", () => {
    const div = document.body.appendChild(document.createElement("div"));
    expect(skipped(div)).toBe(false);
  });

  it("leaves a key a focused control already handled", () => {
    const node = document.body.appendChild(document.createElement("div"));
    node.tabIndex = 0;
    node.addEventListener("keydown", (e) => e.preventDefault());
    expect(skipped(node)).toBe(true);
  });

  it("leaves keys typed into an editable field", () => {
    const input = document.body.appendChild(document.createElement("input"));
    expect(skipped(input)).toBe(true);
  });

  it("ignores auto-repeat", () => {
    const div = document.body.appendChild(document.createElement("div"));
    expect(skipped(div, { repeat: true })).toBe(true);
  });
});

describe("visualizerStepForKey (#2245)", () => {
  const step = (init: KeyboardEventInit) =>
    visualizerStepForKey(new KeyboardEvent("keydown", init));

  it("maps [ to the previous visualizer and ] to the next", () => {
    expect(step({ key: "[" })).toBe(-1);
    expect(step({ key: "]" })).toBe(1);
  });

  it("accepts AltGr (Ctrl+Alt), which types the brackets on some layouts", () => {
    expect(step({ key: "[", ctrlKey: true, altKey: true })).toBe(-1);
  });

  it("leaves Ctrl+[ / Ctrl+] (Back / Forward), Cmd and other keys alone", () => {
    expect(step({ key: "[", ctrlKey: true })).toBe(0);
    expect(step({ key: "]", metaKey: true })).toBe(0);
    expect(step({ key: "ArrowRight" })).toBe(0);
  });
});

describe("isFallbackEscape", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  function checkEscape(
    target: HTMLElement,
    init: KeyboardEventInit = {},
    prevent = false,
  ): boolean {
    let result: boolean | undefined;
    const listener = (e: KeyboardEvent) => {
      if (prevent) e.preventDefault();
      result = isFallbackEscape(e);
    };
    window.addEventListener("keydown", listener);
    target.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true, ...init }),
    );
    window.removeEventListener("keydown", listener);
    return result!;
  }

  it("accepts a plain unmodified Escape on a non-editable element", () => {
    const div = document.body.appendChild(document.createElement("div"));
    expect(checkEscape(div)).toBe(true);
  });

  it("rejects modified Escape, repeat, and non-Escape keys", () => {
    const div = document.body.appendChild(document.createElement("div"));
    expect(checkEscape(div, { ctrlKey: true })).toBe(false);
    expect(checkEscape(div, { metaKey: true })).toBe(false);
    expect(checkEscape(div, { altKey: true })).toBe(false);
    expect(checkEscape(div, { shiftKey: true })).toBe(false);
    expect(checkEscape(div, { repeat: true })).toBe(false);
    expect(checkEscape(div, { key: "Enter" })).toBe(false);
  });

  it("rejects Escape inside editable fields or when already defaultPrevented", () => {
    const input = document.body.appendChild(document.createElement("input"));
    expect(checkEscape(input)).toBe(false);

    const div = document.body.appendChild(document.createElement("div"));
    expect(checkEscape(div, {}, true)).toBe(false);
  });
});
