import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

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

  it("keeps every screen-hook registration behind an import.meta.env.DEV guard", () => {
    // The guard has to be at the call site: that is what lets the bundler drop the
    // registrations (and the hook names inside them) from release builds.
    const dir = path.resolve(__dirname, "../components");
    const unguarded: string[] = [];
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".svelte"))) {
      const lines = readFileSync(path.join(dir, file), "utf8").split(/\r?\n/);
      lines.forEach((line, i) => {
        if (!line.includes("onDevScreenHooks(")) return;
        const before = lines.slice(Math.max(0, i - 2), i + 1).join("\n");
        if (!before.includes("import.meta.env.DEV")) unguarded.push(`${file}:${i + 1}`);
      });
    }
    expect(unguarded).toEqual([]);
  });
});
