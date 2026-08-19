// Regenerates the winget manifest trio for a given Luminous release and
// (with --push) submits it to microsoft/winget-pkgs. Replaces the old
// hand-edited-per-release version of this script (#477) — the manifest
// version, installer URL, and SHA256 all come from the GitHub release
// instead of being typed in by hand.
//
// Usage:
//   bun run scripts/submit-winget.ts <version> [--push]
//   bun run scripts/submit-winget.ts --check          # drift check, no network write
//
// <version> is the release version (e.g. 1.0.0 or v1.0.0). Without --push,
// only the local manifest folder is (re)generated — nothing is submitted
// upstream. --check regenerates into a temp dir and diffs against the
// latest committed version folder, exiting non-zero on drift.
import { execSync } from "child_process";
import * as crypto from "crypto";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");

const PACKAGE_IDENTIFIER = "EricSoltys.LuminousMusicPlayer";
// Keep in sync with the schema version recommended in microsoft/winget-pkgs's
// PR template (doc/manifest/README.md lists 1.6.0 as deprecated).
const MANIFEST_VERSION = "1.12.0";

// winget's manifest layout nests one folder per dot-separated identifier
// segment under manifests/<first-letter>/, e.g. EricSoltys.LuminousMusicPlayer
// -> manifests/e/EricSoltys/LuminousMusicPlayer/<version>/. Deriving this
// from PACKAGE_IDENTIFIER instead of hardcoding it keeps the path in sync if
// the identifier ever changes again.
const idSegments = PACKAGE_IDENTIFIER.split(".");
const manifestsRoot = path.join(rootDir, "winget", "manifests", idSegments[0][0].toLowerCase(), ...idSegments);

function normalizeVersion(v: string): string {
  return v.replace(/^v/, "");
}

function latestCommittedVersion(): string {
  const versions = fs.readdirSync(manifestsRoot).filter(f =>
    fs.statSync(path.join(manifestsRoot, f)).isDirectory()
  );
  if (versions.length === 0) {
    throw new Error(`No existing version folders under ${manifestsRoot}`);
  }
  // Sort as dotted-numeric versions, not lexically (1.0.0 > 0.99.7).
  versions.sort((a, b) => {
    const pa = a.split(".").map(Number);
    const pb = b.split(".").map(Number);
    for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
      const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
      if (diff !== 0) return diff;
    }
    return 0;
  });
  return versions[versions.length - 1];
}

async function sha256File(filePath: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const hash = crypto.createHash("sha256");
    const stream = fs.createReadStream(filePath);
    stream.on("data", chunk => hash.update(chunk));
    stream.on("end", () => resolve(hash.digest("hex")));
    stream.on("error", reject);
  });
}

interface ReleaseInfo {
  version: string;
  installerUrl: string;
  installerSha256: string;
}

async function fetchReleaseInfo(version: string): Promise<ReleaseInfo> {
  const tag = `v${version}`;
  console.log(`Fetching release ${tag} from GitHub...`);

  const assetsJson = execSync(
    `gh release view ${tag} --json assets -q ".assets[].name"`,
    { cwd: rootDir, encoding: "utf-8" }
  );
  const assetNames = assetsJson.split("\n").map(s => s.trim()).filter(Boolean);
  const exeAsset = assetNames.find(n => n.endsWith("_x64-setup.exe"));
  if (!exeAsset) {
    throw new Error(
      `No *_x64-setup.exe asset found on release ${tag}. Assets: ${assetNames.join(", ")}`
    );
  }

  const scratchDir = path.join(rootDir, "scratch", "winget-download");
  fs.rmSync(scratchDir, { recursive: true, force: true });
  fs.mkdirSync(scratchDir, { recursive: true });

  console.log(`Downloading ${exeAsset}...`);
  execSync(`gh release download ${tag} -p "${exeAsset}" -D "${scratchDir}"`, {
    cwd: rootDir,
    stdio: "inherit"
  });

  const downloadedPath = path.join(scratchDir, exeAsset);
  const installerSha256 = await sha256File(downloadedPath);
  fs.rmSync(scratchDir, { recursive: true, force: true });

  return {
    version,
    installerUrl: `https://github.com/esoltys/luminous/releases/download/${tag}/${exeAsset}`,
    installerSha256
  };
}

