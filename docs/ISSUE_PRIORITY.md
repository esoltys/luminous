# Issue Priority

Open issues in the "2.0" milestone have a priority (P1–P4) indicating when they should be worked
on after 1.0 ships, tracked exclusively as a field on the "Luminous Music Player" GitHub Project.
If you determine an issue's priority, set it via the Project board (or leave it for the user to
set) — do not create a `P1`–`P4` label as a substitute, even if no priority field is readily
settable through the `gh` CLI. Use this scheme when picking up 2.0 work or triaging a new issue
into it:

- **P1** — real (non-cosmetic) bugs, plus foundational/first-run work that other issues depend on.
- **P2** — features touched often, expected for parity with comparable desktop music players, or
  that encourage exploring the library (cover art, artist bios and connections, browsing by
  genre) rather than just playing tracks.
- **P3** — lower-frequency or power-user features, and cosmetic/low-severity bugs.
- **P4** — speculative, large-scope, or low-demand work; revisit after core 2.0 features ship.

Don't default new issues to P2/P3 — assign a priority using the same criteria above.
