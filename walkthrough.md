# Walkthrough — Issue #129 Polish: Toggles, Album Cards, Top 5, Playlist Infobox

Branch: `claude/github-issue-129-comments-e3a2d4`
Issue: [#129: Polishing before 1.0 release](https://github.com/esoltys/luminous/issues/129)

This addresses the four newly-added comments on #129 (the ones without a 👍 — comments already
covered by a merged PR were left alone).

## 1. Auto/Custom playlist infobox now sits above the count+sort row

`PlaylistsCollectionView.svelte` rendered the "How do Auto Playlists work?" / "About this
playlist" info banner *below* the sticky "Showing N playlists / Sort" bar. Moved the banner
(for both the Auto and Custom sub-tabs) above that bar, matching the arrow in the issue
screenshot.

## 2. Album cards: year next to artist, genre where year used to be

`AlbumCard.svelte` showed `year` on the left of the 3rd line and a track-count category label
(`Single`/`EP`/`Album`/`N-Disc Set`) on the right. Per the request:
- Year moved up to sit flush-right on the artist-name line.
- The 3rd line now shows the album's `genre` flush-left; the category label stays flush-right.

## 3. Standardized toggle switches

Built a shared `Toggle.svelte` component (simple on/off pill switch, matching the reference
mockup) with an optional `showOnOffLabel` prop for compact rows. Converted:
- **Application & Updates** settings: the two native checkboxes (`update-check-toggle`,
  `update-auto-install-toggle`) in `FoldersView.svelte`.
- **Folders** settings: the two existing custom pill-switches (Real-time Watching, Rescan on
  Startup) now use the shared component instead of duplicated markup.
- **Equalizer**: Enable EQ, Loudness Normalization, Fade on Pause, Auto Crossfade, Suppress
  Same-Album Crossfade — 5 checkboxes converted to the compact (switch-only) variant, so this
  already-dense panel doesn't get any more crowded.
- **Organize Files** tool options: Replace Spaces, ASCII Only, Move Extra Files.
- **Auto-Refill** (auto-playlist detail view): redone from a single clickable bordered
  pill-button into a label + switch, matching the "on/off is the standard" mockup rather than
  the old embedded-toggle-inside-a-button style.
- **Smart Playlist Builder**: "Auto-Refill Batch Playback" toggle now uses the shared component
  too (was a bespoke `peer-checked` Tailwind toggle).

Left the column-visibility checkboxes in the library table's column-picker menu alone — that's a
multi-select context menu, not an on/off setting, so a checkbox is the right control there.

## 4. Top 5 (Home page): playlist and song rows

`HomeRowList.svelte` (the shared row component used for Most Played / Recently Added):
- **Song rows**: the trailing top-right slot now shows the literal word "Song" instead of
  track duration (mirrors how album rows already show a category label there).
- **Playlist rows**: top-right now shows "Playlist"; the subtitle line (bottom-left) now shows
  the playlist's category (`Genre` / `Decade` / `Smart` / `Custom`, mirroring
  `PlaylistCard.svelte`'s own category derivation) instead of the generic "Playlists" label;
  the bottom-right slot now shows the song count (previously the only thing shown, and it sat
  in the top-right slot).
- **Hover-to-play was actually broken for playlists**: multi-track playlists render their cover
  via `CoverStack`'s stacked-photos branch, where the front cover tile has an inline
  `z-index: 10`. The row's hover-play overlay button had no explicit z-index, so it painted
  *underneath* that cover tile and was invisible/unclickable. Added `z-20` to the overlay
  (matching the same pattern already used in `AlbumCard.svelte`).

## Verification

- `bun run check` — 0 errors, 0 warnings.
- `bun run vitest run` — 26 files, 260 tests passing (updated `AlbumCard.test.ts`,
  `Equalizer.test.ts`, and `HomeRowList.test.ts` for the markup/behavior changes above; added a
  genre assertion and a decade/genre category-label test).
- Not yet visually verified in a live dev server — please check with `bun run tauri dev`:
  - Settings → Application & Updates, Folders, Equalizer, Tools tabs — toggle styling.
  - An Auto Playlist's Auto-Refill control.
  - Playlists → Auto and Custom tabs — infobox position.
  - Album grid — year/genre placement.
  - Home page Top 5 — playlist row labels and hover-to-play on a playlist with multiple tracks.

## Next Steps

1. Review the diff / try it in the dev server.
2. Merge into `main`.
3. Comment on and close [#129](https://github.com/esoltys/luminous/issues/129) once merged
   (comment threads with 👍 were left untouched since they're already covered elsewhere).
