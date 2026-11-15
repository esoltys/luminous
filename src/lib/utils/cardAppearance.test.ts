import { describe, it, expect } from "vitest";
import { rowClasses, HOVER_ZOOM, HOVER_ZOOM_STACKED } from "./cardAppearance";

describe("rowClasses", () => {
  it("draws a filled, bordered, rounded box when boxed", () => {
    const classes = rowClasses("boxed").split(" ");
    expect(classes).toContain("bg-brand-sidebar");
    expect(classes).toContain("border");
    expect(classes).toContain("rounded-lg");
  });

  it("drops the box for a hairline divider and a hover tint when plain", () => {
    const classes = rowClasses("plain").split(" ");
    expect(classes).not.toContain("bg-brand-sidebar");
    expect(classes).not.toContain("rounded-lg");
    expect(classes).toContain("border-b");
    expect(classes).toContain("hover:bg-brand-sidebar");
  });

  it("keeps a keyboard-focus outline when plain, so focus never goes invisible", () => {
    expect(rowClasses("plain")).toContain("focus-visible:outline-brand-accent");
  });
});

describe("hover zoom", () => {
  it("uses the same scale and duration for plain and stacked covers", () => {
    for (const c of [HOVER_ZOOM, HOVER_ZOOM_STACKED]) {
      expect(c).toContain("group-hover:scale-105");
      expect(c).toContain("duration-300");
    }
  });

  it("transitions the scale property when a stack also transitions transform and opacity", () => {
    expect(HOVER_ZOOM_STACKED).toContain("transition-[transform,scale,opacity]");
  });
});
