<script lang="ts">
  import type { AlbumItem } from "../types";
  import { collectionStore } from "../stores/collection.svelte";
  import CoverArt from "./CoverArt.svelte";
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
