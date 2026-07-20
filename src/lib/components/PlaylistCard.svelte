<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ListMusic, Play } from "lucide-svelte";
  import type { Playlist, PlaylistItem } from "../types";
  import { getArtistGradient } from "../utils/artist";
  import { songsToCoverStack } from "../utils/covers";
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
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

  let updatedLabel = $derived(new Date(playlist.updated * 1000).toLocaleDateString());

  function handlePlayButtonClick(e: MouseEvent) {
    e.stopPropagation();
    const songIds = tracks.filter((t) => t.song && !t.song.unavailable).map((t) => t.song!.id);
    if (songIds.length > 0) {
      playerStore.playSongs(songIds, 0);
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
    {#if topAlbums.length > 0}
      <CoverStack covers={topAlbums} hoverEffect={true} sizeClass="w-36 h-36" />
    {:else}
      <div class="w-full h-full bg-gradient-to-br {getArtistGradient(playlist.name)} flex items-center justify-center overflow-hidden border border-brand-border/60">
        <ListMusic class="w-10 h-10 text-white/80" />
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
  <div class="flex items-center justify-between mt-0.5 text-[10px] text-brand-text-secondary/50">
    <span class="truncate">{i18n.t('playlists.updatedOn', { date: updatedLabel })}</span>
    <span class="shrink-0">{playlist.track_count === 1 ? i18n.t('playlists.oneSong') : i18n.t("playlists.songsCount", { count: playlist.track_count })}</span>
  </div>
</div>
