<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { AlbumItem, Song } from "../types";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import CoverArt from "./CoverArt.svelte";
  import { Play } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { getAlbumCategoryLabel } from "../utils/artist";
  import { queueAlbumAsPlaylist } from "../utils/playlist";

  interface Props {
    album: AlbumItem;
    onclick?: (e: MouseEvent) => void;
    ondblclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
  }

  let {
    album,
    onclick: customClick,
    ondblclick: customDblClick,
    oncontextmenu: customContextMenu,
  }: Props = $props();

  function handleClick(e: MouseEvent) {
    if (customClick) {
      customClick(e);
    } else {
      collectionStore.viewAlbum(album.album || "");
    }
  }

  async function handleDblClick(e: MouseEvent) {
    if (customDblClick) {
      customDblClick(e);
    } else {
      await queueAlbumAsPlaylist(album);
    }
  }

  async function playAlbum(e: MouseEvent) {
    e.stopPropagation();
    const songs = await invoke<Song[]>("get_songs_by_album", { album: album.album || "" });
    if (songs.length > 0) {
      playerStore.playSongs(songs.map((s) => s.id), 0, undefined, {
        type: "album",
        album: album.album || "",
        albumArtist: album.artist ?? undefined,
      });
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  role="button"
  tabindex="0"
  onclick={handleClick}
  ondblclick={handleDblClick}
  oncontextmenu={(e) => customContextMenu?.(e)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); handleClick(e as unknown as MouseEvent); } }}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 hover:border-brand-accent/40 hover:shadow-md hover:shadow-brand-accent/10 transition-all duration-200 cursor-pointer select-none"
>
  <div class="relative shrink-0 overflow-hidden">
    <CoverArt
      songId={album.sample_song_id ?? undefined}
      artEmbedded={album.art_embedded}
      artAutomatic={album.art_automatic}
      artManual={album.art_manual}
      sizeClass="w-11 h-11"
    />
    <button
      onclick={playAlbum}
      class="absolute inset-0 z-20 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer"
      title={i18n.t('playerBar.play')}
    >
      <Play class="w-4 h-4 text-white fill-current" />
    </button>
  </div>

  <div class="min-w-0 flex-1">
    <p class="truncate text-sm font-semibold text-brand-text-primary">{album.album || i18n.t('collection.unknownAlbum')}</p>
    <p class="truncate text-xs text-brand-text-secondary font-medium">{album.artist || i18n.t('collection.variousArtists')}</p>
  </div>

  <div class="shrink-0 max-w-40 text-right">
    <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate">{getAlbumCategoryLabel(album.track_count, album.disc_count)}</p>
    {#if album.year}
      <p class="text-xs text-brand-text-secondary truncate">{album.year}</p>
    {/if}
  </div>
</div>
