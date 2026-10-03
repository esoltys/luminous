import { describe, it, expect } from "vitest";
import source from "./PlayerBar.svelte?raw";

describe("PlayerBar add-on overlay mount", () => {
  it("mounts the generic overlay without naming any add-on", () => {
    expect(source).toContain("<AddonOverlay");
    expect(source).not.toMatch(/mothman/i);
    expect(source).not.toMatch(/activeThemeId\s*===/);
  });
});
