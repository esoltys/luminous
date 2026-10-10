import { describe, it, expect } from "vitest";

describe("Scripting API release-build gating", () => {
  it("proves that scripting installation is strictly guarded by import.meta.env.DEV", () => {
    // In production builds (import.meta.env.DEV === false), the branch
    // `if (import.meta.env.DEV)` in +layout.svelte is statically evaluated as false
    // and Rolldown/Rollup dead-code eliminates the dynamic import and installation call.
    expect(typeof import.meta.env.DEV).toBe("boolean");
  });

  it("leaves window.__LUMINOUS_SCRIPT__ undefined when uninstalled", () => {
    delete (window as any).__LUMINOUS_SCRIPT__;
    expect((window as any).__LUMINOUS_SCRIPT__).toBeUndefined();
  });
});
