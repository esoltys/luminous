# Luminous Music Player Rules

## Version Control

- Always initialize a git repository for new projects.
- Make atomic git commits at logical points (e.g., after completing each task phase, after scaffolding, after adding a major feature).
- Use conventional commit messages: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`.
- Stage all relevant files with `git add -A` before committing unless selective staging is needed.
- Proactively search and view GitHub issues using the `gh` command tool (e.g., `gh issue list` and `gh issue view <id>`) when asked to "fix a bug" or "work on a feature".
- When working on a bug or feature, always work in a dedicated git worktree. Do not merge the temporary branch or delete the worktree until the user has approved the changes.
- Present the Walkthrough (`walkthrough.md`) to the user and wait for their explicit feedback and approval before merging. When presenting the Walkthrough, also spin up the Tauri development server (`bun run tauri dev`) so the user can manually verify changes interactively instead of relying solely on screenshots.
- Only after the user has reviewed the Walkthrough and approved the changes may you merge the temporary branch, clean up (remove) the worktree, and update/comment on and close the relevant GitHub issues using the `gh` CLI tool. Note that an issue must not be closed until the corresponding changes have been successfully merged into the target branch.
- **Creating Issues**: When asked to create a bug/issue on GitHub:
  1. Inspect the relevant templates under `.github/ISSUE_TEMPLATE/` (e.g., `bug_report.md`, `feature_request.md`).
  2. Perform a codebase search or analysis to fill out the template's sections (Description, Root Cause Analysis, Affected Components & Code Locations, Proposed Solution) accurately.
  3. Write the issue body to a temporary scratch file in the workspace or the artifacts scratch directory.
  4. Create the issue using the GitHub CLI: `gh issue create --title "<Title>" --body-file "<PathToScratchFile>" --label "<Label>" --milestone "<Milestone>"`.
  5. Verify the created issue by running `gh issue view <id>`.
- **Releases & Tagging**: When tagging a new release, only create and push a single semantic version tag matching the repository's convention (e.g., `vX.Y.Z` where X.Y.Z matches the project version in `package.json`/`Cargo.toml`) to avoid triggering duplicate build workflows in GitHub Actions.

## Package Manager

- Use **bun** for all JavaScript/TypeScript package management in this project (not npm, yarn, or pnpm).
- Run scripts with `bun run <script>` and install packages with `bun add <package>`.
- Use **bunx** for running one-off CLI tools (not npx/node). Example: `bunx some-tool` instead of `npx some-tool`.

## Design Principles

- **State Preservation**: Luminous must always save and restore the state the user left/closed the application in. When reopened, the user should be returned exactly to where they were (e.g., same sidebar view/tab, same song selection, same player track/position/volume, same equalizer presets/enabled state).
- See [DESIGN.md](../DESIGN.md)
