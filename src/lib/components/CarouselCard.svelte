<script lang="ts">
  import type { HomeItem } from "../types";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import CoverArt from "./CoverArt.svelte";
  import { Play } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { Song } from "../types";
  import { i18n } from "../stores/i18n.svelte";

  let { item }: { item: HomeItem } = $props();

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const seconds = Math.floor(ns / 1_000_000_000);
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}:${secs.toString().padStart(2, "0")}`;
  }

  async function handlePlay(e: MouseEvent) {
    e.stopPropagation();
    if (item.type === "song") {
      await playerStore.playSong(item.song.id);
    } else {
      let songs = await invoke<Song[]>("get_songs_by_album", {
        album: item.album.album || "",
      });
      // Filter out excluded formats
      songs = songs.filter(song => !collectionStore.isFormatExcluded(song.filetype));
      if (songs.length > 0) {
        const songIds = songs.map((s) => s.id);
        playerStore.playSongs(songIds, 0);
      }
    }
  }

  function handleCardClick() {
    if (item.type === "album") {
      collectionStore.navigateTo("collection", "songs", item.album.album || "");
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  onclick={handleCardClick}
  class="flex-shrink-0 w-48 group relative {item.type === 'album' ? 'cursor-pointer' : ''}"
>
  <!-- Card Container -->
  <div class="relative rounded-lg overflow-hidden bg-brand-sidebar border border-brand-border/50 transition-all duration-200 hover:border-brand-accent hover:shadow-lg hover:shadow-brand-accent/10">
    <!-- Cover Art -->
    <div class="relative aspect-square overflow-hidden bg-brand-sidebar">
      {#if item.type === "song"}
        <CoverArt
          songId={item.song.id}
          artEmbedded={item.song.art_embedded}
          artAutomatic={item.song.art_automatic}
          artManual={item.song.art_manual}
          sizeClass="w-full h-full"
        />
      {:else}
        <CoverArt
          songId={undefined}
          artEmbedded={item.album.art_embedded}
          artAutomatic={item.album.art_automatic}
          artManual={item.album.art_manual}
          sizeClass="w-full h-full"
        />
      {/if}

      <!-- Play Button Overlay -->
      <div class="absolute inset-0 bg-black/0 group-hover:bg-black/40 transition-colors duration-200 flex items-center justify-center">
        <button
          onclick={handlePlay}
          class="opacity-0 group-hover:opacity-100 transition-opacity duration-200 bg-brand-accent hover:bg-brand-accent-hover rounded-full p-3 text-brand-accent-contrast shadow-lg cursor-pointer"
          title={i18n.t('playerBar.play')}
        >
          <Play class="w-6 h-6 fill-current" />
        </button>
      </div>
    </div>

    <!-- Metadata -->
    <div class="p-3 space-y-2">
      {#if item.type === "song"}
        <!-- Song Title -->
        <h3 class="text-sm font-semibold text-brand-text-primary truncate" title={item.song.title}>
          {item.song.title || i18n.t('collection.unknownSong')}
        </h3>

        <!-- Song Artist -->
        <p class="text-xs text-brand-text-secondary truncate" title={item.song.artist}>
          {item.song.artist || i18n.t('collection.unknownArtist')}
        </p>

        <!-- Song Duration -->
        <p class="text-xs text-brand-text-secondary/60">
          {formatDuration(item.song.length_nanosec)}
        </p>
      {:else}
        <!-- Album Title -->
        <h3 class="text-sm font-semibold text-brand-text-primary truncate hover:text-brand-accent-text hover:underline transition-all" title={item.album.album || i18n.t('collection.unknownAlbum')}>
          {item.album.album || i18n.t('collection.unknownAlbum')}
        </h3>

        <!-- Album Artist -->
        {#if item.album.artist}
          <button
            onclick={(e) => {
              e.stopPropagation();
              collectionStore.viewArtist(item.album.artist || "");
            }}
            class="text-xs text-brand-text-secondary hover:text-brand-accent-text hover:underline transition-all truncate cursor-pointer text-left w-full"
            title={i18n.t('collection.filterByArtist', { artist: item.album.artist })}
          >
            {item.album.artist}
          </button>
        {:else}
          <p class="text-xs text-brand-text-secondary truncate">{i18n.t('collection.unknownArtist')}</p>
        {/if}

        <!-- Album Tracks & Year -->
        <p class="text-xs text-brand-text-secondary/60">
          {i18n.t('playlists.songsCount', { count: item.album.track_count })} {#if item.album.year}({item.album.year}){/if}
        </p>
      {/if}
    </div>
  </div>
</div>

<style>
</style>