function versionManifest(version: string): string {
  return `# yaml-language-server: $schema=https://aka.ms/winget-manifest.version.${MANIFEST_VERSION}.schema.json

PackageIdentifier: ${PACKAGE_IDENTIFIER}
PackageVersion: ${version}
DefaultLocale: en-US
ManifestType: version
ManifestVersion: ${MANIFEST_VERSION}
`;
}

function installerManifest(info: ReleaseInfo): string {
  return `# yaml-language-server: $schema=https://aka.ms/winget-manifest.installer.${MANIFEST_VERSION}.schema.json

PackageIdentifier: ${PACKAGE_IDENTIFIER}
PackageVersion: ${info.version}
MinimumOSVersion: 10.0.0.0
InstallerType: nullsoft
Scope: user
InstallModes:
  - interactive
  - silent
UpgradeBehavior: install
FileExtensions:
  - mp3
  - flac
  - ogg
  - opus
  - m4a
  - alac
  - aac
  - wav
  - aiff
  - aif
  - wv
  - mpc
  - ape
  - tta
  - dsf
  - dff
  - asf
  - wma
  - m4b
  - m3u
  - m3u8
  - pls
  - xspf
Installers:
  - Architecture: x64
    InstallerUrl: ${info.installerUrl}
    InstallerSha256: ${info.installerSha256}
ManifestType: installer
ManifestVersion: ${MANIFEST_VERSION}
`;
}

function localeManifest(version: string): string {
  return `# yaml-language-server: $schema=https://aka.ms/winget-manifest.defaultLocale.${MANIFEST_VERSION}.schema.json

PackageIdentifier: ${PACKAGE_IDENTIFIER}
PackageVersion: ${version}
PackageLocale: en-US
Publisher: Eric James Soltys
PublisherUrl: https://esoltys.dev/luminous/
PublisherSupportUrl: https://github.com/esoltys/luminous/issues
Author: Eric James Soltys
PackageName: Luminous Music Player
PackageUrl: https://github.com/esoltys/luminous
License: MIT
LicenseUrl: https://github.com/esoltys/luminous/blob/main/LICENSE
Copyright: Copyright © 2026 Eric James Soltys
ShortDescription: A high-performance home for the music you already own
Description: Luminous is a fast, local-first player for your own audio library — no streaming, no subscriptions, no cloud. Just your files, indexed, searchable, and beautifully played.
Moniker: luminous
Tags:
  - music
  - player
  - audio
  - local
  - mp3
  - flac
  - tauri
ManifestType: defaultLocale
ManifestVersion: ${MANIFEST_VERSION}
`;
}

function writeManifests(targetDir: string, info: ReleaseInfo) {
  fs.mkdirSync(targetDir, { recursive: true });
  fs.writeFileSync(path.join(targetDir, `${PACKAGE_IDENTIFIER}.yaml`), versionManifest(info.version));
  fs.writeFileSync(path.join(targetDir, `${PACKAGE_IDENTIFIER}.installer.yaml`), installerManifest(info));
  fs.writeFileSync(path.join(targetDir, `${PACKAGE_IDENTIFIER}.locale.en-US.yaml`), localeManifest(info.version));
}

function validateManifest(dir: string) {
  console.log(`\nValidating manifest at ${dir}...`);
  try {
    execSync(`winget validate --manifest "${dir}"`, { cwd: rootDir, stdio: "inherit" });
  } catch {
    throw new Error("winget manifest validation failed. See output above.");
  }
}

