import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, existsSync } from "node:fs";
import path from "node:path";

describe("Scripting API release-build gating", () => {
  it("proves that the release bundle contains no scripting API symbols", () => {
    const buildDir = path.resolve(__dirname, "../../../build");
    const isStrict =
      process.env.STRICT_RELEASE_GATING === "true" ||
      process.env.GITHUB_WORKFLOW === "Release Build";

    if (!existsSync(buildDir)) {
      if (isStrict) {
        throw new Error(
          `Release build directory "${buildDir}" was not found. Run "bun run build" before running this release verification.`
        );
      }
      return;
    }

    function collectJsFiles(dir: string): string[] {
      const entries = readdirSync(dir, { withFileTypes: true });
      const files: string[] = [];
      for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory()) {
          files.push(...collectJsFiles(fullPath));
        } else if (entry.isFile() && entry.name.endsWith(".js")) {
          files.push(fullPath);
        }
      }
      return files;
    }

    const jsFiles = collectJsFiles(buildDir);
    expect(jsFiles.length).toBeGreaterThan(0);

    const leakingFiles: string[] = [];
    const marker = "__LUMINOUS_SCRIPT__";

    for (const file of jsFiles) {
      const content = readFileSync(file, "utf8");
      if (content.includes(marker)) {
        leakingFiles.push(path.relative(buildDir, file));
      }
    }

    expect(
      leakingFiles,
      `Found scripting marker "${marker}" in production release bundle chunks: ${leakingFiles.join(", ")}`
    ).toEqual([]);
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
