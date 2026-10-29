// scripts/guide-manifest.ts
// Contract and validation engine for user-guide screenshot assets.
//
// External capture tools consume docs/user-guide/assets/manifest.json to know what
// images to produce per language, target paths, and expected pixel dimensions.
// Both `sync-docs.ts` and the test suite call validateGuideAssets() to guarantee
// complete parity between the guide HTML, manifest, and disk files.
import * as fs from "node:fs";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, "..");
export const USER_GUIDE_DIR = path.join(REPO_ROOT, "docs", "user-guide");
export const DEFAULT_MANIFEST_PATH = path.join(USER_GUIDE_DIR, "assets", "manifest.json");

const GUIDE_FILE_REGEX = /^luminous-user-guide-([A-Z]{2})\.html$/;
const IMG_TAG_REGEX = /<img\b([^>]*)>/g;
const SRC_ATTR_REGEX = /\bsrc="([^"]+)"/;
const WIDTH_ATTR_REGEX = /\bwidth="(\d+)"/;
const HEIGHT_ATTR_REGEX = /\bheight="(\d+)"/;

const MANUAL_LANGUAGE_LOCALES: Record<string, string> = {
  EN: "en-CA",
  FR: "fr-CA",
  DE: "de",
  ES: "es",
  IT: "it",
  RU: "ru",
  UK: "uk",
};

export interface GuideImageEntry {
  /** Relative path from docs/user-guide/, matching HTML src. Normalized to POSIX slashes. */
  readonly src: string;
  /** Image identifier without directory or extension (e.g. "home-welcome"). */
  readonly name: string;
  /** Intrinsic pixel width. */
  readonly width: number;
  /** Intrinsic pixel height. */
  readonly height: number;
}

interface GuideLanguageManifest {
  /** Asset subdirectory under assets/ (e.g. "en-CA", "de"). */
  readonly locale: string;
  /** File name of the guide HTML (e.g. "luminous-user-guide-EN.html"). */
  readonly guide: string;
  /** Ordered list of screenshot assets referenced in this manual. */
  readonly images: GuideImageEntry[];
}

export interface GuideManifest {
  readonly version: number;
  readonly languages: Record<string, GuideLanguageManifest>;
}

type ValidationIssueCode =
  | "MISSING_IN_MANIFEST"
  | "EXTRA_IN_MANIFEST"
  | "MISSING_ON_DISK"
  | "DIMENSION_MISMATCH"
  | "HTML_DIMENSION_MISMATCH";

export interface ValidationIssue {
  readonly code: ValidationIssueCode;
  readonly message: string;
  readonly file: string;
  readonly expected?: { width: number; height: number };
  readonly actual?: { width: number; height: number };
}

export interface ValidationReport {
  readonly valid: boolean;
  readonly issues: ValidationIssue[];
}

export interface ValidationOptions {
  guideDir?: string;
  manifestPath?: string;
  checkDiskFiles?: boolean;
  checkDiskDimensions?: boolean;
  checkHtmlDimensions?: boolean;
}

function normalizePath(p: string): string {
  return p.split(path.sep).join("/");
}

/**
 * Reads intrinsic pixel dimensions of PNG and WebP files from binary headers without external dependencies.
 */
function readDimensionsFromBuffer(buf: Buffer): { width: number; height: number } | null {
  // PNG: signature 0x89 50 4E 47 0D 0A 1A 0A, followed by IHDR chunk
  if (
    buf.length >= 24 &&
    buf[0] === 0x89 &&
    buf[1] === 0x50 &&
    buf[2] === 0x4e &&
    buf[3] === 0x47 &&
    buf.toString("ascii", 12, 16) === "IHDR"
  ) {
    return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
  }

  // WebP: RIFF ... WEBP
  if (
    buf.length >= 30 &&
    buf.toString("ascii", 0, 4) === "RIFF" &&
    buf.toString("ascii", 8, 12) === "WEBP"
  ) {
    const chunk = buf.toString("ascii", 12, 16);
    if (chunk === "VP8 ") {
      // Lossy VP8: 14-bit width and height at bytes 26-29
      return {
        width: buf.readUInt16LE(26) & 0x3fff,
        height: buf.readUInt16LE(28) & 0x3fff,
      };
    }
    if (chunk === "VP8L") {
      // Lossless VP8L: 14-bit width and height packed across bytes 21-24
      const b0 = buf[21];
      const b1 = buf[22];
      const b2 = buf[23];
      const b3 = buf[24];
      return {
        width: 1 + (((b1 & 0x3f) << 8) | b0),
        height: 1 + (((b3 & 0x0f) << 10) | (b2 << 2) | ((b1 & 0xc0) >> 6)),
      };
    }
    if (chunk === "VP8X") {
      // Extended VP8X: 24-bit canvas width and height - 1 at bytes 24-29
      return {
        width: 1 + buf.readUIntLE(24, 3),
        height: 1 + buf.readUIntLE(27, 3),
      };
    }
  }

  return null;
}

