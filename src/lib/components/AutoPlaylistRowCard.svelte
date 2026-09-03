<script lang="ts">
  import { Heart, Clock, Hourglass, Calendar, Music, Gauge, Tag, TrendingUp, AlertTriangle } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { getPlaylistDisplayName } from "../utils/playlist";
  import { toTitleCase } from "../utils/formatters";

  interface Props {
    label: string;
    kind: "favourites" | "recently_added" | "most_played" | "history" | "genre" | "decade" | "bpm" | "artist_tag" | "missing_metadata";
    genre?: string;
    artistTag?: string;
    decade?: string;
    bpm?: string;
    playlistId?: number;
    updated?: number;
    trackCount: number;
    onClick: () => void;
  }

  let { label, kind, genre, artistTag, decade, bpm, playlistId, updated, trackCount, onClick }: Props = $props();

  let displayLabel = $derived.by(() => {
    if ((kind === "genre" || kind === "decade" || kind === "bpm" || kind === "artist_tag" || kind === "missing_metadata") && playlistId !== undefined) {
      const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
      if (pl) return getPlaylistDisplayName(pl);
    }
    return kind === "artist_tag" ? toTitleCase(label) : label;
  });

  let subtitleLabel = $derived.by(() => {
    if (kind === "missing_metadata") return i18n.t("playlists.missingMetadataAutoPlaylist");
    if (kind === "decade" || decade) return i18n.t("playlists.decadeAutoPlaylist");
    if (kind === "bpm") return i18n.t("playlists.bpmAutoPlaylist");
    if (kind === "artist_tag" || artistTag) return i18n.t("playlists.artistTagAutoPlaylist");
    if (kind === "genre" || genre) return i18n.t("playlists.genreAutoPlaylist");
    if (kind === "favourites") return i18n.t("playlists.favouritesAutoPlaylist");
    if (kind === "recently_added") return i18n.t("playlists.recentlyAddedAutoPlaylist");
    if (kind === "most_played") return i18n.t("playlists.mostPlayedAutoPlaylist");
    if (kind === "history") return i18n.t("playlists.historyAutoPlaylist");
    return i18n.t("playlists.genreAutoPlaylist");
  });

  let updatedLabel = $derived.by(() => {
    if ((kind !== "genre" && kind !== "decade" && kind !== "bpm" && kind !== "artist_tag" && kind !== "missing_metadata") || updated === undefined) return null;
    return formatRelativeDate(updated);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
>
  <div
    class="relative shrink-0 w-11 h-11 flex items-center justify-center overflow-hidden border {kind === 'decade'
      ? 'bg-[#38BDF8]/15 border-[#38BDF8]/30'
      : kind === 'genre'
        ? 'bg-[#34D399]/15 border-[#34D399]/30'
        : kind === 'artist_tag'
          ? 'bg-[#FB923C]/15 border-[#FB923C]/30'
          : kind === 'bpm'
            ? 'bg-[#E879F9]/15 border-[#E879F9]/30'
            : kind === 'favourites'
              ? 'bg-[#F43F5E]/15 border-[#F43F5E]/30'
              : kind === 'most_played'
                ? 'bg-[#DC2626]/15 border-[#DC2626]/30'
                : kind === 'history'
                  ? 'bg-[#8B5CF6]/15 border-[#8B5CF6]/30'
                  : kind === 'missing_metadata'
                    ? 'bg-amber-500/15 border-amber-500/30'
                    : 'bg-[#FACC15]/15 border-[#FACC15]/30'}"
  >
    {#if kind === "favourites"}
      <Heart class="w-5 h-5 text-[#F43F5E] fill-current" />
    {:else if kind === "recently_added"}
      <Clock class="w-5 h-5 text-[#CA8A04]" />
    {:else if kind === "most_played"}
      <TrendingUp class="w-5 h-5 text-[#DC2626]" />
    {:else if kind === "history"}
      <Hourglass class="w-5 h-5 text-[#8B5CF6]" />
    {:else if kind === "decade"}
      <Calendar class="w-5 h-5 text-[#38BDF8]" />
    {:else if kind === "artist_tag"}
      <Tag class="w-5 h-5 text-[#FB923C]" />
    {:else if kind === "bpm"}
      <Gauge class="w-5 h-5 text-[#E879F9]" />
    {:else if kind === "missing_metadata"}
      <AlertTriangle class="w-5 h-5 text-amber-500" />
    {:else}
      <Music class="w-5 h-5 text-[#34D399]" />
    {/if}
  </div>

  <div class="min-w-0 flex-1">
    <p class="truncate text-sm font-semibold text-brand-text-primary" title={displayLabel}>{displayLabel}</p>
    <p class="truncate text-xs text-brand-text-secondary font-medium">{subtitleLabel}</p>
  </div>

  <div class="shrink-0 max-w-40 text-right">
    <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate">
      {trackCount === 1 ? i18n.t("playlists.oneSong") : i18n.t("playlists.songsCount", { count: trackCount })}
    </p>
    {#if updatedLabel}
      <p class="text-xs text-brand-text-secondary truncate">{updatedLabel}</p>
    {/if}
  </div>
</div>
