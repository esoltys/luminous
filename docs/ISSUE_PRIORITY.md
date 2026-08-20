# Issue Priority & Status

Priority (P1–P4) and Status (Todo/In Progress/Done/Parked) are tracked exclusively as fields on
the "Luminous Music Player" GitHub Project (`gh project` number `3`, owner `esoltys`) — never as
labels. Both are readable and settable directly through the `gh project` CLI; there's no need to
ask the user to update them by hand or fall back to a label as a substitute.

## Adding a new issue to the board

A freshly created issue isn't a Project item yet, so it has no Priority/Status to read or set
until it's added:

```bash
gh project item-add 3 --owner esoltys --url <issue-url>
```

## Reading current values

```bash
gh project item-list 3 --owner esoltys --format json
```

Each item in the result includes `priority` and `status` directly, plus `content.number` so you
can match it to a specific issue.

## Setting a value

Find the item's `id` from the `item-list` output above (matching on `content.number`), then:

```bash
gh project item-edit --project-id PVT_kwHOAAE3ZM4BgXrH --id <item-id> \
  --field-id <field-id> --single-select-option-id <option-id>
```

**Priority** — field id `PVTSSF_lAHOAAE3ZM4BgXrHzhakJHw`:

| Option | id |
| --- | --- |
| P1 | `89eea1e7` |
| P2 | `145b89a5` |
| P3 | `3c790d36` |
| P4 | `1db7b067` |

**Status** — field id `PVTSSF_lAHOAAE3ZM4BgXrHzhakFIo`:

| Option | id |
| --- | --- |
| Todo | `f75ad846` |
| In Progress | `47fc9ee4` |
| Done | `98236657` |
| Parked | `02ee74f6` |

If the Project's fields are ever recreated, these IDs will change — re-run `gh project field-list
3 --owner esoltys --format json` and update this table.

## Priority scheme

Every bug/feature issue gets a Priority when it's created, regardless of milestone — set Status
to "Todo" and assign a Priority using this scheme (2.0-milestone issues additionally use it to
indicate when they should be worked on after 1.0 ships):

- **P1** — real (non-cosmetic) bugs, plus foundational/first-run work that other issues depend on.
- **P2** — features touched often, expected for parity with comparable desktop music players, or
  that encourage exploring the library (cover art, artist bios and connections, browsing by
  genre) rather than just playing tracks.
- **P3** — lower-frequency or power-user features, and cosmetic/low-severity bugs.
- **P4** — speculative, large-scope, or low-demand work; revisit after core 2.0 features ship.

Don't default new issues to P2/P3 — assign a priority using the same criteria above.
