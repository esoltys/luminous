<script lang="ts">
  import type { HomeItem } from "../types";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import CoverArt from "./CoverArt.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import { Play } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { isSmartPlaylistSpec } from "../utils/filterParser";

  let { item }: { item: HomeItem } = $props();

  // Mirrors ArtistDetailView's openPlaylist: genre/decade auto-playlists open
  // in AutoPlaylistDetailView, custom playlists (including Smart Playlists,
  // which are also dynamic_enabled) open in the regular PlaylistView.
  function openPlaylist() {
    if (item.type !== "playlist") return;
    const playlist = item.playlist;
    if (playlist.dynamic_enabled && !isSmartPlaylistSpec(playlist.dynamic_spec)) {
      const isDecade = playlist.dynamic_spec?.startsWith("decade:") ?? false;
      collectionStore.viewAutoPlaylist(
        isDecade
          ? { kind: "decade", decade: playlist.dynamic_spec?.replace(/^decade:/, "") ?? playlist.name, playlistId: playlist.id, updated: playlist.updated }
          : { kind: "genre", genre: playlist.dynamic_spec ?? playlist.name, playlistId: playlist.id, updated: playlist.updated }
      );
      return;
    }
    playlistsStore.selectPlaylist(playlist.id);
    collectionStore.viewPlaylist(playlist.id);
  }

  async function handlePlay(e: MouseEvent) {
    e.stopPropagation();
    if (item.type === "song") {
      await playerStore.playSong(item.song.id);
    }
  }
</script>

{#if item.type === "album"}
  <AlbumCard album={item.album} widthClass="w-48 shrink-0" />
{:else if item.type === "playlist"}
  <PlaylistCard playlist={item.playlist} widthClass="w-48 shrink-0" onClick={openPlaylist} />
{:else}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="flex-shrink-0 w-48 group relative">
    <!-- Card Container -->
    <div class="relative overflow-hidden rounded-xl bg-brand-sidebar border border-brand-border/60 transition-all duration-200 hover:border-brand-accent/40 flex flex-col h-full">
      <!-- Cover Art -->
      <div class="relative aspect-square overflow-hidden bg-brand-sidebar w-full">
        <CoverArt
          songId={item.song.id}
          artEmbedded={item.song.art_embedded}
          artAutomatic={item.song.art_automatic}
          artManual={item.song.art_manual}
          sizeClass="w-full h-full"
        />

        <!-- Play Button Overlay -->
        <div class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity z-20">
          <button
            onclick={handlePlay}
            class="w-12 h-12 rounded-full bg-brand-accent text-brand-accent-contrast flex items-center justify-center scale-75 group-hover:scale-100 transition-transform cursor-pointer"
            title={i18n.t('playerBar.play')}
          >
            <Play class="w-5 h-5 fill-current ml-0.5" />
          </button>
        </div>
      </div>

      <!-- Metadata -->
      <div class="p-3.5 flex flex-col flex-1">
        <!-- Song Title -->
        <h3 class="font-semibold text-sm text-brand-text-primary truncate" title={item.song.title}>
          {item.song.title || i18n.t('collection.unknownSong')}
        </h3>

        <!-- Song Artist -->
        <p class="text-xs text-brand-text-secondary truncate mt-0.5 font-medium" title={item.song.artist}>
          {item.song.artist || i18n.t('collection.unknownArtist')}
        </p>
      </div>
    </div>
  </div>
{/if}

<style>
</style>
