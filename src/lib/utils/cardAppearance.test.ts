import { describe, it, expect } from "vitest";
import { rowClasses } from "./cardAppearance";

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
