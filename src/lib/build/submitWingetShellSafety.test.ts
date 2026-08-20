// @vitest-environment node
//
// Regression test for a CodeQL-flagged shell-command-injection risk in
// `scripts/submit-winget.ts` (fixed in aa542c6/4eeeb64): `fetchReleaseInfo`
// built `gh release view`/`gh release download` commands via `execSync`
// with the release tag and asset name interpolated straight into a shell
// string. The tag comes from a CLI argument and the asset name comes back
// from `gh release view`'s own JSON output — neither is validated against
// shell metacharacters, so a crafted release/tag name could break out of
// the intended command. The fix switched both calls to `execFileSync` with
// an argument array, which never reaches a shell at all.
//
// This script isn't structured to export `fetchReleaseInfo` for a
// behavioral test without a live `gh` binary and network access, so this
// is a static-source regression check instead: it reads the actual
// `fetchReleaseInfo` function body and asserts the shell-injection shape
// (`execSync` called with a template string built from `tag`/`exeAsset`/
// `scratchDir`) is absent. It fails immediately if that pattern is
// reintroduced, without needing to invoke `gh` itself.
import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const scriptPath = path.resolve(__dirname, "../../../scripts/submit-winget.ts");
const source = fs.readFileSync(scriptPath, "utf-8");

function extractFunctionBody(src: string, functionName: string): string {
  const start = src.indexOf(`function ${functionName}(`);
  expect(start, `could not find function ${functionName} in ${scriptPath}`).toBeGreaterThan(-1);
  // Walk braces from the first `{` after the signature to find the matching close.
  const openBrace = src.indexOf("{", start);
  let depth = 0;
  for (let i = openBrace; i < src.length; i++) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") {
      depth--;
      if (depth === 0) return src.slice(start, i + 1);
    }
  }
  throw new Error(`unbalanced braces scanning ${functionName}`);
}

describe("submit-winget.ts: gh release commands never reach a shell", () => {
  const fetchReleaseInfo = extractFunctionBody(source, "fetchReleaseInfo");

  it("uses execFileSync (not execSync) for the gh release calls", () => {
    // The two gh invocations (release view + release download) must both be execFileSync calls.
    const ghCallCount = (fetchReleaseInfo.match(/execFileSync\(\s*"gh"/g) ?? []).length;
    expect(ghCallCount, "expected both gh calls (release view + release download) to use execFileSync").toBe(2);
  });

  it("never builds a gh execSync command via string interpolation of tag/asset input", () => {
    // The historical bug: execSync(`gh release view ${tag} ...`) / execSync(`gh release download ${tag} -p "${exeAsset}" ...`).
    // Match any execSync(` ... gh ... ${ ... } ... `) template-literal call.
    const shellInterpolatedGhCall = /execSync\(\s*`[^`]*\bgh\b[^`]*\$\{/;
    expect(
      shellInterpolatedGhCall.test(fetchReleaseInfo),
      "found a gh command built via execSync template-literal interpolation — this is exactly the shell-injection shape fixed in aa542c6"
    ).toBe(false);
  });

  it("passes the release tag and asset name as separate argv entries, not embedded in a command string", () => {
    // execFileSync("gh", [...]) — the tag/asset must appear as their own
    // array elements (e.g. `tag,` or `exeAsset,`), never inside a backtick
    // string that also contains "gh".
    expect(fetchReleaseInfo).toMatch(/execFileSync\(\s*"gh",\s*\[/);
  });
});
