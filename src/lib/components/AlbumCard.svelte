<script lang="ts">
  import type { AlbumItem, Song } from "../types";
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import CoverStack, { type CoverItem } from "./CoverStack.svelte";
  import LinkButton from "./LinkButton.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { getAlbumCategoryLabel } from "../utils/artist";

  interface Props {
    album: AlbumItem;
    covers?: CoverItem[];
    widthClass?: string;
    showArtist?: boolean;
    onclick?: (e: MouseEvent) => void;
    ondblclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
  }

  let {
    album,
    covers,
    widthClass = "w-full",
    showArtist = true,
    onclick: customClick,
    ondblclick: customDblClick,
    oncontextmenu: customContextMenu,
  }: Props = $props();

  function handleCardClick(e: MouseEvent) {
    if (customClick) {
      customClick(e);
    } else {
      collectionStore.viewAlbum(album.album || "");
    }
  }

  async function handleCardDblClick(e: MouseEvent) {
    if (customDblClick) {
      customDblClick(e);
    } else {
      const albumName = album.album || i18n.t('collection.unknownAlbum');
      const playlistName = i18n.t('collection.albumPlaylistName', { name: albumName });
      let existingPlaylist = playlistsStore.playlists.find(p => p.name === playlistName);

      if (existingPlaylist) {
        await playlistsStore.selectPlaylist(existingPlaylist.id);
        await playlistsStore.clearPlaylist(existingPlaylist.id);
      } else {
        await playlistsStore.createPlaylist(playlistName);
      }

      try {
        let songs = await invoke<Song[]>("get_songs_by_album", {
          album: album.album || "",
        });

        if (songs.length > 0) {
          const songIds = songs.map((s) => s.id);
          if (playlistsStore.activeCustomPlaylist) {
            await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, songIds);
            collectionStore.activeTab = "playlists";
            await playerStore.playPlaylistItem(playlistsStore.activeCustomPlaylist.id, 0);
          }
        }
      } catch (err) {
        console.error("Failed to add album to playlist:", err);
      }
    }
  }

</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={handleCardClick}
  ondblclick={handleCardDblClick}
  oncontextmenu={(e) => customContextMenu?.(e)}
  class="{widthClass} bg-brand-sidebar border border-brand-border/60 rounded-b-xl overflow-hidden flex flex-col group hover:border-brand-accent/40 transition-all duration-200 cursor-pointer select-none"
>
  <div
    class="aspect-square bg-brand-main flex items-center justify-center text-brand-accent-text relative overflow-hidden w-full"
  >
    <CoverStack
      covers={covers && covers.length > 0 ? covers : [{ artEmbedded: album.art_embedded, artAutomatic: album.art_automatic, artManual: album.art_manual }]}
      sizeClass={covers && covers.length > 1 ? "w-24 h-24" : "w-full h-full"}
    />
  </div>
  <div class="p-3.5 flex flex-col flex-1">
    <LinkButton
      onclick={(e) => { e.stopPropagation(); collectionStore.viewAlbum(album.album || ""); }}
      class="font-semibold text-sm text-brand-text-primary truncate w-full"
      title={i18n.t('collection.filterByAlbum', { album: album.album || i18n.t('collection.unknownAlbum') })}
    >
      {album.album || i18n.t('collection.unknownAlbum')}
    </LinkButton>
    <div class="flex items-center justify-between mt-0.5 gap-2">
      {#if showArtist}
        {#if album.artist}
          <LinkButton
            onclick={(e) => { e.stopPropagation(); collectionStore.viewArtist(album.artist || ""); }}
            class="text-xs text-brand-text-secondary truncate min-w-0 font-medium"
            title={i18n.t('collection.filterByArtist', { artist: album.artist })}
          >
            {album.artist}
          </LinkButton>
        {:else}
          <span class="text-xs text-brand-text-secondary text-left truncate min-w-0 font-medium">{i18n.t('collection.variousArtists')}</span>
        {/if}
      {:else}
        <span></span>
      {/if}
      <span class="text-xs text-brand-text-secondary font-medium shrink-0">{album.year || ""}</span>
    </div>
    <div class="flex items-center justify-between mt-0.5 text-xs text-brand-text-secondary font-medium gap-2">
      <span class="truncate min-w-0">{album.genre || ""}</span>
      <span class="shrink-0">{getAlbumCategoryLabel(album.track_count, album.disc_count)}</span>
    </div>
  </div>
</div>