// microsoft/winget-pkgs's PR template requires a local install test in
// addition to `winget validate` — this actually installs (and immediately
// removes) the package via the local manifest, using --accept-*-agreements
// so it can run unattended.
function installTestManifest(dir: string, version: string) {
  console.log(`\nTest-installing manifest at ${dir}...`);
  try {
    execSync(
      `winget install --manifest "${dir}" --accept-package-agreements --accept-source-agreements --silent`,
      { cwd: rootDir, stdio: "inherit" }
    );
  } catch {
    throw new Error("winget install test failed. See output above.");
  }
  try {
    execSync(`winget uninstall --id "${PACKAGE_IDENTIFIER}" --version "${version}" --silent`, {
      cwd: rootDir,
      stdio: "inherit"
    });
  } catch {
    console.warn("[WARNING] Could not uninstall test install automatically — remove it manually if needed.");
  }
}

async function runCheck() {
  const version = latestCommittedVersion();
  const committedDir = path.join(manifestsRoot, version);
  const scratchDir = path.join(rootDir, "scratch", "winget-check");
  fs.rmSync(scratchDir, { recursive: true, force: true });

  const info = await fetchReleaseInfo(version);
  writeManifests(scratchDir, info);

  const files = [`${PACKAGE_IDENTIFIER}.yaml`, `${PACKAGE_IDENTIFIER}.installer.yaml`, `${PACKAGE_IDENTIFIER}.locale.en-US.yaml`];
  let drifted = false;
  for (const file of files) {
    const committed = fs.readFileSync(path.join(committedDir, file), "utf-8");
    const generated = fs.readFileSync(path.join(scratchDir, file), "utf-8");
    if (committed !== generated) {
      drifted = true;
      console.error(`\nDrift detected in ${file}:`);
      try {
        execSync(`diff -u "${path.join(committedDir, file)}" "${path.join(scratchDir, file)}"`, {
          stdio: "inherit"
        });
      } catch {
        // diff exits non-zero when files differ — expected here, not a failure.
      }
    }
  }
  fs.rmSync(scratchDir, { recursive: true, force: true });

  if (drifted) {
    console.error(
      `\nwinget manifest for ${version} is out of date. Run: bun run scripts/submit-winget.ts ${version}`
    );
    process.exit(1);
  }
  console.log(`winget manifest for ${version} matches the current release. No drift.`);
}

