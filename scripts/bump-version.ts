// scripts/bump-version.ts
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

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

  console.log("[Version Bump] Completed successfully!");
}

main();
