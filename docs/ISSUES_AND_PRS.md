# Creating Issues & Pull Requests

Moved out of `AGENTS.md`; follow this when creating an issue, epic or pull request.

1. **Inspect templates**: Inspect the relevant templates under `.github/ISSUE_TEMPLATE/` (`bug_report.md`, `feature_request.md`, `epic.md`) and `.github/PULL_REQUEST_TEMPLATE.md`.
   - **MANDATORY**: Always use the Markdown internal/dev templates (`.github/ISSUE_TEMPLATE/feature_request.md`, `.github/ISSUE_TEMPLATE/bug_report.md`, `.github/ISSUE_TEMPLATE/epic.md`), **NEVER** the YAML forms (`feature_request.yml` / `bug_report.yml`).
   - The `.md` templates are internal developer templates tailored for engineering architecture, root cause analysis, and affected component tracking.
   - The `.yml` files are public GitHub Issue Forms intended exclusively for external user submissions. Agents must NEVER use `.yml` templates or assign the `user request` label.
2. **Analysis**: Perform codebase search or analysis to fill out the template's sections accurately:
   - For features (`feature_request.md`): `## 💡 Description`, `## 🔍 Implementation Design / Strategy`, `## 🛠️ Affected Components & Code Locations`.
   - For bugs (`bug_report.md`): `## 🔴 Description`, `## 🔍 Root Cause Analysis`, `## 🛠️ Affected Components & Code Locations`, `## 💡 Proposed Solution`.
3. **Format bodies**:
   - Write paragraphs and bullet points as single unwrapped lines. Never hand-wrap lines at ~80–100 columns — GitHub renders single newlines as hard breaks.
   - Write the issue or PR body to a temporary scratch file in the workspace or the artifacts scratch directory.
4. **Milestone selection**:
   - Never file a new issue against a Closed milestone.
   - Unless the user specifies one, default to the lowest open milestone that hasn't been released (no `vX.Y.0` in `gh release list`; open milestones via `gh api repos/esoltys/luminous/milestones -q '.[].title'`), rather than asking which milestone to use each time.
   - **Never attach a milestone to a Pull Request** — milestones track issues only.
5. **Create the issue**:
   - Bugs: `gh issue create --title "<Title>" --body-file "<PathToScratchFile>" --label "bug" --milestone "<Milestone>"`
   - Features: `gh issue create --title "<Title>" --body-file "<PathToScratchFile>" --milestone "<Milestone>"` (no label needed; never add "user request")
   - Epics: `gh issue create --title "Epic: <Title>" --body-file "<PathToScratchFile>" --milestone "<Milestone>"` (no label needed). After creating the epic, link its sub-issues with `gh issue edit <epic-id> --add-sub-issue <id1>,<id2>...`. Never list sub-issues in the description body — GitHub renders them below automatically. If there are no related issues, leave the `## Related` section out entirely. Epic sub-issue order is manual: order them with the GraphQL `reprioritizeSubIssue` mutation (`issueId`, `subIssueId`, and `afterId`/`beforeId`), then re-read `subIssues` to confirm.
6. **Project Priority & Status**:
   - **NEVER apply a `P1`/`P2`/`P3`/`P4` label to an issue, and never pass `--label P1` (or P2–P4) to `gh issue create` or `gh issue edit`.** Priority is not a label; it is tracked exclusively on the "Luminous Music Player" GitHub Project.
   - Add the issue to the "Luminous Music Player" Project board, set Status to "Todo", and assign a Priority using the scheme in [docs/ISSUE_PRIORITY.md](ISSUE_PRIORITY.md) (which details `gh project item-add` / `item-edit` commands and field/option IDs).
