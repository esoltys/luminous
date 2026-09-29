// Fails when a @tauri-apps/* npm package and its matching Rust crate drift
// apart in major/minor version. Tauri build enforces that pairing and refuses
// to build when it is violated, but PR CI never runs `tauri build`, so a
// mismatch passes every check and only surfaces once a release tag is pushed (#1208).
//
// Usage: bun run scripts/check-tauri-versions.ts
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export interface Mismatch {
  npmPackage: string;
  npmVersion: string;
  crateName: string;
  crateVersion?: string;
  errorType: "version_mismatch" | "missing_crate";
}

export interface CheckedPair {
  npmPackage: string;
  npmVersion: string;
  crateName: string;
  crateVersion: string;
}

export interface CheckResult {
  checked: CheckedPair[];
  mismatches: Mismatch[];
}

/**
 * Maps an npm package name to its corresponding Rust crate name.
 * - `@tauri-apps/cli*` -> null (CLI has no crate counterpart)
 * - `@tauri-apps/api` -> "tauri"
 * - `@tauri-apps/plugin-<name>` -> "tauri-plugin-<name>"
 * - Any other package -> null
 */
export function npmToCrateName(npmPkg: string): string | null {
  if (npmPkg.startsWith("@tauri-apps/cli")) {
    return null;
  }
  if (npmPkg === "@tauri-apps/api") {
    return "tauri";
  }
  if (npmPkg.startsWith("@tauri-apps/plugin-")) {
    const pluginName = npmPkg.slice("@tauri-apps/plugin-".length);
    return `tauri-plugin-${pluginName}`;
  }
  return null;
}

/**
 * Extracts major and minor version numbers from a semver string.
 */
export function parseMajorMinor(version: string): { major: number; minor: number } | null {
  const match = version.match(/^(\d+)\.(\d+)/);
  if (!match) return null;
  return {
    major: parseInt(match[1], 10),
    minor: parseInt(match[2], 10),
  };
}

/**
 * Parses packages from bun.lock content.
 */
export function parseBunLock(content: string): Map<string, string> {
  const map = new Map<string, string>();
  try {
    // Strip trailing commas before closing braces/brackets to allow standard JSON.parse
    const cleaned = content.replace(/,(\s*[}\]])/g, "$1");
    const json = JSON.parse(cleaned);
    if (json && typeof json === "object" && json.packages && typeof json.packages === "object") {
      for (const [pkgName, val] of Object.entries(json.packages)) {
        if (Array.isArray(val) && typeof val[0] === "string") {
          const spec = val[0];
          const atIdx = spec.lastIndexOf("@");
          if (atIdx > 0) {
            map.set(pkgName, spec.slice(atIdx + 1));
          }
        } else if (typeof val === "string") {
          const atIdx = val.lastIndexOf("@");
          if (atIdx > 0) {
            map.set(pkgName, val.slice(atIdx + 1));
          }
        } else if (val && typeof val === "object" && typeof (val as Record<string, unknown>).version === "string") {
          map.set(pkgName, (val as Record<string, string>).version);
        }
      }
    }
  } catch {
    // Fallback regex parsing if JSON parsing fails
    const pkgRegex = /"(@tauri-apps\/[^"]+)":\s*\[\s*"[^"\n]*?@([0-9]+\.[0-9]+[^\s",]*)"/g;
    let match: RegExpExecArray | null;
    while ((match = pkgRegex.exec(content)) !== null) {
      map.set(match[1], match[2]);
    }
  }
  return map;
}

/**
 * Parses crates and their versions from Cargo.lock content.
 */
export function parseCargoLock(content: string): Map<string, string> {
  const map = new Map<string, string>();
  const packageBlocks = content.split(/^\[\[package\]\]/m);
  for (const block of packageBlocks) {
    const nameMatch = block.match(/^\s*name\s*=\s*"([^"]+)"/m);
    const versionMatch = block.match(/^\s*version\s*=\s*"([^"]+)"/m);
    if (nameMatch && versionMatch) {
      map.set(nameMatch[1], versionMatch[1]);
    }
  }
  return map;
}

/**
 * Compares resolved @tauri-apps npm packages against their paired Rust crates.
 */