/**
 * Reads intrinsic dimensions from a file on disk. Returns null if unreadable or not found.
 */
export function readImageDimensions(filePath: string): { width: number; height: number } | null {
  if (!fs.existsSync(filePath)) return null;
  try {
    const buf = fs.readFileSync(filePath);
    return readDimensionsFromBuffer(buf);
  } catch {
    return null;
  }
}

/**
 * Extracts all screenshot image references and declared HTML dimensions from guide HTML files.
 */
export function extractHtmlImageReferences(guideDir: string = USER_GUIDE_DIR): Record<string, GuideImageEntry[]> {
  const result: Record<string, GuideImageEntry[]> = {};
  const files = fs.readdirSync(guideDir).filter((f) => GUIDE_FILE_REGEX.test(f)).sort();

  for (const file of files) {
    const langMatch = file.match(GUIDE_FILE_REGEX);
    if (!langMatch) continue;
    const lang = langMatch[1];
    const html = fs.readFileSync(path.join(guideDir, file), "utf8");
    const entries: GuideImageEntry[] = [];

    for (const match of html.matchAll(IMG_TAG_REGEX)) {
      const tagContent = match[1];
      const srcMatch = tagContent.match(SRC_ATTR_REGEX);
      if (!srcMatch) continue;
      const src = srcMatch[1];
      // Only process guide screenshot assets under assets/
      if (!src.startsWith("assets/")) continue;

      const widthMatch = tagContent.match(WIDTH_ATTR_REGEX);
      const heightMatch = tagContent.match(HEIGHT_ATTR_REGEX);
      const width = widthMatch ? parseInt(widthMatch[1], 10) : 0;
      const height = heightMatch ? parseInt(heightMatch[1], 10) : 0;
      const name = path.basename(src, path.extname(src));

      entries.push({
        src: normalizePath(src),
        name,
        width,
        height,
      });
    }

    result[lang] = entries;
  }

  return result;
}

/**
 * Loads and parses the manifest JSON.
 */
export function loadManifest(manifestPath: string = DEFAULT_MANIFEST_PATH): GuideManifest {
  if (!fs.existsSync(manifestPath)) {
    throw new Error(`User guide image manifest not found at: ${manifestPath}`);
  }
  const content = fs.readFileSync(manifestPath, "utf8");
  return JSON.parse(content) as GuideManifest;
}

/**
 * Builds the canonical manifest from the actual images on disk for each guide language.
 */
export function generateManifest(guideDir: string = USER_GUIDE_DIR): GuideManifest {
  const htmlRefs = extractHtmlImageReferences(guideDir);
  const languages: Record<string, GuideLanguageManifest> = {};

  for (const [lang, refs] of Object.entries(htmlRefs)) {
    const locale = MANUAL_LANGUAGE_LOCALES[lang] || lang.toLowerCase();
    const guide = `luminous-user-guide-${lang}.html`;
    const images: GuideImageEntry[] = [];

    for (const ref of refs) {
      // Determine the expected dimensions from the actual file on disk
      const filePath = path.join(guideDir, ref.src);
      const dims = readImageDimensions(filePath);
      if (!dims) {
        throw new Error(`Cannot read dimensions for asset: ${filePath}`);
      }

      images.push({
        src: ref.src,
        name: ref.name,
        width: dims.width,
        height: dims.height,
      });
    }

    languages[lang] = {
      locale,
      guide,
      images,
    };
  }

  return {
    version: 1,
    languages,
  };
}

/**
 * Writes the manifest JSON to disk with standard indentation.
 */
export function writeManifest(
  manifest: GuideManifest,
  manifestPath: string = DEFAULT_MANIFEST_PATH
): void {
  fs.mkdirSync(path.dirname(manifestPath), { recursive: true });
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n", "utf8");
}

/**
 * Validates guide HTML files, manifest, and disk assets for complete parity.
 */
