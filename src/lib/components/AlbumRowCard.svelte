<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { AlbumItem } from "../types";
  import { collectionStore } from "../stores/collection.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { queueAlbumAsPlaylist } from "../utils/playlist";
  import FavouriteCornerFlag from "./FavouriteCornerFlag.svelte";

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

  async function rateAlbum(rating: number) {
    if (!album.album) return;
    album.rating = await invoke<number>("set_album_rating", { album: album.album, rating });
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
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
>
  <div class="relative shrink-0 overflow-hidden">
    <CoverArt
      songId={album.sample_song_id ?? undefined}
      artEmbedded={album.art_embedded}
      artAutomatic={album.art_automatic}
      artManual={album.art_manual}
      sizeClass="w-11 h-11"
    />
    {#if album.rating === 5}
      <FavouriteCornerFlag size="sm" />
    {/if}
  </div>

  <div class="min-w-0 flex-1 flex flex-col gap-0.5">
    <div class="flex items-center justify-between gap-2">
      <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">{album.album || i18n.t('collection.unknownAlbum')}</p>
      <span class="text-xs text-brand-text-secondary font-medium tabular-nums shrink-0">{album.year || ""}</span>
    </div>
    <div class="flex items-center justify-between gap-2">
      <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">{album.artist || i18n.t('collection.variousArtists')}</p>
      <span class="shrink-0"><SongRating rating={album.rating} onRate={rateAlbum} size="sm" /></span>
    </div>
  </div>
</div>
