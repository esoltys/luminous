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
      not require an issue and do not need to be listed in the release notes.) A commit
      title often shows only the PR number, so read `Fixes #N` in the PR body
      (`gh pr view N --json body`) before concluding a fix has no issue.
- [ ] The release-notes source is the milestone, not the log:
      `gh issue list --milestone X.Y --state all --limit 300 --json number,title,labels`.
      Treat a PR carrying the milestone as a mistake (milestones are for issues only).
- [ ] Check that third-party integrations (Last.fm, MusicBrainz, Files.fm) still work with
      the keys the build ships. 4.0 found late that Last.fm requires a Pro subscription for
      app keys.

## Content

- [ ] Update the user manual (`docs/user-guide/luminous-user-guide-{EN,FR,DE,ES,IT,RU,UK}.html`) for any
      user-facing changes. Edit every guide in the same change, and quote button and menu
      names exactly as that locale's catalog shows them (see [TRANSLATIONS.md](TRANSLATIONS.md)).
- [ ] Regenerate screenshots, with your own Luminous closed. Run Capture Studio in `esoltys/luminous-store`:
  ```bash
  cd ../luminous-store
  bun run studio
  ```
  Execute the guide recipes (`guide-*`) targeting the Luminous user guide directory (`docs/user-guide/assets/<tag>/screenshots/{light,dark}/`). Review the resulting PNGs before committing — at least en-CA, en-US and one Cyrillic locale (ru or uk). After replacing or updating any PNG, run `bun run guide-webp`; `scripts/guide-assets.test.ts` fails if the embedded set exceeds its size budget, and `bun scripts/guide-manifest.ts` (or `bun run sync-docs`) verifies all assets match the `docs/user-guide/assets/manifest.json` contract.
  Capture pitfalls (all hit in 4.0):
  - The Home greeting and the pinned Moment Mix name depend on the hour. Start the capture
    just after a bucket boundary (05:00, 12:00, 17:00, 21:00) and keep it inside one bucket,
    then montage-crop Home across every locale (ImageMagick) and check the greeting and mix
    name agree. Check the first scene of the first locale too: a cold start can catch
    "Loading your library…".
  - Build the capture exe with `CARGO_TARGET_DIR=target` so it is traceable (the shared
    target dir can link another worktree's stale engine crates). `--out` must be shaped like a
    luminous checkout.
  - Before changing a scene's height, grep the scenes for every match. A blanket `sed` on
    `tall(1500)` also resized the equalizer scene and broke the `sync-docs` manifest check.
  - A capture build leaves an ignored `build/` dir, which makes `release-gating.test.ts` fail
    inside `bun run release`. Delete `build/` after the capture.
  - A settings screen that shows an account handle (Integrations) is cropped at its manifest
    height; raise the scene and the manifest together when a card is cut off.
- [ ] Write `docs/release-notes/vX.Y.Z.md` for the new version. Chores do not need to be listed in the release notes.
- [ ] Prepare the Microsoft Store listing in `esoltys/luminous-store` (its own PR there):
      - Render Store screenshots and artwork via Capture Studio (`bun run studio`), which drives the real app and compiles marketing composites across all supported Store languages into `assets/<locale>/screenshots/` and `assets/<locale>/artwork/`.
      - Update `ReleaseNotes` in every language column of `listing.csv`, and check that
        `Description`, `Feature1..N` and the screenshot captions (one per shot, in shot order)
        still match what ships. The `en-us` column uses US spelling.
      - Dry-run `ConvertTo-Pdp.ps1` + `New-SubmissionPackage` (that repo's README, "Testing
        changes").

## Verification

- [ ] A fresh release worktree has no `node_modules`: run `bun install --frozen-lockfile`
      first. The suite below can then run as one background script that logs to a file.
- [ ] `bun run check` — svelte-check / TypeScript
- [ ] `bun run lint:ast` — architecture-invariant rules
- [ ] `bun run test:run` — frontend unit tests, including locale completeness and the
      guide-asset size budget
- [ ] `cargo test --workspace` (from repo root, so `crates/` members run too) — Rust unit and integration tests
- [ ] `bun run fmt:check` (or `cargo fmt --all --check`), `bun run audit:cargo` (or `cargo deny check`), and `cargo clippy --all-targets -- -D warnings` (from
      `src-tauri/`) — no warnings at all, not just no new ones; fix any pre-existing
      warnings you encounter rather than leaving them
- [ ] In `bun run tauri dev`, check this release's new UI in the longest locales (de, ru)
      for overflow or truncation. Before launching:
  - Tell the user a window is about to open and to leave it alone (in 4.0 it was closed,
    and the check had to restart).
  - Run it on a COPY of the real data dir (`LUMINOUS_DATA_DIR` plus
    `WEBVIEW2_USER_DATA_FOLDER`), never the real profile: migrations are one-way. The real
    data is in `%APPDATA%\39231EricJamesSoltys.LuminousMusicPlayer`.
  - Use `CARGO_TARGET_DIR=target`, and check port 1420 is free
    (`Get-NetTCPConnection -LocalPort 1420`): another worktree's leftover Vite makes
    `beforeDevCommand` fail. Check the process's command line before killing it.
- [ ] Check local-time behaviour on the real machine: the Home greeting, the Moment Mix name
      and the Daypart buckets must agree. 4.0 found that jiff could not map a new Windows
      time zone and the backend silently used UTC.
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
  release. Scan times drift with the drive and library state (3.2.0's first scan was 2.2 s
  one day and 116 s three weeks later), so re-run the previous release's baseline the SAME
  DAY (a baseline worktree built with `CARGO_TARGET_DIR=target`, passed with `--exe`) before
  reading a scan-time delta as a regression. Doing so caught the 4.0 forced-rescan
  regression (#2160). The capture and release exes overwrite each other in a shared target
  dir; the session cannot remove its own baseline worktree on Windows, so hand the
  `git worktree remove` to the user.
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
      `docs/release-notes/vX.Y.Z.md`, then publish it (the workflow creates it as a
      draft with `prerelease: false`, so it's just sitting there until published).
- [ ] Microsoft Store submission is a separate, manual step in the private
      `esoltys/luminous-store` repo (not part of this repo's pipeline). Publish the GitHub
      release FIRST: the workflow downloads the `.msix` with `gh release download`, which
      cannot see a draft ("release not found"). Then trigger it there:
  ```bash
  gh workflow run publish-store.yml -f tag=vX.Y.Z
  ```
  Add `-f update_screenshots=true` whenever the Store listing prep above changed anything —
  without it the new screenshots and release notes never reach the Store (it replaces the
  whole live listing; see that repo's `README.md`). Watch this run too, not just the build — a green run
  stages the draft submission in Partner Center (`PendingCommit`).
- [ ] Commit the staged submission into certification. `publish-store.yml` deliberately stops at an
      uncommitted draft to ensure human review. To commit it (initiating Microsoft certification), run:
  ```powershell
  $global:SBDisableTelemetry = $true; Import-Module StoreBroker
  $cred = New-Object System.Management.Automation.PSCredential($env:STORE_CLIENT_ID, (ConvertTo-SecureString $env:STORE_CLIENT_SECRET -AsPlainText -Force))
  Set-StoreBrokerAuthentication -TenantId $env:STORE_TENANT_ID -Credential $cred
  Complete-ApplicationSubmission -AppId $env:STORE_APP_ID -SubmissionId <submissionId>
  ```
  (or click **Submit to the Store** in the Partner Center dashboard).
- [ ] Check actual certification status from `~/source/luminous-store` (set
      `STORE_TENANT_ID`/`STORE_CLIENT_ID`/`STORE_CLIENT_SECRET`/`STORE_APP_ID` as local env
      vars first, from the values used to create that repo's GitHub secrets):
  ```powershell
  ./Get-StoreSubmissionStatus.ps1
  ```
  Confirm `status` moves from `CommitStarted` / `Certification` to `Published` (or check `statusDetails`
  for errors if it doesn't). This is what actually satisfies "the Microsoft Store
  submission went through."
- [ ] Download and install the new build on at least one real machine per platform
      (Windows + Linux) — don't just trust the CI build succeeded.
- [ ] Confirm the `.flatpak` bundle is attached to the release, and that
      `flatpak update org.luminous.music` (with the `esoltys` remote already added — see
      [docs/FLATPAK.md](FLATPAK.md)) picks up the new version from the shared repo. The ref
      is `app/org.luminous.music/x86_64/master` (not `stable`); check it returns 200 at
      `https://esoltys.dev/flatpak/repo/refs/heads/app/org.luminous.music/x86_64/master`.
- [ ] Refresh the landing page (esoltys.dev/luminous, EN and FR) with the landing-page
      skill, copying screenshots from `origin/main`'s `docs/user-guide/assets` (not a
      checkout that predates the recapture), and verify the live Home image shows the
      same greeting and Moment Mix as the guide.
- [ ] Verify the in-app updater picks up the new release from an older installed version
      (the app checks for updates on launch — confirm the prompt/flow actually works end
      to end, not just that the build has `createUpdaterArtifacts` set).
- [ ] Update the [opendesktop.org listing](https://www.opendesktop.org/c/2370163/) with the
      new `.deb`/`.rpm` (via "Add URL" pointing at the GitHub release assets, not a
      re-upload) and a changelog entry. This is a manual listing with no automated sync —
      easy to let it silently go stale if skipped.
- [ ] Close/link the GitHub issues resolved by this release; update the milestone if one's
      in use.
