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
   Store submission).

## CI Monitoring
- Use `gh run watch <run-id> --exit-status` to monitor a run instead of polling with repeated `gh run list`/API calls. Only fall back to scheduled re-checks for waits expected to exceed 15 minutes.
- Track the last reported run ID/conclusion and do not re-report a result that was already surfaced to the user.

## Workflows
- This repo's `.github/workflows/` must not have overlapping triggers: check for duplicate `on:` conditions (e.g. both `push: tags` and `release: published`) before adding or editing a workflow.

## GitHub issue/PR body formatting
- GitHub renders a single `\n` within a paragraph as a hard line break (not CommonMark's soft-wrap-to-space behavior). Never hand-wrap prose paragraphs in an issue/PR body at ~80-100 cols like source code — write each paragraph and each bullet as one unwrapped line, or the rendered body shows spurious mid-sentence line breaks.

## Claude Code specifics

- Use the tools in this harness (Read/Edit/Grep/Glob) instead of shelling out to `cat`/`sed`/`grep`/`find`.
- Do not run `bun run tauri dev` as a background task — it does not work as expected in this harness. Ask the user to run it themselves and verify manually (AGENTS.md's Version Control section covers the full worktree → walkthrough → approval → merge → close workflow).
- This project's dedicated worktree convention (`.claude/worktrees/` for Claude, `.worktrees/<name>/` for other assistants) is documented in AGENTS.md under Version Control — follow it for any bug/feature work.

## Scope Control

- Match the size of the edit to the size of the request. For README/docs changes, propose the diff in chat before writing if it exceeds ~10 lines.
- Do not add explanatory prose, extra sections, or intermediary files that were not asked for.
