import { execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");
const manifestDir = path.join(rootDir, "winget", "manifests", "e", "EricSoltys", "Luminous", "0.99.2");

async function main() {
  console.log("Submitting EricSoltys.Luminous v0.99.2 to microsoft/winget-pkgs...");
  
  const tmpDir = path.join(rootDir, "scratch", "winget-pkgs-temp");
  if (fs.existsSync(tmpDir)) {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
  fs.mkdirSync(path.join(rootDir, "scratch"), { recursive: true });

  console.log("Forking microsoft/winget-pkgs...");
  try {
    execSync("gh repo fork microsoft/winget-pkgs --clone=false", { stdio: "inherit" });
  } catch {
    console.log("Fork already exists or succeeded with notice.");
  }

  console.log("Cloning esoltys/winget-pkgs (shallow)...");
  execSync(`git clone --depth=1 https://github.com/esoltys/winget-pkgs.git "${tmpDir}"`, { stdio: "inherit" });

  const branchName = "EricSoltys.Luminous-0.99.2";
  try {
    execSync(`git checkout -b ${branchName}`, { cwd: tmpDir, stdio: "inherit" });
  } catch {
    execSync(`git checkout ${branchName}`, { cwd: tmpDir, stdio: "inherit" });
  }

  const targetDir = path.join(tmpDir, "manifests", "e", "EricSoltys", "Luminous", "0.99.2");
  fs.mkdirSync(targetDir, { recursive: true });

  const files = [
    "EricSoltys.Luminous.yaml",
    "EricSoltys.Luminous.installer.yaml",
    "EricSoltys.Luminous.locale.en-US.yaml"
  ];

  for (const file of files) {
    fs.copyFileSync(path.join(manifestDir, file), path.join(targetDir, file));
  }

  execSync("git add -A", { cwd: tmpDir, stdio: "inherit" });
  try {
    execSync('git commit -m "New package: EricSoltys.Luminous version 0.99.2"', { cwd: tmpDir, stdio: "inherit" });
  } catch {
    console.log("No changes to commit.");
  }

  console.log(`Pushing branch ${branchName}...`);
  execSync(`git push -u origin ${branchName} --force`, { cwd: tmpDir, stdio: "inherit" });

  console.log("Creating Pull Request to microsoft/winget-pkgs...");
  const prCmd = `gh pr create --repo microsoft/winget-pkgs --head esoltys:${branchName} --base master --title "New package: EricSoltys.Luminous version 0.99.2" --body "New package submission for Luminous v0.99.2"`;
  execSync(prCmd, { cwd: tmpDir, stdio: "inherit" });

  fs.rmSync(tmpDir, { recursive: true, force: true });
  console.log("\n=============================================================");
  console.log("Successfully submitted PR to microsoft/winget-pkgs!");
  console.log("=============================================================");
}

main().catch(err => {
  console.error("Error submitting Winget PR:", err);
  process.exit(1);
});
