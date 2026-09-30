import { describe, it, expect, afterEach } from "vitest";
import { shouldSkipGlobalShortcut } from "./globalShortcuts";

describe("shouldSkipGlobalShortcut", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  /** Dispatch a keydown on `target` and report what the window-level check decides. */
  function skipped(target: HTMLElement, init: KeyboardEventInit = {}): boolean {
    let result: boolean | undefined;
    const listener = (e: KeyboardEvent) => (result = shouldSkipGlobalShortcut(e));
    window.addEventListener("keydown", listener);
    target.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true, cancelable: true, ...init }));
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
