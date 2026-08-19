// Prepares a Flathub submission from the in-repo Flatpak manifest, mirroring
// scripts/submit-winget.ts's shape (fork/clone, drop files in, open a PR).
//
// Flathub has two distinct phases, unlike winget's single versioned-directory
// PR model:
//   1. New-app submission (this script's default target): open a PR against
//      a NEW BRANCH named after the app id on flathub/flathub, containing
//      just the manifest + generated sources at the branch root. Flathub's
//      infra creates flathub/org.luminous.music from that branch once merged
//      — see https://docs.flathub.org/docs/for-app-authors/submission.
//   2. Post-acceptance updates: once flathub/org.luminous.music exists, point
//      REPO at "flathub/org.luminous.music" instead and this script opens a
//      normal PR against its main branch.
//
// This has not been run against the real flathub/flathub repo yet — dry-run
// (--dry-run) and read the diff in scratch/ before ever pushing/opening a PR
// for a real submission.
import { execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");

const APP_ID = "org.luminous.music";

// Switch to "esoltys/org.luminous.music" (or wherever it lands) once accepted.
const REPO = process.env.FLATHUB_SUBMIT_REPO ?? "flathub/flathub";
const [REPO_OWNER, REPO_NAME] = REPO.split("/");

const args = process.argv.slice(2);
const tagArg = args.find((a) => a.startsWith("--tag="))?.split("=")[1];
const dryRun = args.includes("--dry-run");

if (!tagArg) {
  console.error("Usage: bun run scripts/submit-flathub.ts --tag=vX.Y.Z [--dry-run]");
  process.exit(1);
}

async function main() {
  console.log(`Preparing Flathub submission for ${APP_ID} @ ${tagArg} -> ${REPO}...`);

  const commit = execSync(`git rev-list -n 1 ${tagArg}`, { cwd: rootDir }).toString().trim();
  console.log(`Resolved ${tagArg} -> ${commit}`);

  const scratchDir = path.join(rootDir, "scratch", "flathub-submit");
  const tmpDir = path.join(scratchDir, "target-repo");
  fs.mkdirSync(scratchDir, { recursive: true });
  if (fs.existsSync(tmpDir)) {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }

  const branchName = REPO === "flathub/flathub" ? APP_ID : "master";

  console.log(`Forking ${REPO}...`);
  try {
    execSync(`gh repo fork ${REPO} --clone=false`, { stdio: "inherit" });
  } catch {
    console.log("Fork already exists or succeeded with notice.");
  }

  console.log(`Cloning esoltys/${REPO_NAME} (shallow)...`);
  execSync(`git clone --depth=1 https://github.com/esoltys/${REPO_NAME}.git "${tmpDir}"`, {
    stdio: "inherit",
  });

  try {
    execSync(`git checkout -b ${branchName}`, { cwd: tmpDir, stdio: "inherit" });
  } catch {
    execSync(`git checkout ${branchName}`, { cwd: tmpDir, stdio: "inherit" });
  }

  // Build the submission manifest: same as the in-repo one, but with the
  // `type: dir` local-checkout source swapped for a pinned `type: git` source
  // — Flathub's build infra clones fresh, it has no access to this checkout.
  const manifestSrc = path.join(rootDir, "flatpak", `${APP_ID}.yml`);
  let manifest = fs.readFileSync(manifestSrc, "utf-8");
  manifest = manifest.replace(
    /- type: dir\n\s+path: \.\.\/\.\.\n/,
    `- type: git\n        url: https://github.com/esoltys/luminous.git\n        tag: ${tagArg}\n        commit: ${commit}\n`,
  );

  const filesToCopy = ["cargo-sources.json", "node-sources.json", "package-lock.json", `${APP_ID}.desktop`];

  if (dryRun) {
    console.log("--- Dry run: would write the following to the target repo ---");
    console.log(`${APP_ID}.yml:\n${manifest}`);
    console.log(`Plus copied as-is: ${filesToCopy.join(", ")}`);
    return;
  }

  fs.writeFileSync(path.join(tmpDir, `${APP_ID}.yml`), manifest);
  for (const file of filesToCopy) {
    fs.copyFileSync(path.join(rootDir, "flatpak", file), path.join(tmpDir, file));
  }

  execSync("git add -A", { cwd: tmpDir, stdio: "inherit" });
  try {
    execSync(`git commit -m "${APP_ID}: update to ${tagArg}"`, { cwd: tmpDir, stdio: "inherit" });
  } catch {
    console.log("No changes to commit.");
  }

  console.log(`Pushing branch ${branchName}...`);
  execSync(`git push -u origin ${branchName} --force`, { cwd: tmpDir, stdio: "inherit" });

  console.log(`Creating Pull Request to ${REPO}...`);
  const prCmd = `gh pr create --repo ${REPO} --head esoltys:${branchName} --base master --title "${APP_ID}: ${tagArg}" --body "Update Luminous Flatpak manifest to ${tagArg}."`;
  execSync(prCmd, { cwd: tmpDir, stdio: "inherit" });

  fs.rmSync(tmpDir, { recursive: true, force: true });
  console.log("\n=============================================================");
  console.log(`Successfully submitted PR to ${REPO}!`);
  console.log("=============================================================");
}

main().catch((err) => {
  console.error("Error submitting Flathub PR:", err);
  process.exit(1);
});
