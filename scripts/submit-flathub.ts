// Prepares a Flathub submission from the in-repo Flatpak manifest, mirroring
// scripts/submit-winget.ts's shape (fork/clone, drop files in, open a PR).
//
// Flathub has two distinct phases, unlike winget's single versioned-directory
// PR model:
//   1. New-app submission (this script's default target): open a PR against
//      a NEW BRANCH named after the app id on flathub/flathub, containing
//      just the manifest + generated sources at the branch root. Flathub's
//      infra creates flathub/io.github.esoltys.Luminous from that branch once
//      merged — see https://docs.flathub.org/docs/for-app-authors/submission.
//   2. Post-acceptance updates: once flathub/io.github.esoltys.Luminous exists,
//      point REPO at "flathub/io.github.esoltys.Luminous" instead and this
//      script opens a normal PR against its main branch.
//      .github/workflows/flatpak-publish.yml is an alternative to this step —
//      it pushes an accepted build straight into Flathub's build pipeline via
//      flat-manager on every release tag, once a FLATHUB_TOKEN repo secret
//      exists (see #476).
//
// Always dry-run (--dry-run) and read the diff before pushing/opening a PR
// for a real submission.
import { execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");

// Deliberately not org.luminous.music (the app's internal Tauri identifier,
// used by .deb/.rpm) — Flathub requires a controlled domain for a reverse-DNS
// id, which music.luminous.org isn't. See the comment atop
// flatpak/io.github.esoltys.Luminous.yml for the full rationale.
const APP_ID = "io.github.esoltys.Luminous";

// Switch to "esoltys/io.github.esoltys.Luminous" (or wherever it lands) once accepted.
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

  const branchName = REPO === "flathub/flathub" ? APP_ID : "master";
  // New-app submissions branch off flathub/flathub's `new-pr` branch, not its
  // `master` — see https://github.com/flathub/flathub/blob/master/CONTRIBUTING.md.
  // Post-acceptance updates against flathub/io.github.esoltys.Luminous branch
  // off its normal `master`.
  const baseBranch = REPO === "flathub/flathub" ? "new-pr" : "master";

  // Build the submission manifest: same as the in-repo one, but with the
  // `type: dir` local-checkout source swapped for a pinned `type: git` source
  // — Flathub's build infra clones fresh, it has no access to this checkout.
  const manifestSrc = path.join(rootDir, "flatpak", `${APP_ID}.yml`);
  let manifest = fs.readFileSync(manifestSrc, "utf-8");
  manifest = manifest.replace(
    /- type: dir\n\s+path: \.\.\/\.\.\n/,
    `- type: git\n        url: https://github.com/esoltys/luminous.git\n        tag: ${tagArg}\n        commit: ${commit}\n`,
  );
  // `type: dir` (used for local/CI builds) nests repo contents under
  // ./luminous/ — see the comment above build-commands in the manifest.
  // `type: git` clones straight into the build root, so those `cd luminous
  // &&` prefixes and `luminous/`-prefixed paths need to come back out here.
  manifest = manifest.replace(/cd luminous && /g, "");
  manifest = manifest.replace(/(?<![\w/])luminous\//g, "");

  const filesToCopy = ["cargo-sources.json", "node-sources.json", "package.json", "package-lock.json", `${APP_ID}.desktop`];

  if (dryRun) {
    console.log("--- Dry run: would write the following to the target repo ---");
    console.log(`${APP_ID}.yml:\n${manifest}`);
    console.log(`Plus copied as-is: ${filesToCopy.join(", ")}`);
    console.log(`(No fork, clone, or push performed — nothing touched on GitHub.)`);
    return;
  }

  // Everything below here talks to GitHub (fork/clone/push/PR) — only runs
  // once dryRun above has returned.
  const scratchDir = path.join(rootDir, "scratch", "flathub-submit");
  const tmpDir = path.join(scratchDir, "target-repo");
  fs.mkdirSync(scratchDir, { recursive: true });
  if (fs.existsSync(tmpDir)) {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }

  console.log(`Forking ${REPO}...`);
  try {
    // No --default-branch-only: forks all branches, matching CONTRIBUTING.md's
    // "Copy the master branch only" left unchecked.
    execSync(`gh repo fork ${REPO} --clone=false`, { stdio: "inherit" });
  } catch {
    console.log("Fork already exists or succeeded with notice.");
  }

  // Clone directly at baseBranch (not the fork's default branch) — a plain
  // `git clone --depth=1` grabs the default branch (master), and master has
  // no shared history with new-pr on flathub/flathub, which makes GitHub
  // reject the PR later ("no history in common").
  console.log(`Cloning esoltys/${REPO_NAME} (shallow, base: ${baseBranch})...`);
  execSync(
    `git clone --depth=1 --branch ${baseBranch} https://github.com/esoltys/${REPO_NAME}.git "${tmpDir}"`,
    { stdio: "inherit" },
  );

  try {
    execSync(`git checkout -b ${branchName}`, { cwd: tmpDir, stdio: "inherit" });
  } catch {
    execSync(`git checkout ${branchName}`, { cwd: tmpDir, stdio: "inherit" });
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

  // Flathub's submission-checker bot hard-requires this exact title format
  // for a NEW submission ("Add $FLATPAK_ID") — confirmed by a real rejection
  // ("PR title is 'Add $FLATPAK_ID'"). Post-acceptance updates aren't new
  // submissions, so they keep the old descriptive title.
  const prTitle = REPO === "flathub/flathub" ? `Add ${APP_ID}` : `${APP_ID}: ${tagArg}`;

  console.log(`Creating Pull Request to ${REPO} (base: ${baseBranch})...`);
  const prCmd = `gh pr create --repo ${REPO} --head esoltys:${branchName} --base ${baseBranch} --title "${prTitle}" --body "Update Luminous Flatpak manifest to ${tagArg}."`;
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
