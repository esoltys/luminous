---
name: release
description: Cut a Luminous release by working through docs/RELEASE_CHECKLIST.md
---

Cut release $ARGUMENTS.

[docs/RELEASE_CHECKLIST.md](../../../docs/RELEASE_CHECKLIST.md) is the source of truth — it's
actively maintained. Re-read it before starting. If anything below has drifted from it, the
checklist wins — follow the checklist, but also tell the user what changed and that this file
needs updating to match. Work through it in order:

0. **Cloud check**: if this is running in a cloud/remote environment (not the user's local
   machine), STOP now and tell the user — cutting a release must happen locally.
1. **Worktree check**: `pwd` and `git status --porcelain` — confirm we're in a dedicated release
   worktree (not the main checkout) and it's clean. Abort and tell the user if not. `git fetch
   origin`.
2. **Scope**: `git log <last-tag>..main --oneline`; confirm every user-facing fix has a linked,
   `bug`-labeled issue — file one retroactively (and link the PR) if it's missing.
3. **Content**: update the user manual (`docs/user-guide/luminous-user-guide-*.dc.html`) for any
   user-facing changes — these are externally managed, don't hand-edit them, regenerate/export the
   usual way. Regenerate screenshots for changed views (`bun run take-screenshots`, or
   `--name=<entry>` for just one) and read the resulting PNGs to confirm they're correct. Write
   `docs/release-notes/vX.Y.Z.md`.
4. **Verification**: `bun run check`, `bun run test:run`, `cargo test` (from `src-tauri/`),
   `cargo clippy --all-targets` (from `src-tauri/` — zero warnings, including pre-existing ones),
   and the manual QA walkthrough in `docs/TESTING.md` (real-hardware smoke test —
   `cargo test --test smoke_test -- --ignored --nocapture` from `src-tauri/` — plus a
   `bun run tauri dev` exercise of anything specific to this release). Compare memory against
   the previous release on Windows (Luminous closed, a track over 2.5 min loaded; pass the
   upcoming version since the bump hasn't happened yet):
   `bun run tauri build --no-bundle`, `bun run perf:memory -- --app-version X.Y.Z`,
   `bun run perf:chart -- --baseline <previous> --candidate X.Y.Z`; add the rows, chart and
   delta table to `docs/PERFORMANCE.md` and look into any private-bytes change beyond ~3%.
   Confirm the Security Audit and CodeQL workflows are green on `main`.
5. **Cut it**: `bun run release <version>` (no `--push` — that would push straight to `main` and
   bypass review) to bump the version, run its own checks, commit, and tag locally on the release
   branch.
6. **PR**: open a PR from the release branch into `main` using the repo PR template. NEVER push
   directly to `main`. If it's a version-bump-only PR, list the issues/PRs it covers in the
   description.
7. **STOP for the user**: tag pushes are protected — Claude gets a 403. Ask the user to review and
   merge the PR, then give them the checklist's re-tag sequence to run once merged (the local tag
   from step 5 points at the worktree branch, not `main`'s merge commit, so it must be deleted and
   recreated on `main` before pushing):
   ```bash
   git checkout main && git pull
   git tag -d vX.Y.Z
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin vX.Y.Z
   ```
8. **Watch the build**: once the user confirms the tag is pushed, `gh run watch <run-id>
   --exit-status` until **every** job concludes — both platform legs and `flatpak-build` — and
   report back on the signed Linux + Windows bundles and the `.flatpak`.
9. **Post-build**: help edit the draft GitHub release body in from `docs/release-notes/vX.Y.Z.md`,
   remind the user to toggle "Create a discussion for this release" before publishing, and walk
   through (don't run unprompted):
   - the Microsoft Store submission, which lives in the separate private `esoltys/luminous-store`
     repo (`gh workflow run publish-store.yml -f tag=vX.Y.Z` there), then its certification
     status via `./Get-StoreSubmissionStatus.ps1` from `~/source/luminous-store`
   - installing the build on a real machine per platform (Windows + Linux)
   - confirming the `.flatpak` is attached and `flatpak update org.luminous.music` picks up the
     new version from the shared repo (`docs/FLATPAK.md`)
   - verifying the in-app updater from an older installed version
   - updating the opendesktop.org listing (new `.deb`/`.rpm` via "Add URL" + changelog entry)
   - closing/linking the resolved GitHub issues and updating the milestone

Before calling the release done, confirm all four hold (CLAUDE.md's Definition of Done): the
release workflow is green, the pushed tag matches `package.json`/`Cargo.toml`, the GitHub
release has the expected artifacts (including a `.msix`/`.msixbundle` and a `.flatpak`), and
the shared OSTree repo at `esoltys.dev/flatpak/` has the new ref (skipped by design if the
shared-repo secrets aren't bootstrapped yet).