export function checkTauriVersions(bunLockContent: string, cargoLockContent: string): CheckResult {
  const npmPackages = parseBunLock(bunLockContent);
  const cargoCrates = parseCargoLock(cargoLockContent);

  const checked: CheckedPair[] = [];
  const mismatches: Mismatch[] = [];

  for (const [npmPkg, npmVer] of npmPackages) {
    const crateName = npmToCrateName(npmPkg);
    if (!crateName) {
      continue;
    }

    const crateVer = cargoCrates.get(crateName);
    if (!crateVer) {
      mismatches.push({
        npmPackage: npmPkg,
        npmVersion: npmVer,
        crateName,
        errorType: "missing_crate",
      });
      continue;
    }

    const npmMM = parseMajorMinor(npmVer);
    const crateMM = parseMajorMinor(crateVer);

    if (!npmMM || !crateMM || npmMM.major !== crateMM.major || npmMM.minor !== crateMM.minor) {
      mismatches.push({
        npmPackage: npmPkg,
        npmVersion: npmVer,
        crateName,
        crateVersion: crateVer,
        errorType: "version_mismatch",
      });
    } else {
      checked.push({
        npmPackage: npmPkg,
        npmVersion: npmVer,
        crateName,
        crateVersion: crateVer,
      });
    }
  }

  // Sort checked items for consistent output
  checked.sort((a, b) => a.npmPackage.localeCompare(b.npmPackage));
  mismatches.sort((a, b) => a.npmPackage.localeCompare(b.npmPackage));

  return { checked, mismatches };
}

/**
 * Formats a user-friendly error message listing all mismatches and how to resolve them.
 */
export function formatMismatches(mismatches: Mismatch[]): string {
  const lines: string[] = [];
  lines.push(`✖ Found ${mismatches.length} Tauri version mismatch${mismatches.length === 1 ? "" : "es"} between bun.lock and Cargo.lock:`);
  lines.push("");

  for (const m of mismatches) {
    if (m.errorType === "missing_crate") {
      lines.push(`  • ${m.npmPackage} (${m.npmVersion}) has no matching '${m.crateName}' crate in Cargo.lock`);
      lines.push(`    Add the crate to src-tauri/Cargo.toml or remove the npm package.`);
    } else {
      lines.push(`  • ${m.npmPackage} (npm: ${m.npmVersion}) ↔ ${m.crateName} (crate: ${m.crateVersion})`);
      lines.push(`    Major/minor version mismatch. Tauri requires npm and crate versions to match in major.minor.`);
      lines.push(`    Fix options:`);
      lines.push(`      Update crate: cd src-tauri && cargo update -p ${m.crateName} --precise ${m.npmVersion}`);
      lines.push(`      Update npm:   bun add ${m.npmPackage}@^${m.crateVersion}`);
    }
    lines.push("");
  }

  return lines.join("\n");
}

export function main() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const bunLockPath = path.join(root, "bun.lock");
  const cargoLockPath = path.join(root, "Cargo.lock");

  if (!fs.existsSync(bunLockPath)) {
    console.error(`✖ Could not find bun.lock at ${bunLockPath}`);
    process.exit(1);
  }
  if (!fs.existsSync(cargoLockPath)) {
    console.error(`✖ Could not find Cargo.lock at ${cargoLockPath}`);
    process.exit(1);
  }

  const bunLockContent = fs.readFileSync(bunLockPath, "utf-8");
  const cargoLockContent = fs.readFileSync(cargoLockPath, "utf-8");

  const { checked, mismatches } = checkTauriVersions(bunLockContent, cargoLockContent);

  if (mismatches.length > 0) {
    console.error(formatMismatches(mismatches));
    process.exit(1);
  }

  console.log(`✔ Tauri version check passed: ${checked.length} paired package(s) match major.minor:`);
  for (const pair of checked) {
    console.log(`  • ${pair.npmPackage} (${pair.npmVersion}) ↔ ${pair.crateName} (${pair.crateVersion})`);
  }
}

const isMain =
  (typeof import.meta !== "undefined" && Boolean((import.meta as unknown as { main?: boolean }).main)) ||
  (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]));

if (isMain) {
  main();
}
