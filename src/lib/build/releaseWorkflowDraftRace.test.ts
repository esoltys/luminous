// @vitest-environment node
//
// Regression test for the release-workflow draft-release race (e58ef3b):
// the Linux and Windows release legs both used `tauri-action`, which
// creates the GitHub draft release itself if one doesn't exist yet.
// Running both platforms in the same matrix at once raced to create two
// separate drafts with split build assets and incompatible updater
// `latest.json` manifests (hit in v0.99.2) — silently breaking auto-update
// for whichever platform's manifest didn't end up live.
//
// The original fix serialized the matrix (`max-parallel: 1`). That was
// later superseded by 7979cc1, which instead splits release creation into
// its own `create-release` job that runs once *before* the platform
// matrix, and has each matrix leg pass that job's `release_id`/`tag`
// straight into `tauri-action` (`releaseId`/`tagName`) instead of letting
// `tauri-action` discover-or-create the draft itself. That's the
// mechanism actually preventing the race today — `max-parallel: 1` is
// gone from the file entirely — so this test pins *that* invariant rather
// than the older one, on the actual current workflow text.
import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const workflowPath = path.resolve(__dirname, "../../../.github/workflows/release.yml");
const workflow = fs.readFileSync(workflowPath, "utf-8");

function extractJobBlock(yaml: string, jobName: string): string {
  // Job blocks are top-level (2-space indented) keys under `jobs:`; take
  // everything from `\n  <jobName>:` up to the next top-level job key or
  // end of file.
  const jobStart = yaml.search(new RegExp(`\\n  ${jobName}:\\n`));
  expect(jobStart, `could not find job "${jobName}" in ${workflowPath}`).toBeGreaterThan(-1);
  const rest = yaml.slice(jobStart + 1);
  const nextJobMatch = rest.slice(rest.indexOf("\n") + 1).search(/\n  [a-zA-Z0-9_-]+:\n/);
  return nextJobMatch === -1 ? rest : rest.slice(0, nextJobMatch + rest.indexOf("\n") + 1);
}

describe("release.yml: platform matrix can't race to create the draft release", () => {
  it("has a dedicated create-release job producing a release_id output", () => {
    const createRelease = extractJobBlock(workflow, "create-release");
    expect(createRelease).toMatch(/release_id:\s*\$\{\{\s*steps\.release\.outputs\.id\s*\}\}/);
  });

  it("the platform-matrix release job depends on create-release", () => {
    const release = extractJobBlock(workflow, "release");
    expect(
      release,
      "the `release` job must declare `needs: create-release` so the draft exists before any matrix leg runs"
    ).toMatch(/needs:\s*create-release/);
  });

  it("each matrix leg passes the pre-created releaseId/tagName into tauri-action instead of letting it discover/create the draft", () => {
    const release = extractJobBlock(workflow, "release");
    expect(release).toMatch(/releaseId:\s*\$\{\{\s*needs\.create-release\.outputs\.release_id\s*\}\}/);
    expect(release).toMatch(/tagName:\s*\$\{\{\s*needs\.create-release\.outputs\.tag\s*\}\}/);
  });
});

// Both softprops/action-gh-release and tauri-action set the release's draft
// state on every upload. A hardcoded value there published v2.5.0 early: its
// flatpak_only re-run passed `draft: ${{ !inputs.flatpak_only }}` (false)
// against a release that was still a draft. Every upload must instead keep
// the state create-release found the release in.
describe("release.yml: asset uploads preserve the release's existing draft state", () => {
  it("create-release exposes the release's draft state as an output", () => {
    const createRelease = extractJobBlock(workflow, "create-release");
    expect(createRelease).toMatch(/draft:\s*\$\{\{\s*steps\.release\.outputs\.draft\s*\}\}/);
    expect(createRelease).toMatch(/draft=\$\(gh release view "\$TAG" --json isDraft -q \.isDraft\)/);
  });

  it("every draft/releaseDraft setting passes through create-release's draft output", () => {
    const settings = [...workflow.matchAll(/^\s*(draft|releaseDraft):\s*(.+)$/gm)]
      .filter(([, , value]) => !value.includes("steps.release.outputs.draft"));
    expect(settings.length, "expected the tauri-action, MSIX, and Flatpak uploads to set draft state").toBeGreaterThanOrEqual(3);
    for (const [line, , value] of settings) {
      expect(value.trim(), `hardcoded draft state: ${line.trim()}`).toBe("${{ needs.create-release.outputs.draft }}");
    }
  });
});
