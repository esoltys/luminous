<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListMusic, Heart, Clock, Calendar, Music, RefreshCw } from "lucide-svelte";
  import CardBadge from "./CardBadge.svelte";
  import type { PlaylistItem, Song } from "../types";
  import { songsToCoverStack } from "../utils/covers";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import CoverStack from "./CoverStack.svelte";

  interface Props {
    label: string;
    kind: "favourites" | "recently_added" | "genre" | "decade";
    genre?: string;
    decade?: string;
    /** For kind "genre" or "decade": the materialized playlist row backing it (refreshed at most every 24h). */
    playlistId?: number;
    /** For kind "genre" or "decade": when this playlist's songs were last (re)generated. */
    updated?: number;
    trackCount: number;
    autoPlay?: boolean;
    onClick: () => void;
    widthClass?: string;
  }

  import { playlistsStore } from "../stores/playlists.svelte";
  import { getPlaylistDisplayName } from "../utils/playlist";

  let { label, kind, genre, decade, playlistId, updated, trackCount, autoPlay = false, onClick, widthClass = "w-full" }: Props = $props();

  let displayLabel = $derived.by(() => {
    if ((kind === "genre" || kind === "decade") && playlistId !== undefined) {
      const pl = playlistsStore.playlists.find((p) => p.id === playlistId);
      if (pl) return getPlaylistDisplayName(pl);
    }
    return label;
  });

  let subtitleLabel = $derived.by(() => {
    if (kind === "decade" || decade) return i18n.t("playlists.decadeAutoPlaylist");
    if (kind === "genre" || genre) return i18n.t("playlists.genreAutoPlaylist");
    if (kind === "favourites") return i18n.t("playlists.favouritesAutoPlaylist");
    if (kind === "recently_added") return i18n.t("playlists.recentlyAddedAutoPlaylist");
    return i18n.t("playlists.genreAutoPlaylist");
  });

  let songs = $state<Song[]>([]);

  $effect(() => {
    const k = kind;
    const g = genre;
    const d = decade;
    const pid = playlistId;

    const request =
      (k === "genre" || k === "decade") && pid !== undefined
        ? invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: pid }).then((items) =>
            items.filter((item) => !!item.song).map((item) => item.song as Song)
          )
        : k === "favourites"
          ? invoke<Song[]>("get_favourite_songs")
          : k === "recently_added"
            ? invoke<Song[]>("get_recently_added_songs", { limit: 50 })
            : k === "decade"
              ? invoke<Song[]>("get_songs_by_decade", { decade: d ?? "", limit: 50 })
              : invoke<Song[]>("get_songs_by_genre", { genre: g ?? "", limit: 50 });

    request
      .then((res) => {
        if (kind === k && genre === g && decade === d && playlistId === pid) {
          songs = res;
        }
      })
      .catch((err) => {
        console.error("Failed to load auto-playlist songs for card:", err);
      });
  });

  // Favourites/Recently Added use a fixed icon cover instead of a CoverStack —
  // they're rebuilt from the whole library on every load, so a coverstack of
  // whichever songs happen to be in them right now reads as arbitrary rather
  // than representative (unlike a genre, decade, or user playlist).
  let topCovers = $derived(kind === "genre" || kind === "decade" ? songsToCoverStack(songs) : []);

  let badgeColorClass = $derived.by(() => {
    switch (kind) {
      case "decade": return "bg-[#2563EB] text-white";
      case "genre": return "bg-[#059669] text-white";
      case "favourites": return "bg-[#DB2777] text-white";
      case "recently_added": return "bg-[#CA8A04] text-white";
    }
  });

  let updatedLabel = $derived.by(() => {
    if ((kind !== "genre" && kind !== "decade") || updated === undefined) return null;
    return formatRelativeDate(updated);
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  class="{widthClass} bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col text-left hover:border-brand-accent/40 transition-all duration-200 cursor-pointer group"
>
  <div class="aspect-square w-full mb-3 bg-brand-main relative flex items-center justify-center">
    {#if (kind === "genre" || kind === "decade") && topCovers.length > 0}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br {kind === 'decade' ? 'from-[#2563EB]/25 to-[#38BDF8]/15 border-[#38BDF8]/30 shadow-[0_0_20px_2px_rgba(56,189,248,0.35)]' : 'from-[#059669]/25 to-[#34D399]/15 border-[#34D399]/30 shadow-[0_0_20px_2px_rgba(52,211,153,0.35)]'} flex items-center justify-center overflow-hidden border relative">
        <CoverStack covers={topCovers} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
      </div>
    {:else if kind === "favourites"}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#DB2777]/25 to-[#F43F5E]/15 flex items-center justify-center overflow-hidden border border-[#F43F5E]/30 shadow-[0_0_20px_2px_rgba(244,63,94,0.35)]">
        <Heart class="w-10 h-10 text-[#F43F5E] fill-current" />
      </div>
    {:else if kind === "recently_added"}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#CA8A04]/25 to-[#FACC15]/15 flex items-center justify-center overflow-hidden border border-[#FACC15]/30 shadow-[0_0_20px_2px_rgba(250,204,21,0.35)]">
        <Clock class="w-10 h-10 text-[#CA8A04]" />
      </div>
    {:else if kind === "decade"}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#2563EB]/25 to-[#38BDF8]/15 flex items-center justify-center overflow-hidden border border-[#38BDF8]/30 shadow-[0_0_20px_2px_rgba(56,189,248,0.35)]">
        <Calendar class="w-10 h-10 text-[#38BDF8]" />
      </div>
    {:else if kind === "genre"}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br from-[#059669]/25 to-[#34D399]/15 flex items-center justify-center overflow-hidden border border-[#34D399]/30 shadow-[0_0_20px_2px_rgba(52,211,153,0.35)]">
        <Music class="w-10 h-10 text-[#34D399]" />
      </div>
    {:else}
      <div class="w-full h-full bg-brand-main bg-gradient-to-br from-slate-700/40 to-slate-900/30 flex items-center justify-center overflow-hidden border border-slate-400/20 shadow-[0_0_20px_2px_rgba(100,116,139,0.25)]">
        <ListMusic class="w-10 h-10 text-slate-300" />
      </div>
    {/if}

    {#if autoPlay}
      <CardBadge icon={RefreshCw} label={i18n.t('playlists.autoPlayBadgeLabel')} title={i18n.t('playlists.autoPlayBadgeTooltip')} colorClass={badgeColorClass} spin />
    {/if}
  </div>

  <button
    onclick={(e) => { e.stopPropagation(); onClick(); }}
    class="font-semibold text-sm text-brand-text-primary hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate w-full cursor-pointer"
    title={displayLabel}
  >
    {displayLabel}
  </button>
  <div class="text-xs text-brand-text-secondary truncate w-full mt-0.5 font-medium">
    {subtitleLabel}
  </div>
  <div class="flex items-center justify-between mt-0.5 text-xs text-brand-text-secondary">
    <span class="truncate">{updatedLabel ?? ""}</span>
    <span class="shrink-0">{trackCount === 1 ? i18n.t('playlists.oneSong') : i18n.t("playlists.songsCount", { count: trackCount })}</span>
  </div>
</div>
