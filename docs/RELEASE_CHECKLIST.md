# Release Checklist

Steps to work through before cutting a Luminous release. `bun run release <version>`
(see [`scripts/release.ts`](../scripts/release.ts)) automates version bump + `bun run
check` + `bun run test:run` + `cargo check` + commit + tag, but everything below it is
manual.

## Scope

- [ ] Enumerate what's landed since the last tag: `git log <last-tag>..main --oneline`.
      Confirm every user-facing fix has a linked, `bug`-labeled GitHub issue — PRs
      occasionally merge without one (e.g. #433 shipped before #453 existed to track it).
      Create one retroactively and link the PR if it's missing, so the release notes and
      the post-build issue-closing step below have something to point at. (Chore PRs do
      not require an issue and do not need to be listed in the release notes.)

## Content

- [ ] Update the user manual (`docs/user-guide/luminous-user-guide-{EN,FR,DE,ES,IT,RU,UK}.html`) for any
      user-facing changes. Edit every guide in the same change, and quote button and menu
      names exactly as that locale's catalog shows them (see [TRANSLATIONS.md](TRANSLATIONS.md)).
- [ ] Regenerate screenshots, with your own Luminous closed. Library scenes come from a copy of
      your real database (real stats, pins and playlists); the first-run scenes need an empty
      profile, scanned from a music folder:
  ```bash
  bun run take-scene-screenshots --clone-profile
  ```
  ```bash
  bun run take-scene-screenshots --library <music-folder> --stage=fresh
  ```
  This drives the real app at 1440×900 (setup in [TESTING.md](TESTING.md)'s "Scene
  screenshots") and captures every shipped locale into
  `docs/user-guide/assets/<tag>/screenshots/{light,dark}/`. A full run takes about 25
  minutes, so capture one locale first (`--locale=en-US`) and review it; `--name=<scene>`
  narrows further. Don't edit files in this checkout while it runs: Vite reloads the page and
  the run fails. `bun run take-screenshots` (mocked IPC, `scripts/screenshot-scenes.json`)
  covers views that only exist there. After replacing a PNG by hand, run
  `bun run guide-webp`; `scripts/guide-assets.test.ts` fails if the embedded set exceeds
  its size budget. Read the resulting PNGs before committing — at least en-CA, en-US and
  one Cyrillic locale (ru or uk).
- [ ] Write `docs/release-notes/vX.Y.Z.md` for the new version. Chores do not need to be listed in the release notes.
- [ ] Prepare the Microsoft Store listing in `esoltys/luminous-store` (its own PR there):
      - Build the 1366×768 composites (wallpaper, headline, window) from the dark captures
        above, which include the Store-only `store-*` scenes, with
        `./New-StoreScreenshots.ps1 -GuideAssets <luminous>/docs/user-guide/assets` in that
        repo. Its `store-screenshots.json` lists the shots, their scenes and the headline per
        language. Each Store language has one source locale: `en-us` ← `en-US`, `fr` ←
        `fr-CA` (the `fr` listing serves every French market, written in Canadian French), and
        `de`, `es`, `it`, `ru`, `uk` ← the same tag. Read a few results before committing.
      - Update `ReleaseNotes` in every language column of `listing.csv`, and check that
        `Description`, `Feature1..N` and the screenshot captions (one per shot, in shot order)
        still match what ships. The `en-us` column uses US spelling.
      - Dry-run `ConvertTo-Pdp.ps1` + `New-SubmissionPackage` (that repo's README, "Testing
        changes").

## Verification

- [ ] `bun run check` — svelte-check / TypeScript
- [ ] `bun run lint:ast` — architecture-invariant rules
- [ ] `bun run test:run` — frontend unit tests, including locale completeness and the
      guide-asset size budget
- [ ] `cargo test` (from `src-tauri/`) — Rust unit tests + BDD suites
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` (from
      `src-tauri/`) — no warnings at all, not just no new ones; fix any pre-existing
      warnings you encounter rather than leaving them
- [ ] In `bun run tauri dev`, check this release's new UI in the longest locales (de, ru)
      for overflow or truncation.
- [ ] Do the manual QA walkthrough in [docs/TESTING.md](TESTING.md) (real-hardware smoke
      test + dev-build exercise), plus anything specific to what changed this release.
- [ ] Compare memory against the previous release on Windows (from a local machine, with
      Luminous closed and a track over 2.5 minutes loaded in the player). Pass the
      upcoming version, since the bump hasn't happened yet at this point:
  ```bash
  bun run tauri build --no-bundle
  bun run perf:memory -- --app-version X.Y.Z
  bun run perf:chart -- --baseline <previous> --candidate X.Y.Z
  ```
  Add the new rows, chart and delta table to [docs/PERFORMANCE.md](PERFORMANCE.md).
  Look into any private-bytes change beyond ~3% (run-to-run noise) before cutting the
  release.
- [ ] Check the [Security Audit](../.github/workflows/audit.yml) and
      [CodeQL](../.github/workflows/codeql.yml) workflows are green on `main`.

## Cut the release

- [ ] **If this release is being run from a Cloud Environment (not a local machine),
      stop here and notify the user.** Cutting a release requires local execution —
      don't proceed with the steps below from a cloud/remote environment.
- [ ] `bun run release <version>` to bump the version, run checks, commit, and tag
      locally on the release worktree branch. Don't pass `--push` — that pushes the
      branch straight to `main`, bypassing branch protection and skipping review.
- [ ] Open a PR from the release branch into `main` and get it reviewed/merged like any
      other change. If the release only bundles fixes already merged to `main` (a
      version-bump-only PR), list the issues/PRs it covers in the description so the
      diff — just version files + release notes — has context.
- [ ] After the PR merges, `git checkout main && git pull` so `main` includes the merge
      commit, then re-tag `main` at that commit (the local tag from `bun run release`
      still points at the old worktree-branch commit, which is not `main`'s tip once
      merged):
  ```bash
  git tag -d vX.Y.Z
  git tag -a vX.Y.Z -m "Release vX.Y.Z"
  git push origin vX.Y.Z
  ```
  Pushing the tag (not `main` itself) is what triggers
  [`release.yml`](../.github/workflows/release.yml), which builds signed Linux + Windows
  bundles and drafts a GitHub release.
- [ ] Watch the GitHub Actions run to completion for **all** platforms, including the
      `flatpak-build` job (`gh run watch`, or the Beeper notification from `release.ts`
      if configured).

## Post-build

- [ ] Edit the draft GitHub release: replace the auto-generated body with the contents of
      `docs/release-notes/vX.Y.Z.md`. Before publishing, toggle "Create a discussion for
      this release" so it also posts as an Announcement in GitHub Discussions, then
      publish it (the workflow creates it as a draft with `prerelease: false`, so it's
      just sitting there until published).
- [ ] Microsoft Store submission is a separate, manual step in the private
      `esoltys/luminous-store` repo (not part of this repo's pipeline). After publishing,
      trigger it there:
  ```bash
  gh workflow run publish-store.yml -f tag=vX.Y.Z
  ```
  Add `-f update_screenshots=true` whenever the Store listing prep above changed anything —
  without it the new screenshots and release notes never reach the Store (it replaces the
  whole live listing; see that repo's `README.md`). Watch this run too, not just the build — a green run
  only means the submission was committed for certification, not that it's live yet;
  Microsoft's cert pass still has to complete.
- [ ] Check actual certification status from `~/source/luminous-store` (set
      `STORE_TENANT_ID`/`STORE_CLIENT_ID`/`STORE_CLIENT_SECRET`/`STORE_APP_ID` as local env
      vars first, from the values used to create that repo's GitHub secrets):
  ```powershell
  ./Get-StoreSubmissionStatus.ps1
  ```
  Confirm `status` moves from `Certification` to `Published` (or check `statusDetails`
  for errors if it doesn't). This is what actually satisfies "the Microsoft Store
  submission went through" — a green `publish-store.yml` run only means the submission
  was committed, not certified/live.
- [ ] Download and install the new build on at least one real machine per platform
      (Windows + Linux) — don't just trust the CI build succeeded.
- [ ] Confirm the `.flatpak` bundle is attached to the release, and that
      `flatpak update org.luminous.music` (with the `esoltys` remote already added — see
      [docs/FLATPAK.md](FLATPAK.md)) picks up the new version from the shared repo.
- [ ] Verify the in-app updater picks up the new release from an older installed version
      (the app checks for updates on launch — confirm the prompt/flow actually works end
      to end, not just that the build has `createUpdaterArtifacts` set).
- [ ] Update the [opendesktop.org listing](https://www.opendesktop.org/c/2370163/) with the
      new `.deb`/`.rpm` (via "Add URL" pointing at the GitHub release assets, not a
      re-upload) and a changelog entry. This is a manual listing with no automated sync —
      easy to let it silently go stale if skipped.
- [ ] Close/link the GitHub issues resolved by this release; update the milestone if one's
      in use.
