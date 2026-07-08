// scripts/release.ts
import { Database } from "bun:sqlite";
import { execSync } from "child_process";
import * as fs from "fs";
import * as path from "path";
import { fileURLToPath } from "url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, "..");

const BEEPER_API_URL = "http://127.0.0.1:23373";

// Helper to get Beeper access token and self-chat ID from local AppData
async function getBeeperClient() {
  const dbPath = path.join(
    process.env.APPDATA || "",
    "BeeperTexts",
    "account.db"
  );
  if (!fs.existsSync(dbPath)) {
    throw new Error(`Beeper database not found at ${dbPath}`);
  }

  const db = new Database(dbPath, { readonly: true });
  const account = db.query("SELECT access_token FROM account LIMIT 1").get() as any;
  if (!account || !account.access_token) {
    throw new Error("No access token found in Beeper database");
  }

  const token = account.access_token;

  // Query chats to find the self-chat (Eric / isSelfRoom)
  const res = await fetch(`${BEEPER_API_URL}/v1/chats`, {
    headers: { Authorization: `Bearer ${token}` }
  });
  if (!res.ok) {
    throw new Error(`Failed to fetch Beeper chats: ${res.status}`);
  }
  const data = (await res.json()) as any;
  const chats = data.items || [];
  
  // Find a chat that has Eric as self-room or participant
  const selfChat = chats.find((c: any) => 
    c.participants?.items?.every((p: any) => p.isSelf) || 
    c.title === "Eric"
  );

  if (!selfChat) {
    throw new Error("Could not find a Beeper self-chat (Eric)");
  }

  return {
    token,
    chatID: selfChat.id,
    sendNotification: async (text: string) => {
      const sendRes = await fetch(`${BEEPER_API_URL}/v1/chats/${selfChat.id}/messages`, {
        method: "POST",
        headers: {
          "Authorization": `Bearer ${token}`,
          "Content-Type": "application/json"
        },
        body: JSON.stringify({ text })
      });
      if (!sendRes.ok) {
        console.error(`[Beeper] Failed to send notification: ${sendRes.status} ${await sendRes.text()}`);
      } else {
        console.log(`[Beeper] Notification sent: "${text.split("\n")[0]}"`);
      }
    }
  };
}

function runCommand(cmd: string) {
  console.log(`\nExecuting: ${cmd}`);
  execSync(cmd, { stdio: "inherit", cwd: rootDir });
}

