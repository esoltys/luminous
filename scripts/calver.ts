// Luminous versions are calendar versions: YYYY.M.PATCH, e.g. 2026.11.0.
// - M is never zero-padded (`2026.1.0`, not `2026.01.0`): Cargo parses the
//   version as SemVer, which rejects leading zeros.
// - PATCH counts releases within the UTC calendar month, starting at 0.
// The MSIX manifest needs four parts; tauri-windows-bundle appends `.0`.
//
// The 4.x releases shipped in November 2026 count as that month's first
// releases (4.0.0 = 2026.11.0, 4.1.0 = 2026.11.1) but have no CalVer tags,
// which is why the next version also considers the current manifest version:
// it always holds the last released version, because the bump happens at
// release time.
import { execSync } from "child_process";
import { readFileSync } from "fs";
import { fileURLToPath } from "url";

export interface CalVer {
  year: number;
  month: number;
  patch: number;
}

const CALVER = /^(\d{4})\.([1-9]|1[0-2])\.(0|[1-9]\d*)$/;

/** Parses `YYYY.M.PATCH` (an optional leading `v` is allowed), or returns null. */
export function parseCalVer(version: string): CalVer | null {
  const m = CALVER.exec(version.replace(/^v/, ""));
  if (!m) return null;
  return { year: Number(m[1]), month: Number(m[2]), patch: Number(m[3]) };
}

export function formatCalVer(v: CalVer): string {
  return `${v.year}.${v.month}.${v.patch}`;
}

/**
 * The version the next release takes on `now` (read in UTC): one past the
 * highest PATCH this month among `tags` and `current`, or `YYYY.M.0` when
 * neither has a release this month. Non-CalVer entries (e.g. `v4.1.0`) are
 * ignored.
 */
export function nextCalVer(now: Date, tags: readonly string[], current: string): string {
  const year = now.getUTCFullYear();
  const month = now.getUTCMonth() + 1;
  let highest = -1;
  for (const candidate of [...tags, current]) {
    const v = parseCalVer(candidate);
    if (v && v.year === year && v.month === month) highest = Math.max(highest, v.patch);
  }
  return formatCalVer({ year, month, patch: highest + 1 });
}

/** Release tags (`v*`) known locally, after a best-effort fetch from origin. */
export function readReleaseTags(cwd: string): string[] {
  try {
    execSync("git fetch --tags --quiet origin", { cwd, stdio: "ignore" });
  } catch {
    console.warn("[calver] Could not fetch tags from origin; using local tags only.");
  }
  return execSync('git tag --list "v*"', { cwd, encoding: "utf-8" })
    .split("\n")
    .map((t) => t.trim())
    .filter(Boolean);
}

/**
 * Resolves the version to release: `requested` if given (it must be a valid
 * CalVer), otherwise the next one computed from today's UTC date, the repo's
 * tags and `current`.
 */
export function resolveReleaseVersion(
  requested: string | undefined,
  current: string,
  cwd: string,
): string {
  if (requested) {
    const v = parseCalVer(requested);
    if (!v) {
      throw new Error(
        `"${requested}" is not a YYYY.M.PATCH calendar version (e.g. 2026.11.0; no zero-padded month).`,
      );
    }
    return formatCalVer(v);
  }
  return nextCalVer(new Date(), readReleaseTags(cwd), current);
}

// `bun run version:next` prints the version the next release will take, for
// release notes and perf runs written before the bump.
if (import.meta.main) {
  const root = fileURLToPath(new URL("..", import.meta.url));
  const { version } = JSON.parse(readFileSync(`${root}/package.json`, "utf-8"));
  console.log(resolveReleaseVersion(undefined, version, root));
}
