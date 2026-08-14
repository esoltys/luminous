# Release Checklist

Steps to work through before cutting a Luminous release. `bun run release <version>`
(see [`scripts/release.ts`](../scripts/release.ts)) automates version bump + `bun run
check` + `bun run test:run` + `cargo check` + commit + tag, but everything below it is
manual.

## Content

- [ ] Update the user manual (`docs/user-guide/luminous-user-guide-*.dc.html`) for any
      user-facing changes. These `.dc.html` files are externally managed — don't hand-edit
      them directly; regenerate/export them the usual way.
- [ ] Regenerate screenshots for any changed views:
  ```bash
  bun run take-screenshots
  ```
  Use `--name=<entry>` (e.g. `bun run take-screenshots --name=equalizer`) to capture just
  one view instead of the whole suite. New screenshot-worthy features get an entry added
  to the tracked `scripts/mock-config.json` (see `.claude/CLAUDE.md` for the harness
  details). Read the resulting PNGs to confirm they rendered correctly before committing.
- [ ] Write `docs/release-notes/vX.Y.Z.md` for the new version.

## Verification

- [ ] `bun run check` — svelte-check / TypeScript
- [ ] `bun run test:run` — frontend unit tests
- [ ] `cargo test` (from `src-tauri/`) — Rust unit tests + BDD suites
- [ ] `cargo clippy --all-targets` (from `src-tauri/`) — no warnings at all, not just no
      new ones; fix any pre-existing warnings you encounter rather than leaving them
- [ ] Run the smoke test — real audio device, real playback, tags, playlists, equalizer:
  ```bash
  cargo test --test smoke_test -- --ignored --nocapture
  ```
  (run from `src-tauri/`)
- [ ] Manually exercise the app from a dev build (`bun run tauri dev`): import/scan a
      folder, play/pause/seek/volume, create a playlist, edit tags, check the equalizer,
      and anything specific to what changed this release.
- [ ] Check the [Security Audit](../.github/workflows/audit.yml) and
      [CodeQL](../.github/workflows/codeql.yml) workflows are green on `main`.

## Cut the release

- [ ] `bun run release <version>` to bump the version, run checks, commit, and tag
      locally — then `bun run release <version> --push` (or push manually) to trigger
      [`release.yml`](../.github/workflows/release.yml), which builds signed Linux +
      Windows bundles and drafts a GitHub release.
- [ ] Watch the GitHub Actions run to completion for **both** platforms
      (`gh run watch`, or the Beeper notification from `release.ts` if configured).

## Post-build

- [ ] Edit the draft GitHub release: replace the auto-generated body with the contents of
      `docs/release-notes/vX.Y.Z.md`. Before publishing, toggle "Create a discussion for
      this release" so it also posts as an Announcement in GitHub Discussions, then
      publish it (the workflow creates it as a draft with `prerelease: false`, so it's
      just sitting there until published).
- [ ] Download and install the new build on at least one real machine per platform
      (Windows + Linux) — don't just trust the CI build succeeded.
- [ ] Verify the in-app updater picks up the new release from an older installed version
      (the app checks for updates on launch — confirm the prompt/flow actually works end
      to end, not just that the build has `createUpdaterArtifacts` set).
- [ ] Close/link the GitHub issues resolved by this release; update the milestone if one's
      in use.
