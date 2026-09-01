// Regenerates every piece of flatpak/ (and the metainfo release history) that
// derives from the rest of the repo, so it never has to be done by hand — see
// #476. Wired into scripts/release.ts as a step before the release commit;
// also runnable standalone, and checkable in CI via --check.
//
// Usage:
//   bun run scripts/sync-flatpak.ts            # regenerate flatpak/ + metainfo in place
//   bun run scripts/sync-flatpak.ts --check     # regenerate into a temp dir, diff, exit non-zero on drift
//
// Requires `uv` (https://docs.astral.sh/uv/) on PATH: it runs the vendored
// flatpak-cargo-generator.py (scripts/vendor/) via `uv run`, and
// flatpak-node-generator via `uvx --from git+...` pinned to a specific commit
// of flatpak/flatpak-builder-tools below — neither generator is vendored as
// an npm/pip dependency of this repo, so pinning the exact source commit is
// what keeps regeneration reproducible.
import { execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");
const flatpakDir = path.join(rootDir, "flatpak");
const metainfoPath = path.join(rootDir, "src-tauri", "icons", "org.luminous.music.metainfo.xml");

const CARGO_GENERATOR_SCRIPT = path.join(rootDir, "scripts", "vendor", "flatpak-cargo-generator.py");
// Pin: https://github.com/flatpak/flatpak-builder-tools/commits/main/node
const NODE_GENERATOR_REF = "1fc32195e3e60fe5c97f0af646dec7a99df5962b";
const NODE_GENERATOR_CMD = `uvx --from "git+https://github.com/flatpak/flatpak-builder-tools@${NODE_GENERATOR_REF}#subdirectory=node" flatpak-node-generator`;

const GENERATED_FILES = ["package.json", "package-lock.json", "node-sources.json", "cargo-sources.json"];

function normalizeLineEndings(filePath: string) {
  const content = fs.readFileSync(filePath, "utf-8");
  fs.writeFileSync(filePath, content.replace(/\r\n/g, "\n"));
}

// flatpak-node-generator builds its "dest" paths (and shell commands
// referencing them) with the host OS's path separator. Run on Windows (this
// repo's primary dev machine — see scripts/release.ts), that means literal
// backslashes, which don't match what Linux CI/flatpak-builder expect and
// wouldn't match what's already committed. Once parsed out of JSON, any
// backslash remaining in a string is a real path separator, never escaped
// JSON content (registry URLs/hashes are forward-slash/base64, and tabs or
// quotes are already resolved to their own characters by JSON.parse) — so a
// blanket string replace is safe.
function normalizePathSeparators(filePath: string) {
  const data = JSON.parse(fs.readFileSync(filePath, "utf-8"));
  const walk = (value: unknown): unknown => {
    if (typeof value === "string") return value.replace(/\\/g, "/");
    if (Array.isArray(value)) return value.map(walk);
    if (value && typeof value === "object") {
      return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, walk(v)]));
    }
    return value;
  };
  fs.writeFileSync(filePath, JSON.stringify(walk(data), null, 4));
}

// npm annotates optional platform packages in package-lock.json with a
// "libc" constraint (in addition to the "os"/"cpu" fields), but only when
// resolution runs on Linux — the same `npm install --package-lock-only`
// run on Windows omits it entirely. It doesn't affect what
// flatpak-node-generator actually vendors (node-sources.json is identical
// either way), so strip it for output that's stable regardless of which
// platform ran the sync.
function normalizeLockfileLibcNoise(filePath: string) {
  const data = JSON.parse(fs.readFileSync(filePath, "utf-8"));
  const strip = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(strip);
    if (value && typeof value === "object") {
      return Object.fromEntries(
        Object.entries(value)
          .filter(([k]) => k !== "libc")
          .map(([k, v]) => [k, strip(v)]),
      );
    }
    return value;
  };
  fs.writeFileSync(filePath, JSON.stringify(strip(data), null, 2) + "\n");
}

function currentVersion(): string {
  const pkg = JSON.parse(fs.readFileSync(path.join(rootDir, "package.json"), "utf-8"));
  return pkg.version;
}

