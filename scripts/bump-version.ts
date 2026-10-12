// Bumps the app version across package.json, src-tauri/tauri.conf.json,
// src-tauri/Cargo.toml, and src-tauri/metainfo/org.luminous.music.metainfo.xml
// in one shot, so a release doesn't miss one of them.
// Usage: bun run scripts/bump-version.ts <version>
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";
import { execSync } from "child_process";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");

function main() {
  const args = process.argv.slice(2);
  if (args.length < 1) {
    console.error("Usage: bun run scripts/bump-version.ts <version>");
    console.error("Example: bun run scripts/bump-version.ts 0.31.0");
    process.exit(1);
  }

  let version = args[0];
  // Ensure version matches a semver-like format. If they supply 0.31, make it 0.31.0
  const parts = version.split(".");
  if (parts.length === 2) {
    version = `${version}.0`;
  }

  console.log(`[Version Bump] Target version: ${version}`);

  // 1. package.json
  const pkgPath = path.join(rootDir, "package.json");
  if (fs.existsSync(pkgPath)) {
    const pkg = JSON.parse(fs.readFileSync(pkgPath, "utf-8"));
    const oldVersion = pkg.version;
    pkg.version = version;
    fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
    console.log(`[Version Bump] Updated package.json version: ${oldVersion} -> ${version}`);
  } else {
    console.error(`[Error] package.json not found at ${pkgPath}`);
  }

  // 2. src-tauri/tauri.conf.json
  const tauriPath = path.join(rootDir, "src-tauri", "tauri.conf.json");
  if (fs.existsSync(tauriPath)) {
    const tauri = JSON.parse(fs.readFileSync(tauriPath, "utf-8"));
    const oldVersion = tauri.version;
    tauri.version = version;
    fs.writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + "\n");
    console.log(`[Version Bump] Updated tauri.conf.json version: ${oldVersion} -> ${version}`);
  } else {
    console.error(`[Error] tauri.conf.json not found at ${tauriPath}`);
  }

  // 3. src-tauri/Cargo.toml
  const cargoPath = path.join(rootDir, "src-tauri", "Cargo.toml");
  if (fs.existsSync(cargoPath)) {
    let cargo = fs.readFileSync(cargoPath, "utf-8");
    const oldVersionMatch = cargo.match(/^version\s*=\s*"([^"]+)"/m);
    const oldVersion = oldVersionMatch ? oldVersionMatch[1] : "unknown";
    
    // Replace the version string under [package]
    cargo = cargo.replace(/^version\s*=\s*"[^"]+"/m, `version = "${version}"`);
    fs.writeFileSync(cargoPath, cargo);
    console.log(`[Version Bump] Updated Cargo.toml version: ${oldVersion} -> ${version}`);
  } else {
    console.error(`[Error] Cargo.toml not found at ${cargoPath}`);
  }

  // 4. src-tauri/metainfo/org.luminous.music.metainfo.xml
  const metainfoPath = path.join(rootDir, "src-tauri", "metainfo", "org.luminous.music.metainfo.xml");
  if (fs.existsSync(metainfoPath)) {
    const metainfo = fs.readFileSync(metainfoPath, "utf-8");
    const today = new Date().toISOString().split("T")[0];
    const lines = metainfo.split("\n");
    const targetPattern = `version="${version}"`;
    let updated = false;

    for (let i = 0; i < lines.length; i++) {
      if (lines[i].includes("<release ") && lines[i].includes(targetPattern)) {
        lines[i] = `    <release version="${version}" date="${today}" />`;
        updated = true;
        break;
      }
    }

    if (updated) {
      fs.writeFileSync(metainfoPath, lines.join("\n"));
      console.log(`[Version Bump] Updated existing release in metainfo.xml: ${version} (${today})`);
    } else {
      const releasesIndex = lines.findIndex(l => l.includes("<releases>"));
      if (releasesIndex !== -1) {
        lines.splice(releasesIndex + 1, 0, `    <release version="${version}" date="${today}" />`);
        fs.writeFileSync(metainfoPath, lines.join("\n"));
        console.log(`[Version Bump] Added release to metainfo.xml: ${version} (${today})`);
      } else {
        console.error(`[Error] <releases> block not found in ${metainfoPath}`);
      }
    }
  } else {
    console.error(`[Error] metainfo.xml not found at ${metainfoPath}`);
  }

  // 5. Sync bun.lock — package.json's version bump doesn't touch dependency ranges,
  // but this keeps the lockfile from silently drifting release over release if it
  // was already stale (e.g. a prior dependency bump that never got a fresh install).
  console.log("[Version Bump] Syncing bun.lock...");
  execSync("bun install", { stdio: "inherit", cwd: rootDir });

  console.log("[Version Bump] Completed successfully!");
}

main();
