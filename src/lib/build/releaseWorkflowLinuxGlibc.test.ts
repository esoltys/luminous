// @vitest-environment node
//
// The AppImage (and .deb/.rpm) use the host's glibc, so their glibc floor is
// the Linux build runner's. v3.0.0 was built on ubuntu-24.04 and needed
// glibc 2.39: it didn't start on Ubuntu 22.04 / Debian 12 or in the AppImage
// catalog's test (#1700). These pin the runner, the guard that checks the
// result, and the linux_only re-run that replaced v3.0.0's Linux assets.
import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "../../..");
const workflow = fs.readFileSync(path.join(root, ".github/workflows/release.yml"), "utf-8");

describe("release.yml: Linux glibc floor", () => {
  it("builds the Linux installers on ubuntu-22.04, never a newer image", () => {
    const platform = workflow.match(/^\s*platform:\s*(.+)$/m)?.[1] ?? "";
    expect(platform).toContain("ubuntu-22.04");
    expect(platform).not.toMatch(/ubuntu-(2[4-9]|latest)/);
  });

  it("keys the Rust cache on the runner image", () => {
    expect(workflow).toMatch(/uses:\s*Swatinem\/rust-cache@\S+\s+with:\s+key:\s*\$\{\{\s*matrix\.platform\s*\}\}/);
  });

  it("checks the built AppImage against glibc 2.35", () => {
    expect(workflow).toMatch(/check-glibc-floor\.sh "\$\{APPIMAGES\[0\]\}" 2\.35/);
    expect(fs.existsSync(path.join(root, "scripts/check-glibc-floor.sh"))).toBe(true);
  });
});

describe("release.yml: linux_only and dry_run re-runs", () => {
  it("a linux_only run builds the release's tag; other runs keep the event's ref", () => {
    // A full dispatch for a new version has no git tag yet (a draft release
    // doesn't create one), so only a linux_only run or an explicit tag may
    // pin the checkout to it.
    expect(workflow).toMatch(
      /ref:\s*\$\{\{\s*inputs\.tag \|\| \(inputs\.linux_only && needs\.create-release\.outputs\.tag\) \|\| ''\s*\}\}/
    );
  });

  it("drop the Windows leg and the Flatpak job", () => {
    expect(workflow).toMatch(/\(inputs\.linux_only \|\| inputs\.dry_run\) && '\["ubuntu-22\.04"\]'/);
    expect(workflow).toMatch(/flatpak-build:[\s\S]*?if:[^\n]*!inputs\.linux_only && !inputs\.dry_run/);
  });

  it("linux_only refuses to create a new release", () => {
    expect(workflow).toMatch(/inputs\.linux_only \}\}" == "true" \]\]; then\s+echo "::error::/);
  });

  it("dry_run creates no release and gives tauri-action nothing to upload to", () => {
    // ...but still outputs a draft state: tauri-action rejects an empty
    // releaseDraft even when it uploads nothing.
    expect(workflow).toMatch(/inputs\.dry_run \}\}" == "true" \]\]; then\s+echo "draft=true" >> "\$GITHUB_OUTPUT"\s+exit 0/);
    expect(workflow).toMatch(/releaseId: \$\{\{ !inputs\.dry_run && needs\.create-release\.outputs\.release_id \|\| '' \}\}/);
    expect(workflow).toMatch(/tagName: \$\{\{ !inputs\.dry_run && needs\.create-release\.outputs\.tag \|\| '' \}\}/);
  });
});

describe("release.yml: AppImage zsync delta updates (#1703)", () => {
  it("installs zsync in system dependencies on Linux", () => {
    expect(workflow).toMatch(/apt-get install -y[^\n]*\bzsync\b/);
  });

  it("sets LDAI_UPDATE_INFORMATION with gh-releases-zsync pattern", () => {
    expect(workflow).toMatch(
      /LDAI_UPDATE_INFORMATION:\s*gh-releases-zsync\|esoltys\|luminous\|latest\|Luminous\.Music\.Player_\*_amd64\.AppImage\.zsync/
    );
  });

  it("sparse-checkouts fix-appimage-zsync.ts for older tag builds", () => {
    expect(workflow).toMatch(/sparse-checkout:\s*\|[\s\S]*?scripts\/fix-appimage-zsync\.ts/);
    expect(fs.existsSync(path.join(root, "scripts/fix-appimage-zsync.ts"))).toBe(true);
  });

  it("runs fix-appimage-zsync.ts before attaching or uploading assets", () => {
    expect(workflow).toMatch(/run:\s*bun run workflow-src\/scripts\/fix-appimage-zsync\.ts/);
  });

  it("includes *.zsync in dry_run artifact upload", () => {
    expect(workflow).toMatch(/path:\s*\|[\s\S]*?target\/release\/bundle\/appimage\/\*\.zsync/);
  });

  it("uploads *.zsync to release assets preserving draft state", () => {
    expect(workflow).toMatch(
      /Upload AppImage zsync to Release Assets[\s\S]*?draft:\s*\$\{\{\s*needs\.create-release\.outputs\.draft\s*\}\}[\s\S]*?target\/release\/bundle\/appimage\/\*\.zsync/
    );
  });
});