// flatpak/package.json + package-lock.json are an npm-shaped snapshot of the
// real package.json, kept solely so flatpak-node-generator (which doesn't
// understand bun.lock) can vendor JS deps for an offline build. `playwright`
// is dropped so the generator doesn't try to vendor a Chromium download —
// browser-driven tests aren't run as part of building the app. See the
// comment above build-commands in flatpak/io.github.esoltys.Luminous.yml.
function syncNodeSnapshot(targetDir: string) {
  const pkg = JSON.parse(fs.readFileSync(path.join(rootDir, "package.json"), "utf-8"));
  delete pkg.devDependencies?.playwright;
  fs.writeFileSync(path.join(targetDir, "package.json"), JSON.stringify(pkg, null, 2) + "\n");

  // Seed with the already-committed lockfile (when regenerating into a
  // scratch dir, e.g. --check) so npm resolves incrementally against it
  // instead of re-resolving every caret range against whatever's newest on
  // the registry right now. Without this, --check flags drift whenever an
  // unrelated transitive dependency has published a new patch since the
  // last real sync — a moving target, not an actual repo change.
  const committedLockfile = path.join(flatpakDir, "package-lock.json");
  if (targetDir !== flatpakDir && fs.existsSync(committedLockfile)) {
    fs.copyFileSync(committedLockfile, path.join(targetDir, "package-lock.json"));
  }

  console.log("[sync-flatpak] Resolving flatpak/package-lock.json (npm, lockfile-only)...");
  // --ignore-scripts: without it, npm still runs this snapshot's "prepare"
  // lifecycle script (svelte-kit sync) even with --package-lock-only,
  // littering flatpak/ with a .svelte-kit/ directory that isn't part of
  // the snapshot's purpose (resolving a lockfile, not building anything).
  execSync("npm install --package-lock-only --ignore-scripts --legacy-peer-deps", { cwd: targetDir, stdio: "inherit" });
  normalizeLockfileLibcNoise(path.join(targetDir, "package-lock.json"));
  normalizeLineEndings(path.join(targetDir, "package-lock.json"));
}

function generateNodeSources(targetDir: string) {
  console.log("[sync-flatpak] Generating node-sources.json...");
  const lockfile = path.join(targetDir, "package-lock.json");
  const output = path.join(targetDir, "node-sources.json");
  execSync(`${NODE_GENERATOR_CMD} npm "${lockfile}" -o "${output}"`, { cwd: rootDir, stdio: "inherit" });
  normalizePathSeparators(output);
}

function generateCargoSources(targetDir: string) {
  console.log("[sync-flatpak] Generating cargo-sources.json...");
  // Root Cargo.lock, not src-tauri/Cargo.lock: this is a Cargo workspace, so
  // the root lockfile is the one Cargo actually resolves against (see #475).
  const lockfile = path.join(rootDir, "Cargo.lock");
  const output = path.join(targetDir, "cargo-sources.json");
  execSync(`uv run "${CARGO_GENERATOR_SCRIPT}" "${lockfile}" -o "${output}"`, { cwd: rootDir, stdio: "inherit" });
  normalizeLineEndings(output);
}

function regenerateInto(targetDir: string) {
  fs.mkdirSync(targetDir, { recursive: true });
  syncNodeSnapshot(targetDir);
  generateNodeSources(targetDir);
  generateCargoSources(targetDir);
}

// Idempotent: no-ops if this version already has a <release> entry.
function syncMetainfoRelease(version: string, date: string) {
  const xml = fs.readFileSync(metainfoPath, "utf-8");
  if (xml.includes(`version="${version}"`)) {
    console.log(`[sync-flatpak] metainfo already has a <release> entry for ${version}, skipping.`);
    return;
  }
  const updated = xml.replace(/(<releases>\n)/, `$1    <release version="${version}" date="${date}" />\n`);
  if (updated === xml) {
    throw new Error(`Could not find <releases> block in ${metainfoPath}`);
  }
  fs.writeFileSync(metainfoPath, updated);
  console.log(`[sync-flatpak] Added <release version="${version}" date="${date}" /> to metainfo.`);
}

async function runSync() {
  regenerateInto(flatpakDir);
  const version = currentVersion();
  const date = new Date().toISOString().slice(0, 10);
  syncMetainfoRelease(version, date);
  console.log("\nflatpak/ dependency sources and metainfo release history are in sync.");
}

async function runCheck() {
  const scratchDir = path.join(rootDir, "scratch", "flatpak-check");
  fs.rmSync(scratchDir, { recursive: true, force: true });
  regenerateInto(scratchDir);

  let drifted = false;
  for (const file of GENERATED_FILES) {
    const committed = fs.readFileSync(path.join(flatpakDir, file), "utf-8");
    const generated = fs.readFileSync(path.join(scratchDir, file), "utf-8");
    if (committed !== generated) {
      drifted = true;
      console.error(`\nDrift detected in flatpak/${file}:`);
      try {
        execSync(`diff -u "${path.join(flatpakDir, file)}" "${path.join(scratchDir, file)}"`, { stdio: "inherit" });
      } catch {
        // diff exits non-zero when files differ — expected here, not a failure.
      }
    }
  }
  fs.rmSync(scratchDir, { recursive: true, force: true });

  // Also catch a version bump that never ran the sync step: the metainfo
  // should already have a <release> entry for the current package.json version.
  const version = currentVersion();
  const xml = fs.readFileSync(metainfoPath, "utf-8");
  if (!xml.includes(`version="${version}"`)) {
    drifted = true;
    console.error(`\nmetainfo is missing a <release> entry for the current version (${version}).`);
  }

  if (drifted) {
    console.error("\nflatpak/ is out of date. Run: bun run scripts/sync-flatpak.ts");
    process.exit(1);
  }
  console.log("flatpak/ dependency sources and metainfo release history match. No drift.");
}

async function main() {
  const args = process.argv.slice(2);
  if (args.includes("--check")) {
    await runCheck();
  } else {
    await runSync();
  }
}

main().catch((err) => {
  console.error("[sync-flatpak] Failed:", err);
  process.exit(1);
});
