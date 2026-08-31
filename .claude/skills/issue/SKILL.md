---
name: issue
description: Look up a GitHub issue, confirm its milestone-derived base branch, and set up a dedicated worktree to start work on it
---

Start work on issue $ARGUMENTS.

1. **Fetch the issue**: `gh issue view $ARGUMENTS --json number,title,body,milestone,labels,url`.
   If it doesn't exist, stop and say so.
2. **Determine the base branch** per AGENTS.md's Branching Model: if the issue's milestone is
   "2.0", the base is `next`; otherwise it's `main`. If the issue has no milestone set, tell the
   user and ask them to confirm which base to use rather than guessing.
3. **Confirm before doing anything**: show the user the issue title, milestone, and the base
   branch you determined, and get an explicit go-ahead. Don't skip this even when the milestone
   makes the base obvious — this confirmation is the whole point of the skill.
4. **Set up the worktree**: `git fetch origin`, then create a new worktree under
   `.claude/worktrees/<short-issue-slug>` branched from `origin/<base>` (never reuse an existing
   worktree folder from a prior task — always create a fresh one). `cd` into it and run `pwd` to
   verify you're actually there before touching any files.
5. **Mark it in progress**: set the issue's Status to "In Progress" on the "Luminous Music Player"
   Project board (`gh project item-edit` — see docs/ISSUE_PRIORITY.md for the field/option IDs).
6. **Report back**: summarize the issue (what it's asking for, root cause if it's a bug, relevant
   files if you can tell from the description) and confirm you're ready to start implementation —
   then stop and wait for the user's direction on the actual implementation, since that's outside
   this skill's scope.
