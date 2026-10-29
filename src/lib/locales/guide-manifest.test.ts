// @vitest-environment node
import { describe, expect, it } from "vitest";
import * as fs from "node:fs";
import * as path from "node:path";
import { MANUAL_LANGUAGES } from "./index";
import {
  extractHtmlImageReferences,
  loadManifest,
  generateManifest,
  writeManifest,
  validateGuideAssets,
  USER_GUIDE_DIR,
  DEFAULT_MANIFEST_PATH,
  readImageDimensions,
  type ValidationIssue,
} from "../../../scripts/guide-manifest";

describe("user guide image manifest & validation", () => {
  const manifest = loadManifest(DEFAULT_MANIFEST_PATH);
  const htmlRefs = extractHtmlImageReferences(USER_GUIDE_DIR);

  it("passes full validation across HTML files, manifest, and disk assets", () => {
    const report = validateGuideAssets({
      guideDir: USER_GUIDE_DIR,
      manifestPath: DEFAULT_MANIFEST_PATH,
      checkDiskFiles: true,
      checkDiskDimensions: true,
      checkHtmlDimensions: true,
    });

    if (!report.valid) {
      const messages = report.issues.map((i: ValidationIssue) => `[${i.code}] ${i.message}`).join("\n");
      expect.unreachable(`Guide validation failed with ${report.issues.length} issue(s):\n${messages}`);
    }
    expect(report.valid).toBe(true);
    expect(report.issues).toEqual([]);
  });

  it("generates a canonical manifest that matches the committed manifest.json exactly", () => {
    const generated = generateManifest(USER_GUIDE_DIR);
    expect(generated).toEqual(manifest);

    // Verify writeManifest round-trip
    const tmpPath = path.join(USER_GUIDE_DIR, "assets", "manifest.roundtrip.tmp.json");
    try {
      writeManifest(generated, tmpPath);
      expect(fs.existsSync(tmpPath)).toBe(true);
      const reloaded = loadManifest(tmpPath);
      expect(reloaded).toEqual(manifest);
    } finally {
      if (fs.existsSync(tmpPath)) fs.unlinkSync(tmpPath);
    }
  });

  it("declares every shipped manual language in manifest.json", () => {
    const manifestLangs = Object.keys(manifest.languages).sort();
    const expectedLangs = [...MANUAL_LANGUAGES].sort();
    expect(manifestLangs).toEqual(expectedLangs);
  });

  it("maintains the required 29 screenshots per language with positive dimensions", () => {
    for (const lang of MANUAL_LANGUAGES) {
      const langManifest = manifest.languages[lang];
      expect(langManifest, `Manifest missing language ${lang}`).toBeDefined();
      expect(langManifest.images.length).toBe(29);

      for (const img of langManifest.images) {
        expect(img.width).toBeGreaterThan(0);
        expect(img.height).toBeGreaterThan(0);
        expect(img.src).toMatch(new RegExp(`^assets/${langManifest.locale}/screenshots/dark/[a-z0-9-]+\\.webp$`));
      }
    }
  });

  it("guarantees HTML attributes match disk dimensions and manifest entries", () => {
    for (const lang of MANUAL_LANGUAGES) {
      const refs = htmlRefs[lang];
      const langManifest = manifest.languages[lang];
      expect(refs.length).toBe(langManifest.images.length);

      for (let i = 0; i < refs.length; i++) {
        const htmlRef = refs[i];
        const manifestEntry = langManifest.images[i];
        expect(htmlRef.src).toBe(manifestEntry.src);
        expect(htmlRef.width).toBe(manifestEntry.width);
        expect(htmlRef.height).toBe(manifestEntry.height);

        // Check on-disk file directly
        const diskPath = path.join(USER_GUIDE_DIR, manifestEntry.src);
        const diskDims = readImageDimensions(diskPath);
        expect(diskDims).not.toBeNull();
        expect(diskDims?.width).toBe(manifestEntry.width);
        expect(diskDims?.height).toBe(manifestEntry.height);
      }
    }
  });

  it("detects missing files or altered dimensions during validation", () => {
    // Test that the validator correctly detects dimension deviations
    const testDir = path.join(USER_GUIDE_DIR, "assets");
    const sampleImage = path.join(testDir, "en-CA", "screenshots", "dark", "home.webp");
    expect(fs.existsSync(sampleImage)).toBe(true);

    const syntheticManifest = JSON.parse(JSON.stringify(manifest));
    syntheticManifest.languages.EN.images[0].width = 9999;

    const tmpManifestPath = path.join(testDir, "manifest.test.tmp.json");
    try {
      fs.writeFileSync(tmpManifestPath, JSON.stringify(syntheticManifest));
      const report = validateGuideAssets({
        guideDir: USER_GUIDE_DIR,
        manifestPath: tmpManifestPath,
        checkDiskFiles: true,
        checkDiskDimensions: true,
        checkHtmlDimensions: true,
      });

      expect(report.valid).toBe(false);
      const dimensionMismatch = report.issues.find((i: ValidationIssue) => i.code === "DIMENSION_MISMATCH" || i.code === "HTML_DIMENSION_MISMATCH");
      expect(dimensionMismatch).toBeDefined();
    } finally {
      if (fs.existsSync(tmpManifestPath)) {
        fs.unlinkSync(tmpManifestPath);
      }
    }
  });
});
