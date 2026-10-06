# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

The imported `AGENTS.md` above is the canonical instructions file for this repo (commands, architecture, invariants, workflow, design conventions) — it is kept up to date and applies to Claude Code equally. Everything below is Claude-Code-specific and supplements it.

## Release Process
- If a release is being initiated from a Cloud Environment (not the user's local machine), STOP immediately and notify the user — cutting a release must be done from a local environment.
- Tag pushes are protected on this repo: Claude cannot push `v*` tags (GitHub returns 403). When a release tag is needed, prepare everything else, then STOP and give the user the exact command to run (e.g. `git tag -a v1.0.0 -m 'Luminous v1.0.0' && git push origin v1.0.0`).
- Microsoft Store submission is **not** part of this repo's release pipeline — it lives in the
  separate private repo `esoltys/luminous-store` (cloned locally at `~/source/luminous-store`),
  triggered manually (`gh workflow run publish-store.yml -f tag=vX.Y.Z`) against a `luminous`
  release tag once it's published here. `luminous`'s own release is done once the four items
  below hold; Store certification is tracked and verified separately in `luminous-store`.

### Definition of Done (releases)
A release is only complete when **all three** of the following hold. Never report a release as done based on CI status alone.
1. The release GitHub Actions workflow is green.
2. The pushed tag matches the version in `package.json`/`Cargo.toml`.
3. The GitHub release has the expected artifacts attached (including a `.msix`/`.msixbundle` for
   Store submission and a `.flatpak` bundle).
4. The `.flatpak` bundle is attached to the release AND the shared OSTree repo at
   `esoltys.dev/flatpak/` has been updated with the new ref (see
   [docs/FLATPAK.md](docs/FLATPAK.md)) — unless the shared-repo secrets aren't bootstrapped yet,
   in which case the publish step skips by design and this item doesn't block the release.

## CI Monitoring
- Use `gh run watch <run-id> --exit-status` to monitor a run instead of polling with repeated `gh run list`/API calls. Only fall back to scheduled re-checks for waits expected to exceed 15 minutes.
- Track the last reported run ID/conclusion and do not re-report a result that was already surfaced to the user.

## Workflows
- This repo's `.github/workflows/` must not have overlapping triggers: check for duplicate `on:` conditions (e.g. both `push: tags` and `release: published`) before adding or editing a workflow.

## GitHub issue/PR body formatting
- GitHub renders a single `\n` within a paragraph as a hard line break (not CommonMark's soft-wrap-to-space behavior). Never hand-wrap prose paragraphs in an issue/PR body at ~80-100 cols like source code — write each paragraph and each bullet as one unwrapped line, or the rendered body shows spurious mid-sentence line breaks.
- Epic issues: do not list sub-issues in the description body — GitHub automatically shows them below the description if they are properly added as sub-issues (`gh issue edit <epic-id> --add-sub-issue <ids>`). If there are no related issues, leave the `## Related` section out entirely.
- Do not add Claude Code attribution to issues, PRs, or commit messages: no "Generated with Claude Code" line, no `Co-Authored-By: Claude` or `Claude-Session:` trailers.

## Claude Code specifics

- Use the tools in this harness (Read/Edit/Grep/Glob) instead of shelling out to `cat`/`sed`/`grep`/`find`.
- Running `bun run tauri dev` directly is fine. Check first that another instance isn't already running (`ps aux | grep LuminousMusicPlayer`) — this repo uses `tauri-plugin-single-instance`, and launching a second one while the user has their own session up can tear down their running instance instead of just being rejected.
- **Default for any UI check (layout, overflow, console errors, live DOM/CSS, locale text length): `bun run tauri dev`, then attach the Browser pane to `http://127.0.0.1:9222`** (dev mode enables remote devtools automatically) — see [docs/TESTING.md](docs/TESTING.md)'s "Remote devtools for headless/agent debugging". Check `ps aux | grep LuminousMusicPlayer` first. Don't open the bare Vite page on :1420: without the Tauri backend it renders blank. Drive the UI and screenshot it yourself — don't rely solely on non-visual checks (unit tests, type checks) before handing off, and don't ask the user to describe what they see.
- `bunx tsx scripts/inspect-app.ts` (Windows only) is the fallback, not the default: it needs its own debug build plus `tauri-driver` + `msedgedriver` (setup in [docs/TESTING.md](docs/TESTING.md)). Reach for it only when devtools can't do the job, e.g. real OS-level clicks/hover through WebDriver. For pixel-level detail it can crop with `screenshot <path> --css/--text <sel>`; its subcommands are in its file header.
- This project's dedicated worktree convention (`.claude/worktrees/` for Claude, `.worktrees/<name>/` for other assistants) is documented in AGENTS.md under Version Control — follow it for any bug/feature work.

## Scope Control

- Match the size of the edit to the size of the request. For README/docs changes, propose the diff in chat before writing if it exceeds ~10 lines.
- Do not add explanatory prose, extra sections, or intermediary files that were not asked for.
