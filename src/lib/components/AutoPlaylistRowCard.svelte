<script lang="ts">
  import PlaylistKindIcon from "./PlaylistKindIcon.svelte";
  import { getPlaylistCardTheme } from "../utils/playlistCardTheme";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import { playlistsStore } from "../stores/playlists.svelte";
  import {
    getAutoPlaylistSubtitle,
    getPlaylistDisplayName,
    hasBackingRow,
  } from "../utils/playlist";
  import { countryDisplayName } from "../utils/country";
  import { getLanguageDisplayName } from "../utils/language";
  import { toTitleCase } from "../utils/formatters";
  import { rowClasses, useCardAppearance } from "../utils/cardAppearance";

  const appearance = useCardAppearance();

  interface Props {
    label: string;
    kind:
      | "favourites"
      | "recently_added"
      | "most_played"
      | "history"
      | "genre"
      | "decade"
      | "bpm"
      | "artist_tag"
      | "missing_metadata"
      | "missing_musicbrainz"
      | "daypart"
      | "language"
      | "country";
    selector?: string;
    playlistId?: number;
    updated?: number;
    trackCount: number;
    onClick: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
  }

  let { label, kind, selector, playlistId, updated, trackCount, onClick, oncontextmenu }: Props =
    $props();

  let displayLabel = $derived.by(() => {
    if (hasBackingRow(kind) && playlistId !== undefined) {
      const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
      if (pl) return getPlaylistDisplayName(pl);
    }
    if (kind === "language") return getLanguageDisplayName(selector || label);
    if (kind === "country") return countryDisplayName(selector, i18n.currentLocale) || label;
    return kind === "artist_tag" ? toTitleCase(label) : label;
  });

  let subtitleLabel = $derived(getAutoPlaylistSubtitle({ kind, selector }));

  let updatedLabel = $derived.by(() => {
    if (!hasBackingRow(kind) || updated === undefined) return null;
    return formatRelativeDate(updated);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div onclick={onClick} {oncontextmenu} class={rowClasses(appearance)}>
  <div
    class="relative shrink-0 w-11 h-11 flex items-center justify-center overflow-hidden border {getPlaylistCardTheme(
      kind,
    ).tintClass}"
  >
    <PlaylistKindIcon {kind} sizeClass="w-5 h-5" />
  </div>

  <div class="min-w-0 flex-1">
    <p class="truncate text-sm font-semibold text-brand-text-primary" title={displayLabel}>
      {displayLabel}
    </p>
    <p class="truncate text-xs text-brand-text-secondary font-medium">{subtitleLabel}</p>
  </div>

  <div class="shrink-0 max-w-40 text-right">
    <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate">
      {i18n.plural("playlists.songsCount", trackCount)}
    </p>
    {#if updatedLabel}
      <p class="text-xs text-brand-text-secondary truncate">{updatedLabel}</p>
    {/if}
  </div>
</div>