export function validateGuideAssets(options: ValidationOptions = {}): ValidationReport {
  const guideDir = options.guideDir ?? USER_GUIDE_DIR;
  const manifestPath = options.manifestPath ?? DEFAULT_MANIFEST_PATH;
  const checkDiskFiles = options.checkDiskFiles ?? true;
  const checkDiskDimensions = options.checkDiskDimensions ?? true;
  const checkHtmlDimensions = options.checkHtmlDimensions ?? true;

  const issues: ValidationIssue[] = [];

  let manifest: GuideManifest;
  try {
    manifest = loadManifest(manifestPath);
  } catch (err) {
    return {
      valid: false,
      issues: [
        {
          code: "MISSING_IN_MANIFEST",
          message: err instanceof Error ? err.message : String(err),
          file: manifestPath,
        },
      ],
    };
  }

  const htmlRefs = extractHtmlImageReferences(guideDir);

  // 1. Cross-check manifest languages with HTML guide languages
  const manifestLangs = Object.keys(manifest.languages).sort();
  const htmlLangs = Object.keys(htmlRefs).sort();

  for (const lang of htmlLangs) {
    if (!manifest.languages[lang]) {
      issues.push({
        code: "MISSING_IN_MANIFEST",
        message: `Language "${lang}" exists in guide HTML files but is missing from manifest.json.`,
        file: manifestPath,
      });
    }
  }

  for (const lang of manifestLangs) {
    if (!htmlRefs[lang]) {
      issues.push({
        code: "EXTRA_IN_MANIFEST",
        message: `Language "${lang}" is declared in manifest.json but has no corresponding luminous-user-guide-${lang}.html.`,
        file: manifestPath,
      });
    }
  }

  // 2. Check per-language images between HTML, manifest, and disk
  for (const lang of htmlLangs) {
    const langManifest = manifest.languages[lang];
    if (!langManifest) continue;

    const htmlList = htmlRefs[lang];
    const manifestMap = new Map<string, GuideImageEntry>();
    for (const img of langManifest.images) {
      manifestMap.set(img.src, img);
    }

    // Check HTML -> Manifest
    for (const htmlImg of htmlList) {
      const manifestImg = manifestMap.get(htmlImg.src);
      if (!manifestImg) {
        issues.push({
          code: "MISSING_IN_MANIFEST",
          message: `Image "${htmlImg.src}" is referenced in ${langManifest.guide} but missing from manifest.json.`,
          file: htmlImg.src,
        });
        continue;
      }

      if (checkHtmlDimensions) {
        if (htmlImg.width !== manifestImg.width || htmlImg.height !== manifestImg.height) {
          issues.push({
            code: "HTML_DIMENSION_MISMATCH",
            message: `HTML width/height for "${htmlImg.src}" in ${langManifest.guide} (${htmlImg.width}x${htmlImg.height}) does not match manifest (${manifestImg.width}x${manifestImg.height}).`,
            file: langManifest.guide,
            expected: { width: manifestImg.width, height: manifestImg.height },
            actual: { width: htmlImg.width, height: htmlImg.height },
          });
        }
      }
    }

    // Check Manifest -> HTML
    const htmlSrcSet = new Set(htmlList.map((i) => i.src));
    for (const manifestImg of langManifest.images) {
      if (!htmlSrcSet.has(manifestImg.src)) {
        issues.push({
          code: "EXTRA_IN_MANIFEST",
          message: `Manifest declares image "${manifestImg.src}" for language "${lang}", but it is not referenced in ${langManifest.guide}.`,
          file: manifestImg.src,
        });
      }
    }

    // Check disk assets (dark and light, WebP and PNG)
    if (checkDiskFiles || checkDiskDimensions) {
      for (const manifestImg of langManifest.images) {
        const variants = [
          manifestImg.src, // dark WebP
          manifestImg.src.replace("/dark/", "/light/"), // light WebP
          manifestImg.src.replace(/\.webp$/, ".png"), // dark PNG
          manifestImg.src.replace("/dark/", "/light/").replace(/\.webp$/, ".png"), // light PNG
        ];

        for (const variant of variants) {
          const diskPath = path.join(guideDir, variant);
          if (checkDiskFiles && !fs.existsSync(diskPath)) {
            issues.push({
              code: "MISSING_ON_DISK",
              message: `Required asset file does not exist on disk: ${variant}`,
              file: variant,
            });
            continue;
          }

          if (checkDiskDimensions && fs.existsSync(diskPath)) {
            const dims = readImageDimensions(diskPath);
            if (!dims) {
              issues.push({
                code: "DIMENSION_MISMATCH",
                message: `Failed to read image dimensions from: ${variant}`,
                file: variant,
              });
            } else if (dims.width !== manifestImg.width || dims.height !== manifestImg.height) {
              issues.push({
                code: "DIMENSION_MISMATCH",
                message: `Image dimensions for ${variant} (${dims.width}x${dims.height}) do not match manifest (${manifestImg.width}x${manifestImg.height}).`,
                file: variant,
                expected: { width: manifestImg.width, height: manifestImg.height },
                actual: dims,
              });
            }
          }
        }
      }
    }
  }

  return {
    valid: issues.length === 0,
    issues,
  };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.includes("--generate")) {
    const manifest = generateManifest();
    writeManifest(manifest);
    console.log(`[guide-manifest] Generated ${DEFAULT_MANIFEST_PATH}`);
  } else {
    const report = validateGuideAssets();
    if (!report.valid) {
      for (const issue of report.issues) {
        console.error(`[guide-manifest] Error [${issue.code}]: ${issue.message}`);
      }
      process.exit(1);
    }
    console.log("[guide-manifest] All guide assets and manifest verified successfully.");
  }
}

