# Human Touchpoints in the Pipeline

Everything here is a point where a human has to act — not just review — before the
pipeline can proceed. Grounded in what's actually in `.github/workflows/`,
`docs/RELEASE_CHECKLIST.md`, and `.github/store/README.md`. See [`docs/CI.md`](./CI.md)
for the full pipeline this fits into.

| Action | Why it needs a human | What the human must do |
|---|---|---|
| Pushing the release PR to `main` | `RELEASE_CHECKLIST.md` explicitly says not to pass `--push` to `bun run release` "that pushes the branch straight to `main`, bypassing branch protection and skipping review." Branch protection on `main` is asserted by the checklist's wording, not something visible in the workflow YAML itself. | Open a PR from the release worktree branch into `main` and get it reviewed/merged normally. |
| Pushing the `vX.Y.Z` tag | This repo's CLAUDE.md states tag pushes are protected and automated (Claude/CI) pushes of `v*` tags get a 403 from GitHub. Pushing the tag is also what triggers `release.yml`'s build, so it's a deliberate gate on when the expensive signed build kicks off, not just a permissions accident. | Run `git tag -d vX.Y.Z && git tag -a vX.Y.Z -m "Release vX.Y.Z" && git push origin vX.Y.Z` against `main`'s tip (after the release PR has merged) from a human's own credentials. |
| Watching the `release.yml` run to completion | The workflow builds signed Linux and Windows bundles across a matrix; a failure on either leg (e.g. the noted transient 503s from `tauri-apps/binary-releases`, retried up to 3x via `wretry.action` but not guaranteed to succeed) needs a human to notice and re-run or investigate. Nothing in the pipeline pages anyone automatically. | `gh run watch`, or the checklist's optional Beeper notification, until both matrix legs finish. |
| Editing and publishing the draft GitHub release | `release.yml` always creates the release as a **draft** (`gh release create ... --draft`, and `softprops/action-gh-release` is passed `draft: true` explicitly, with a comment noting that omitting `draft` would auto-publish). This is a deliberate content gate — the auto-generated release body is not the real release notes. | Replace the body with `docs/release-notes/vX.Y.Z.md`, optionally toggle "Create a discussion for this release," then click Publish in the GitHub UI. |
| Microsoft Store certification | Publishing the draft fires `publish-store.yml`, which submits the MSIX via StoreBroker's `Update-ApplicationSubmission -ReplacePackages`. A green run of that workflow only means the submission was **committed for certification** — the actual review/cert pass happens entirely on Microsoft's side and cannot be forced, sped up, or verified from GitHub Actions. | Watch the `publish-store.yml` run for success, then separately check the submission's status in Partner Center until Microsoft's certification completes and the update goes live. |
| Retrying a failed/stuck Store submission | `publish-store.yml` supports `workflow_dispatch` with a required `tag` input specifically for this case — re-running it doesn't rebuild anything, it just re-submits the already-uploaded MSIX. Choosing to do this requires human judgment about *why* the prior submission needs retrying (e.g. it was rejected, or needs re-submission after a `sbConfig.json` fix). | Trigger `publish-store.yml` manually via `workflow_dispatch`, supplying the release tag. |
| Manual `release.yml` dispatch | `release.yml` also accepts a bare `workflow_dispatch` with no inputs, which derives the tag from `package.json`'s version instead of a pushed tag ref. This bypasses the tag-push gate above, so it's a judgment call about when that's appropriate (the checklist doesn't document a scenario for using it). | Trigger `release.yml` manually via `workflow_dispatch` if needed; verify the resulting draft release/tag are correct since no tag was pushed to anchor them. |
| Installing and manually verifying the new build | The checklist requires downloading and installing the new build on at least one real machine per platform (Windows + Linux), and manually confirming the in-app updater picks up the release from an older install — neither is something CI does. | Download the published release assets, install on real hardware, exercise the app and the updater flow end to end. |
| Running the pre-release verification suite | None of `bun run check`, `bun run test:run`, `cargo test`, `cargo clippy --all-targets`, or the smoke test (`cargo test --test smoke_test -- --ignored --nocapture`) run in any GitHub Actions workflow — they are entirely manual steps in `RELEASE_CHECKLIST.md`. A PR can merge to `main` without any of these having run in CI. | Run each command locally before cutting a release, per the checklist; fix any pre-existing `cargo clippy` warnings encountered, not just new ones. |
| Closing/linking resolved GitHub issues | Not automated by any workflow; the checklist calls this out as a manual post-build step. | Close or link the issues resolved by the release and update the milestone if one's in use. |

## Secrets referenced in the workflows (existence not independently verified)

The workflow YAML *references* the following repository secrets. This document only
confirms they are referenced in the YAML — it does not and cannot confirm from the
workflow files alone that they are correctly configured, non-expired, or scoped
correctly in the repo's Settings → Secrets, since that's not visible from the checked-in
code:

- `GITHUB_TOKEN` — used throughout (`gh release` commands, `tauri-action`, MSIX upload);
  this one is provided automatically by GitHub Actions per run, not manually configured.
- `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — used by
  `tauri-action` in `release.yml` to sign the updater artifacts. If these are ever
  rotated, that's a manual credential-management action outside any workflow shown here.
- `STORE_TENANT_ID` / `STORE_CLIENT_ID` / `STORE_CLIENT_SECRET` / `STORE_APP_ID` — used
  by `publish-store.yml` to authenticate StoreBroker against the Microsoft Store
  submission API (an Azure AD app registration tied to the Partner Center account, per
  StoreBroker's own setup docs — not something this repo's YAML documents how to
  provision).

If any of these secrets are missing, expired, or wrong, the corresponding workflow will
fail at the step that consumes them; investigating that is itself a manual, human step
since there's no automated secret-health check in this pipeline.
