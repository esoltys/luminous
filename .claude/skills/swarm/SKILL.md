---
name: swarm
description: Parallel-swarm open Todo issues for a milestone into worktrees, implementations, and PRs — one subagent per independent issue. Use when the user says "swarm", "swarm N Claudes on milestone X", "swarm the bugs for 1.5", or otherwise asks to fan out a batch of issues into parallel fixes/PRs instead of working them one at a time.
---

Fan out open, Project-status-"Todo" issues from a milestone into parallel subagents, each of
which independently implements a fix and opens a PR. Arguments: $ARGUMENTS

## 1. Parse arguments

Accept both `key:value` form (`milestone:1.5 base:main type:bugs limit:5`) and natural language
(`"swarm 5 Claudes on milestone 2.0 issues based on priority"`). Extract:

- **milestone** (required) — if not present anywhere in $ARGUMENTS, ask the user with
  `AskUserQuestion` before doing anything else. Don't guess.
- **base branch** (optional) — if not given explicitly, infer it from AGENTS.md's Branching
  Model: milestone "2.0" → `next`, everything else → `main`. Say out loud which base you're
  using and why, since getting this wrong ships unfinished work early or wastes a `next` merge.
- **type** — `bugs` (issues labeled `bug`), `features` (open issues without that label), or
  `both`. This materially changes what gets worked on, so if it's not stated, ask via
  `AskUserQuestion` rather than defaulting silently.
- **limit** (optional) — max number of subagents/issues to run this round. If omitted, swarm
  every eligible independent issue found in step 3.

## 2. Load conventions once

Read AGENTS.md and CLAUDE.md in full before doing anything else — they're the canonical source
for worktree layout, branch/commit conventions, PR template usage, and the Definition of Done.
Don't restate their contents in your own reasoning or in the subagent prompts you write; point
each subagent at the files instead, so nothing here drifts out of sync with them as those docs
change. Also skim `docs/ISSUE_PRIORITY.md` — it has the `gh project` commands and field/option IDs
you need in the next step.

## 3. Find eligible issues

Using `gh issue list --milestone <milestone> --state open` and `gh project item-list 3 --owner
esoltys --format json` (see `docs/ISSUE_PRIORITY.md`), build the set of open issues in this
milestone whose Project Status is "Todo". Filter by `type` from step 1 (bug label present /
absent / either). Pull each candidate's Priority (P1–P4) from the same `item-list` output — you
need it for the limit step below, and Priority is a Project field here, never a label.

## 4. Check independence and group

For each candidate issue, skim its title/body and do a quick `Grep`/`Glob` pass to guess which
files it's likely to touch (component names, module names, file paths mentioned). Two issues that
look like they'll touch the same files or the same narrow area of the codebase are not
independent — group them together rather than treating every issue as parallel-safe by default.
Within a group, only the first issue runs standalone; the rest of that group serialize behind it
(each waits for the previous one's worktree/PR to land before starting, still each an independent
task in the tracker).

If a `limit` was given and the number of eligible *groups* exceeds it, keep the highest-priority
groups (P1 first) and drop the rest — tell the user which issues were left out and why (limit, not
ineligibility) so they can raise the limit or run a follow-up swarm.

## 5. Track and launch

Create one task item per issue (via your task-tracking tool) before launching anything, so
progress is visible as agents complete. For each independent issue (or the head of a serialized
group), launch an `Agent` — groups run their members sequentially, everything else runs in
parallel in the same turn. Give each subagent a self-contained prompt covering:

- Read AGENTS.md and CLAUDE.md yourself first — don't assume this summary is complete.
- Set the issue's Project Status to "In Progress" now (`docs/ISSUE_PRIORITY.md` has the
  `gh project item-edit` command), before starting work.
- Create a fresh worktree at `.claude/worktrees/issue-<N>-<short-slug>` off `<base>` (confirmed
  above), on a branch named `<fix|feat>/<N>-<short-slug>` matching the issue's type. Never edit
  outside that worktree.
- Implement a fix that resolves the issue as scoped — don't expand into adjacent cleanup.
- Run the full verification a contributor would run before pushing: `bun run check`, `bun run
  test:run`, `cargo test` (from `src-tauri/`), `cargo clippy --all-targets` (from `src-tauri/`,
  zero warnings). Fix failures before proceeding; report honestly if something can't be made to
  pass and why.
- Commit with a Conventional Commits message (`feat:`/`fix:`/etc).
- Open a PR targeting `<base>`, using `.github/PULL_REQUEST_TEMPLATE.md`'s structure, with
  "Closes #N" in the summary so merging auto-closes the issue.
- Leave the worktree in place (don't delete it or merge locally) — the PR is the review surface
  for this issue, same as any other PR in this repo.
- Report back: PR URL, which verification commands passed/failed, and any risk or judgment call
  worth flagging to a human reviewer (scope cuts, untested edge cases, anything the issue was
  ambiguous about).

## 6. Summarize

Once every agent (and every serialized group) has finished, produce one table:

| Issue | PR | Tests Passing | Unresolved Risk |
|---|---|---|---|

Pull each row from that subagent's own report — don't re-verify their work yourself unless
something in their report looks wrong. Call out any issues dropped in step 4 (overlap
serialization delays, or limit cutoffs) separately below the table so nothing silently vanishes.
