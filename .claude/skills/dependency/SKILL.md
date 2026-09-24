---
name: dependency
description: Consolidate the week's open Dependabot PRs (npm + cargo) into a single verified PR against main
---

Consolidate open Dependabot PRs into one PR. With no arguments, include every open Dependabot
PR. If $ARGUMENTS lists PR numbers, include only those PRs. Tell the user which open PRs were
left out.

1. **Collect the PRs**:
   `gh pr list --author app/dependabot --json number,title,headRefName,files --jq '.[] | "\(.number)\t\(.title)\t\([.files[].path]|join(","))"'`.
   If there are none, or none of the listed numbers are open Dependabot PRs, run step 6's
   known-blocker check before stopping. If that finds nothing to do, say so and stop.
   Note which files each PR touches: a PR that only touches `bun.lock` (e.g.
   `@tauri-apps/*` declared as `^2`) is a lockfile-only bump — keep the `package.json` range
   as-is for those.
2. **Prep the branch**: `git fetch origin`, confirm the worktree is clean
   (`git status --porcelain`), and branch from (or `git reset --hard` this session's worktree
   branch onto) `origin/main`. See the `issue` skill for why this session's existing worktree is
   reused rather than creating a new one.
3. **Apply the npm bumps with bun** (`bun.lock` is the only JS lockfile — Dependabot uses the `bun`
   ecosystem and CI runs `bun install --frozen-lockfile`; don't recreate `package-lock.json`):
   - `bun add <pkg>@^<new>` for dependencies, `bun add -d <pkg>@^<new>` for devDependencies.
   - For lockfile-only bumps, restore the original range in `package.json` afterwards and run
     `bun install`; confirm the new version landed in `bun.lock`.
   - Respect the ignores in `.github/dependabot.yml` (e.g. no TypeScript major, see #522).
4. **Apply the cargo bumps**: edit `src-tauri/Cargo.toml`, then `cargo update -p <crate>` from
   `src-tauri/`. For breaking (0.x minor / major) bumps, grep for the crate's usages and read its
   changelog — adapt call sites in the same commit.
5. **Verify** (synchronously, one at a time):
   - `bun run check` (svelte-check + knip)
   - `bun run test:run`
   - `cd src-tauri && cargo test` and `cargo clippy --all-targets -- -D warnings`
6. **Drop bumps that can't land**: if a bump breaks a check and the fix isn't a small call-site
   update, revert just that package (restore its range, `bun install` / `cargo update -p`), re-run
   the checks, and record why it was excluded. Known blocker: **vitest 5** breaks
   `@testing-library/jest-dom@7.0.1`'s type augmentation (`toBeInTheDocument` missing on
   `Assertion`, ~470 `bun run check` errors). Check for a newer jest-dom every run
   (`bun pm view @testing-library/jest-dom versions`), even when no vitest PR is open. The held-back
   Dependabot PR is closed, so Dependabot only offers the *next* vitest release. If jest-dom has
   a new release, add `bun add -d vitest@^5 @testing-library/jest-dom@latest` to this batch by
   hand and verify. If it passes, remove this blocker note from the skill in the same PR.
7. **Commit** one `chore(deps): consolidate dependabot bumps` commit. The body lists what was
   bumped (with the Dependabot PR numbers), any code changes a breaking bump required, and any
   excluded PR with the reason.
8. **Open the PR** against `main` using `.github/PULL_REQUEST_TEMPLATE.md` (unwrapped
   paragraphs). Summary lists every bump and PR number; Implementation Notes cover code changes
   and exclusions; Test Plan reports the actual check/test counts.
9. **Merge** only once `gh pr checks <pr> --watch` shows every check concluded and passed, then
   `gh pr merge <pr>` (per AGENTS.md). Dependabot auto-closes the superseded PRs after the merge
   lands on `main`. Confirm with the step 1 command. Close each excluded PR with a comment
   explaining why it was held back (`gh pr close <pr> --comment "..."`). Dependabot then skips
   that version and opens a new PR when the next one is released. Never reply
   `@dependabot ignore this major/minor version`, because that also hides the later release
   that fixes the blocker.