async function submitPR(version: string, targetDir: string) {
  console.log(`\nSubmitting ${PACKAGE_IDENTIFIER} v${version} to microsoft/winget-pkgs...`);

  const tmpDir = path.join(rootDir, "scratch", "winget-pkgs-temp");
  fs.rmSync(tmpDir, { recursive: true, force: true });
  fs.mkdirSync(path.join(rootDir, "scratch"), { recursive: true });

  console.log("Forking microsoft/winget-pkgs...");
  try {
    execSync("gh repo fork microsoft/winget-pkgs --clone=false", { stdio: "inherit" });
  } catch {
    console.log("Fork already exists or succeeded with notice.");
  }

  // winget-pkgs has 600k+ manifest files; a normal clone checks all of them
  // out and can hit Windows' MAX_PATH on unrelated deeply-nested packages.
  // A blobless, sparse (cone-mode) clone only checks out our package's path.
  console.log("Cloning esoltys/winget-pkgs (sparse, blobless)...");
  execSync(
    `git clone --filter=blob:none --no-checkout --depth=1 https://github.com/esoltys/winget-pkgs.git "${tmpDir}"`,
    { stdio: "inherit" }
  );
  execSync("git sparse-checkout init --cone", { cwd: tmpDir, stdio: "inherit" });
  const upstreamManifestPath = ["manifests", idSegments[0][0].toLowerCase(), ...idSegments].join("/");
  execSync(`git sparse-checkout set "${upstreamManifestPath}"`, { cwd: tmpDir, stdio: "inherit" });

  console.log("Syncing with upstream microsoft/winget-pkgs master...");
  execSync("git remote add upstream https://github.com/microsoft/winget-pkgs.git", { cwd: tmpDir, stdio: "inherit" });
  execSync("git fetch --depth=1 upstream master", { cwd: tmpDir, stdio: "inherit" });
  execSync("git checkout -B master upstream/master", { cwd: tmpDir, stdio: "inherit" });

  const branchName = `${PACKAGE_IDENTIFIER}-${version}`;
  execSync(`git checkout -B ${branchName}`, { cwd: tmpDir, stdio: "inherit" });

  const upstreamTargetDir = path.join(tmpDir, "manifests", idSegments[0][0].toLowerCase(), ...idSegments, version);
  fs.mkdirSync(upstreamTargetDir, { recursive: true });

  const files = [`${PACKAGE_IDENTIFIER}.yaml`, `${PACKAGE_IDENTIFIER}.installer.yaml`, `${PACKAGE_IDENTIFIER}.locale.en-US.yaml`];
  for (const file of files) {
    fs.copyFileSync(path.join(targetDir, file), path.join(upstreamTargetDir, file));
  }

  execSync("git add -A", { cwd: tmpDir, stdio: "inherit" });
  try {
    execSync(`git commit -m "New package: ${PACKAGE_IDENTIFIER} version ${version}"`, { cwd: tmpDir, stdio: "inherit" });
  } catch {
    console.log("No changes to commit.");
  }

  console.log(`Pushing branch ${branchName}...`);
  execSync(`git push -u origin ${branchName} --force`, { cwd: tmpDir, stdio: "inherit" });

  console.log("Creating Pull Request to microsoft/winget-pkgs...");
  const prBody = [
    "## \u{1F4D6} Description",
    `New package submission for Luminous Music Player v${version}.`,
    "",
    "## ✅ Checklist",
    "- [x] Signed the Contributor License Agreement",
    "",
    "## \u{1F4E6} Manifest Checklist",
    "- [x] Checked that there aren't other open pull requests for the same manifest update/change",
    "- [x] This PR only modifies one (1) manifest",
    "- [x] Validated manifest locally with `winget validate --manifest <path>`",
    "- [x] Tested manifest locally with `winget install --manifest <path>`",
    `- [x] Manifest conforms to the ${MANIFEST_VERSION} schema`
  ].join("\n");
  const prBodyPath = path.join(tmpDir, ".pr-body.md");
  fs.writeFileSync(prBodyPath, prBody);
  const prCmd = `gh pr create --repo microsoft/winget-pkgs --head esoltys:${branchName} --base master --title "New package: ${PACKAGE_IDENTIFIER} version ${version}" --body-file "${prBodyPath}"`;
  execSync(prCmd, { cwd: tmpDir, stdio: "inherit" });

  fs.rmSync(tmpDir, { recursive: true, force: true });
  console.log("\n=============================================================");
  console.log("Successfully submitted PR to microsoft/winget-pkgs!");
  console.log("=============================================================");
}

async function main() {
  const args = process.argv.slice(2);

  if (args.includes("--check")) {
    await runCheck();
    return;
  }

  const isPushEnabled = args.includes("--push");
  const versionArg = args.find(a => !a.startsWith("-"));
  if (!versionArg) {
    console.error("Usage: bun run scripts/submit-winget.ts <version> [--push]");
    console.error("       bun run scripts/submit-winget.ts --check");
    process.exit(1);
  }

  const version = normalizeVersion(versionArg);
  const targetDir = path.join(manifestsRoot, version);

  const info = await fetchReleaseInfo(version);
  writeManifests(targetDir, info);
  console.log(`\nGenerated manifest at ${path.relative(rootDir, targetDir)}`);

  try {
    validateManifest(targetDir);
  } catch (err) {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  }

  if (!isPushEnabled) {
    console.log("\nManifest generated and validated locally. Re-run with --push to submit to microsoft/winget-pkgs.");
    return;
  }

  // winget-pkgs' PR checklist requires a local install test, not just
  // `winget validate` — only run it for a real submission, not every
  // no-op regeneration (it actually installs/uninstalls the package).
  try {
    installTestManifest(targetDir, version);
  } catch (err) {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  }

  await submitPR(version, targetDir);
}

main().catch(err => {
  console.error("Error:", err);
  process.exit(1);
});
