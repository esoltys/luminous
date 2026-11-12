import { afterEach, describe, expect, it } from "vitest";
import { publishScrollbarWidth } from "./scrollbarWidth";

describe("publishScrollbarWidth", () => {
  afterEach(() => {
    document.documentElement.style.removeProperty("--scrollbar-width");
  });

  it("publishes the probe's scrollbar width as a CSS variable and cleans the probe up", () => {
    const before = document.body.childElementCount;
    publishScrollbarWidth();
    // jsdom does no layout, so the measured width is 0; the contract is "a px length is set".
    expect(document.documentElement.style.getPropertyValue("--scrollbar-width")).toBe("0px");
    expect(document.body.childElementCount).toBe(before);
  });
});
