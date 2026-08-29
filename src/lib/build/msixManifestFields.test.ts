// @vitest-environment node
//
// Regression tests for a cluster of MSIX/Microsoft Store packaging
// regressions found during Store submission review (#418, #468/#469,
// #471/#472). Each was a static config field that got left out or
// referenced a file that didn't exist, and none of them fail a normal
// `cargo build`/`bun run build` — they only surface when `winget`/Partner
// Center actually validates or renders the packaged MSIX. Pinning the
// static config directly is the fast, CI-friendly way to catch a
// regression here instead of only finding out during a Store submission.
import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const genWindowsDir = path.resolve(__dirname, "../../../src-tauri/gen/windows");

describe("MSIX bundle.config.json declares required capabilities/assets (#418, #468/#469)", () => {
  const config = JSON.parse(
    fs.readFileSync(path.join(genWindowsDir, "bundle.config.json"), "utf-8")
  );

  it("declares broadFileSystemAccess as a restricted capability", () => {
    // #418: without this, the packaged app can't read/write outside its
    // MSIX sandbox — e.g. the user's actual music library folders — and
    // every file operation outside the container silently fails.
    expect(config.capabilities?.restricted).toContain("broadFileSystemAccess");
  });

  it("enables resource indexing so scaled/unplated tile assets are actually picked up", () => {
    // #468/#469: without resourceIndex.enabled, the default-black square
    // tile ships instead of the real app icon because Windows never
    // indexes the variant assets to choose from.
    expect(config.resourceIndex?.enabled).toBe(true);
  });

  it("generates scale/targetSize/unplated/lightUnplated tile variants", () => {
    const variants = config.assets?.variants ?? {};
    for (const key of ["scale", "targetSize", "unplated", "lightUnplated"]) {
      expect(variants[key], `assets.variants.${key} must be enabled`).toBe(true);
    }
  });
});

describe("MSIX AppxManifest file type associations (#471/#472)", () => {
  const manifestPath = path.join(genWindowsDir, "AppxManifest.xml.template");
  const manifest = fs.readFileSync(manifestPath, "utf-8");

  // Split on each uap:FileTypeAssociation open tag to get one block per
  // association (crude but sufficient — this is a generated, well-formed
  // template, not arbitrary untrusted XML).
  const blocks = manifest
    .split(/(?=<uap:FileTypeAssociation )/)
    .filter(b => b.startsWith("<uap:FileTypeAssociation "));

  it("declares at least one file type association", () => {
    expect(blocks.length).toBeGreaterThan(0);
  });

  it.each(blocks.map(b => [b.match(/Name="([^"]+)"/)?.[1] ?? "?", b] as const))(
    "association %s has a non-empty DisplayName and an on-disk Logo",
    (_name, block) => {
      const displayNameMatch = block.match(/<uap:DisplayName>([^<]*)<\/uap:DisplayName>/);
      expect(displayNameMatch, `${_name}: missing <uap:DisplayName>`).toBeTruthy();
      expect(
        displayNameMatch![1].trim().length,
        `${_name}: DisplayName must not be blank`
      ).toBeGreaterThan(0);

      const logoMatch = block.match(/<uap:Logo>([^<]*)<\/uap:Logo>/);
      expect(logoMatch, `${_name}: missing <uap:Logo>`).toBeTruthy();

      // Manifest paths use Windows backslashes and are relative to
      // src-tauri/ (two levels up from gen/windows/, the manifest's own
      // asset root).
      const relPath = logoMatch![1].trim().replace(/\\/g, path.sep);
      const absPath = path.resolve(genWindowsDir, "..", "..", relPath);
      expect(
        fs.existsSync(absPath),
        `${_name}: Logo file does not exist on disk: ${absPath}`
      ).toBe(true);
    }
  );
});
