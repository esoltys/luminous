<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListMusic, Play, Heart, Clock, Calendar, Music, RefreshCw, Radio } from "lucide-svelte";
  import type { PlaylistItem, Song } from "../types";
  import { songsToCoverStack } from "../utils/covers";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
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

  let { label, kind, genre, decade, playlistId, updated, trackCount, autoPlay = false, onClick, widthClass = "w-full" }: Props = $props();

  let isActive = $derived(playlistId !== undefined && playlistsStore.pinnedPlaylistId === playlistId);

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

  let updatedLabel = $derived.by(() => {
    if ((kind !== "genre" && kind !== "decade") || updated === undefined) return null;
    return formatRelativeDate(updated);
  });

  function handlePlayButtonClick(e: MouseEvent) {
    e.stopPropagation();
    if (songs.length > 0) {
      playerStore.playSongs(songs.map((s) => s.id), 0, playlistId);
    }
    onClick();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  class="{widthClass} bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col text-left hover:border-brand-accent/40 transition-all duration-200 cursor-pointer group"
>
  <div class="aspect-square w-full mb-3 bg-brand-main relative flex items-center justify-center">
    {#if (kind === "genre" || kind === "decade") && topCovers.length > 0}
      <div class="w-full h-full bg-gradient-to-br {kind === 'decade' ? 'from-cyan-600 to-blue-600' : 'from-emerald-600 to-teal-600'} flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg relative">
        <CoverStack covers={topCovers} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
      </div>
    {:else if kind === "favourites"}
      <div class="w-full h-full bg-gradient-to-br from-purple-600 to-indigo-600 flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg">
        <Heart class="w-10 h-10 text-white/90 fill-current" />
      </div>
    {:else if kind === "recently_added"}
      <div class="w-full h-full bg-gradient-to-br from-amber-500 to-orange-600 flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg">
        <Clock class="w-10 h-10 text-white/90" />
      </div>
    {:else if kind === "decade"}
      <div class="w-full h-full bg-gradient-to-br from-cyan-600 to-blue-600 flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg">
        <Calendar class="w-10 h-10 text-white/90" />
      </div>
    {:else if kind === "genre"}
      <div class="w-full h-full bg-gradient-to-br from-emerald-600 to-teal-600 flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg">
        <Music class="w-10 h-10 text-white/90" />
      </div>
    {:else}
      <div class="w-full h-full bg-gradient-to-br from-slate-700 to-slate-900 flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg">
        <ListMusic class="w-10 h-10 text-white/90" />
      </div>
    {/if}
    <div class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity z-20">
      <button
        onclick={handlePlayButtonClick}
        class="w-12 h-12 rounded-full bg-brand-accent text-brand-accent-contrast flex items-center justify-center scale-75 group-hover:scale-100 transition-transform cursor-pointer"
        title={i18n.t('playerBar.play')}
      >
        <Play class="w-5 h-5 fill-current ml-0.5" />
      </button>
    </div>

    {#if isActive}
      <div
        class="absolute top-2 left-2 z-30 flex items-center gap-1 px-2 py-0.5 rounded-full bg-brand-accent text-brand-accent-contrast text-[9px] font-bold tracking-wide shadow-lg select-none"
        title={i18n.t('playlists.activeBadgeTooltip')}
      >
        <Radio class="w-2.5 h-2.5 animate-pulse" />
        {i18n.t('playlists.activeBadgeLabel')}
      </div>
    {/if}
    {#if autoPlay}
      <!-- Auto-Play badge (#26) -->
      <div
        class="absolute top-2 right-2 z-30 flex items-center gap-1 px-1.5 py-0.5 rounded-full bg-brand-accent/90 text-brand-accent-contrast text-[9px] font-bold tracking-wide shadow-lg"
        title={i18n.t('playlists.autoPlayBadgeTooltip')}
      >
        <RefreshCw class="w-2.5 h-2.5 animate-spin [animation-duration:3s]" />
        {i18n.t('playlists.autoPlayBadgeLabel')}
      </div>
    {/if}
  </div>

  <button
    onclick={(e) => { e.stopPropagation(); onClick(); }}
    class="font-semibold text-sm text-brand-text-primary hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate w-full cursor-pointer"
    title={label}
  >
    {label}
  </button>
  <div class="text-xs text-brand-text-secondary truncate w-full mt-0.5 font-medium">
    {subtitleLabel}
  </div>
  <div class="flex items-center justify-between mt-2 text-[10px] text-brand-text-secondary/50">
    <span class="truncate">{updatedLabel ?? ""}</span>
    <span class="shrink-0">{trackCount === 1 ? i18n.t('playlists.oneSong') : i18n.t("playlists.songsCount", { count: trackCount })}</span>
  </div>
</div>