async function main() {
  const args = process.argv.slice(2);
  const isPushEnabled = args.includes("--push");
  const versionArg = args.find(a => !a.startsWith("-"));

  if (!versionArg) {
    console.error("Usage: bun run scripts/release.ts <version> [--push]");
    console.error("Example: bun run scripts/release.ts 0.31");
    process.exit(1);
  }

  let version = versionArg;
  const parts = version.split(".");
  if (parts.length === 2) {
    version = `${version}.0`;
  }
  const tagName = `v${version}`;

  let beeper: any;
  try {
    beeper = await getBeeperClient();
    console.log(`[Release Automation] Beeper connected successfully. Chat ID: ${beeper.chatID}`);
  } catch (err: any) {
    console.warn(`[WARNING] Beeper not configured/running: ${err.message}`);
  }

  try {
    // 1. Bump version
    console.log(`\n--- Bumping Version to ${version} ---`);
    runCommand(`bun run scripts/bump-version.ts ${version}`);

    // 2. Run local checks
    console.log("\n--- Running Frontend Checks (check) ---");
    runCommand("bun run check");

    console.log("\n--- Running Frontend Tests (test:run) ---");
    runCommand("bun run test:run");

    console.log("\n--- Running Cargo Check ---");
    runCommand("cargo check --manifest-path src-tauri/Cargo.toml");

    // 3. Git Commit
    console.log("\n--- Staging and committing changes ---");
    runCommand("git add -A");
    try {
      runCommand(`git commit -m "chore: bump version to ${version}"`);
    } catch {
      console.log("No changes to commit, continuing...");
    }

    // 4. Git Tag
    console.log(`\n--- Creating tag ${tagName} ---`);
    try {
      runCommand(`git tag -d ${tagName}`);
    } catch {}
    runCommand(`git tag -a ${tagName} -m "Release ${tagName}"`);

    // Notify of tag creation
    if (beeper) {
      await beeper.sendNotification(`🚀 **Luminous Release Prepared locally**\nTag \`${tagName}\` created. Ready to build.`);
    }

    if (!isPushEnabled) {
      console.log(`\n=============================================================`);
      console.log(`Release is prepared locally!`);
      console.log(`To push the release tag and trigger the GitHub Action build, run:`);
      console.log(`  git push origin main && git push origin ${tagName}`);
      console.log(`=============================================================`);
      return;
    }

    // 5. Git Push and Monitor
    console.log("\n--- Pushing main and tag to trigger GitHub Action ---");
    runCommand("git push origin main");
    runCommand(`git push origin ${tagName}`);

    if (beeper) {
      await beeper.sendNotification(`📦 **GitHub Action Build Triggered**\nRelease build for \`${tagName}\` has been pushed and is starting.`);
    }

    console.log("\n--- Monitoring GitHub Action Build ---");
    console.log("Waiting for build to appear on GitHub...");
    
    // Poll for the run to show up
    let runId = "";
    for (let i = 0; i < 10; i++) {
      await new Promise(r => setTimeout(r, 3000));
      try {
        const runJson = execSync(`gh run list --tag ${tagName} --limit 1 --json databaseId,status,conclusion`, { encoding: "utf-8" });
        const runs = JSON.parse(runJson);
        if (runs && runs.length > 0) {
          runId = runs[0].databaseId.toString();
          console.log(`Found GitHub run: ${runId} (Status: ${runs[0].status})`);
          break;
        }
      } catch (err) {
        // Ignored, retry
      }
    }

    if (!runId) {
      console.warn("[WARNING] Could not find triggered GitHub run. Please check GitHub Actions manually.");
      if (beeper) {
        await beeper.sendNotification(`⚠️ **Build Monitor Warning**\nCould not find run for tag \`${tagName}\` on GitHub. Please check actions page.`);
      }
      return;
    }

    // Monitor run completion
    console.log(`Watching run ${runId}...`);
    let completed = false;
    let success = false;
    while (!completed) {
      await new Promise(r => setTimeout(r, 15000)); // Poll every 15s
      try {
        const runJson = execSync(`gh run view ${runId} --json status,conclusion`, { encoding: "utf-8" });
        const run = JSON.parse(runJson);
        console.log(`Polling run ${runId}: status=${run.status}, conclusion=${run.conclusion}`);
        if (run.status === "completed") {
          completed = true;
          success = run.conclusion === "success";
        }
      } catch (err) {
        console.error("Error polling run:", err);
      }
    }

    // Final notification
    if (beeper) {
      if (success) {
        await beeper.sendNotification(`✅ **Luminous Release Build Succeeded!**\nBuild for \`${tagName}\` completed successfully.\nLinux and Windows artifacts are drafted on GitHub.`);
      } else {
        await beeper.sendNotification(`❌ **Luminous Release Build Failed!**\nBuild for \`${tagName}\` failed.\nView logs here: https://github.com/${getRepoName()}/actions/runs/${runId}`);
      }
    }

  } catch (err: any) {
    console.error(`\n[Error] Release script failed: ${err.message}`);
    if (beeper) {
      await beeper.sendNotification(`❌ **Release Automation Failed**\n${err.message}`);
    }
    process.exit(1);
  }
}

function getRepoName(): string {
  try {
    const remote = execSync("git remote get-url origin", { encoding: "utf-8" }).trim();
    // Handles git@github.com:owner/repo.git or https://github.com/owner/repo.git
    const match = remote.match(/github\.com[:/]([^/]+\/[^.]+)/);
    return match ? match[1] : "esoltys/luminous";
  } catch {
    return "esoltys/luminous";
  }
}

main();
