<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListMusic, Play, Calendar, Music, Radio, Layers, Sparkles } from "lucide-svelte";
  import type { Playlist, PlaylistItem } from "../types";
  import { getArtistGradient } from "../utils/artist";
  import { songsToCoverStack } from "../utils/covers";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { formatRelativeDate } from "../utils/date";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import CoverStack from "./CoverStack.svelte";

  let { playlist, onClick, widthClass = "w-full" }: { playlist: Playlist; onClick: () => void; widthClass?: string } = $props();

  let tracks = $state<PlaylistItem[]>([]);

  $effect(() => {
    const id = playlist.id;
    invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: id })
      .then((res) => {
        if (playlist.id === id) {
          tracks = res;
        }
      })
      .catch((err) => {
        console.error("Failed to load playlist tracks for card:", err);
      });
  });

  let topAlbums = $derived(songsToCoverStack(tracks.filter((t) => !!t.song).map((t) => t.song!)));

  // System genre auto-playlists store a bare genre name (no ':') and never
  // reach this component (they render via AutoPlaylistCard instead).
  let autoKind = $derived<"genre" | "decade" | "smart" | null>(
    !playlist.dynamic_enabled
      ? null
      : playlist.dynamic_spec?.startsWith("decade:")
      ? "decade"
      : isSmartPlaylistSpec(playlist.dynamic_spec)
      ? "smart"
      : "genre"
  );

  let subtitleLabel = $derived.by(() => {
    if (!playlist.dynamic_enabled) return null;
    if (autoKind === "decade") return i18n.t("playlists.decadeAutoPlaylist");
    if (autoKind === "genre") return i18n.t("playlists.genreAutoPlaylist");
    return "Smart Rule Playlist";
  });

  let isQueue = $derived(!playlist.dynamic_enabled && playlist.name.toLowerCase() === "queue");
  let isActive = $derived(playlistsStore.effectivePinnedPlaylistId === playlist.id);

  let updatedLabel = $derived(formatRelativeDate(playlist.updated));

  function handlePlayButtonClick(e: MouseEvent) {
    e.stopPropagation();
    const songIds = tracks.filter((t) => t.song && !t.song.unavailable).map((t) => t.song!.id);
    if (songIds.length > 0) {
      playerStore.playSongs(songIds, 0, playlist.id);
    }
    onClick();
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={onClick}
  class="{widthClass} bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col text-left hover:border-brand-accent/40 transition-all duration-200 cursor-pointer group relative"
>
  <div class="aspect-square w-full mb-3 bg-brand-main relative flex items-center justify-center rounded-lg overflow-hidden">
    {#if isQueue}
      <div class="w-full h-full bg-gradient-to-br from-indigo-600 via-purple-600 to-pink-600 flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg">
        <Layers class="w-10 h-10 text-white/90" />
      </div>
    {:else if topAlbums.length > 0 && autoKind}
      <div class="w-full h-full bg-gradient-to-br {autoKind === 'decade' ? 'from-cyan-600 to-blue-600' : autoKind === 'genre' ? 'from-emerald-600 to-teal-600' : 'from-amber-500 via-orange-500 to-orange-600'} flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg">
        <CoverStack covers={topAlbums} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
      </div>
    {:else if topAlbums.length > 0}
      <CoverStack covers={topAlbums} hoverEffect={true} sizeClass="w-[82%] h-[82%]" />
    {:else if autoKind}
      <div class="w-full h-full bg-gradient-to-br {autoKind === 'decade' ? 'from-cyan-600 to-blue-600' : autoKind === 'genre' ? 'from-emerald-600 to-teal-600' : 'from-amber-500 via-orange-500 to-orange-600'} flex items-center justify-center overflow-hidden border border-brand-border/60 rounded-lg">
        {#if autoKind === "decade"}
          <Calendar class="w-10 h-10 text-white/90" />
        {:else if autoKind === "genre"}
          <Music class="w-10 h-10 text-white/90" />
        {:else}
          <Sparkles class="w-10 h-10 text-white/90" />
        {/if}
      </div>
    {:else}
      <div class="w-full h-full bg-gradient-to-br {getArtistGradient(playlist.name)} flex items-center justify-center overflow-hidden border border-brand-border/60">
        <ListMusic class="w-10 h-10 text-white/80" />
      </div>
    {/if}

    {#if (autoKind === "decade" || autoKind === "genre") && playlist.auto_play}
      <div
        class="absolute top-2 right-2 z-30 flex items-center gap-1 px-2 py-0.5 rounded-full bg-brand-accent text-brand-accent-contrast text-[9px] font-bold tracking-wide shadow-lg select-none"
        title={i18n.t('playlists.autoPlayBadgeTooltip')}
      >
        <Radio class="w-2.5 h-2.5" />
        {i18n.t('playlists.autoPlayBadgeLabel')}
      </div>
    {:else if autoKind === "smart"}
      <div
        class="absolute top-2 right-2 z-30 flex items-center gap-1 px-2 py-0.5 rounded-full bg-brand-accent text-brand-accent-contrast text-[9px] font-bold tracking-wide shadow-lg select-none"
        title="Smart Rule-Based Playlist"
      >
        <Sparkles class="w-2.5 h-2.5" />
        Smart
      </div>
    {/if}

    {#if isActive}
      <div
        class="absolute top-2 left-2 z-30 flex items-center gap-1 px-2 py-0.5 rounded-full bg-brand-accent text-brand-accent-contrast text-[9px] font-bold tracking-wide shadow-lg select-none"
        title={i18n.t('playlists.activeBadgeTooltip')}
      >
        <Radio class="w-2.5 h-2.5 animate-pulse" />
        {i18n.t('playlists.activeBadgeLabel')}
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
  </div>

  <button
    onclick={(e) => { e.stopPropagation(); onClick(); }}
    class="font-semibold text-sm text-brand-text-primary hover:text-brand-accent-text hover:underline transition-all duration-150 text-left truncate w-full cursor-pointer"
    title={playlist.name}
  >
    {playlist.name}
  </button>
  {#if subtitleLabel}
    <div class="text-xs text-brand-text-secondary truncate w-full mt-0.5 font-medium">
      {subtitleLabel}
    </div>
  {/if}
  <div class="flex items-center justify-between mt-2 text-[10px] text-brand-text-secondary/50">
    <span class="truncate">{updatedLabel}</span>
    <span class="shrink-0">{playlist.track_count === 1 ? i18n.t('playlists.oneSong') : i18n.t("playlists.songsCount", { count: playlist.track_count })}</span>
  </div>

  {#if !playlist.dynamic_enabled}
    {#if !isActive}
      <button
        onclick={(e) => { e.stopPropagation(); playlistsStore.pinPlaylist(playlist.id); }}
        class="mt-2.5 w-full py-1 px-2.5 text-xs font-semibold rounded-lg bg-brand-main/80 hover:bg-brand-accent hover:text-brand-accent-contrast border border-brand-border/60 text-brand-text-secondary hover:border-transparent transition-all duration-150 flex items-center justify-center gap-1.5 cursor-pointer shadow-xs"
        title={i18n.t('playlists.makeActiveBtn')}
      >
        <Radio class="w-3.5 h-3.5 text-brand-accent-text group-hover:text-current" />
        <span>{i18n.t('playlists.makeActiveBtn')}</span>
      </button>
    {:else}
      <div class="mt-2.5 w-full py-1 px-2.5 text-xs font-semibold rounded-lg bg-brand-accent/15 text-brand-accent-text border border-brand-accent/30 flex items-center justify-center gap-1.5 select-none">
        <Radio class="w-3.5 h-3.5 text-brand-accent-text animate-pulse" />
        <span>{i18n.t('playlists.activeBadgeLabel')}</span>
      </div>
    {/if}
  {/if}
</div>
