<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { AlbumItem } from "../types";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import CoverStack, { type CoverItem } from "./CoverStack.svelte";
  import LinkButton from "./LinkButton.svelte";
  import SongRating from "./SongRating.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { queueAlbumAsPlaylist } from "../utils/playlist";
  import FavouriteCornerFlag from "./FavouriteCornerFlag.svelte";
  import BoxSetDiscIcons from "./BoxSetDiscIcons.svelte";
  import GenreChips from "./GenreChips.svelte";
  import LibraryBadge from "./LibraryBadge.svelte";
 
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
      navigationStore.viewAlbum(album.album || "");
    }
  }

  async function handleCardDblClick(e: MouseEvent) {
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

  let albumDirectory = $derived(collectionStore.getDirectoryForAlbum(album));
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  onclick={handleCardClick}
  ondblclick={handleCardDblClick}
  oncontextmenu={(e) => customContextMenu?.(e)}
  class="{widthClass} bg-brand-sidebar border border-brand-border/60 rounded-b-xl overflow-hidden flex flex-col group hover:border-brand-accent/40 transition-all duration-200 select-none"
>
  <div
    class="aspect-square bg-brand-main flex items-center justify-center text-brand-accent-text relative overflow-hidden w-full"
  >
    <CoverStack
      covers={covers && covers.length > 0 ? covers : [{ artEmbedded: album.art_embedded, artAutomatic: album.art_automatic, artManual: album.art_manual }]}
      sizeClass={covers && covers.length > 1 ? "w-24 h-24" : "w-full h-full"}
    />
    {#if album.rating === 5}
      <FavouriteCornerFlag size="md" />
    {/if}
    {#if album.disc_count > 1}
      <BoxSetDiscIcons discCount={album.disc_count} size="md" />
    {/if}
    {#if albumDirectory}
      <div class="absolute top-2 left-2 z-10 opacity-0 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none max-w-[calc(100%-1rem)]">
        <LibraryBadge directory={albumDirectory} size="xs" />
      </div>
    {/if}
  </div>
  <div class="p-3.5 flex flex-col flex-1">
    <LinkButton
      onclick={(e) => { e.stopPropagation(); navigationStore.viewAlbum(album.album || ""); }}
      class="font-semibold text-sm text-brand-text-primary truncate w-full"
      title={i18n.t('collection.filterByAlbum', { album: album.album || i18n.t('collection.unknownAlbum') })}
    >
      {album.album || i18n.t('collection.unknownAlbum')}
    </LinkButton>
    <div class="flex items-center justify-between mt-0.5 gap-2">
      {#if showArtist}
        {#if album.artist}
          <LinkButton
            onclick={(e) => { e.stopPropagation(); navigationStore.viewArtist(album.artist || ""); }}
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
    <div class="flex items-center justify-between mt-0.5 gap-2">
      <div class="min-w-0"><GenreChips genre={album.genre} /></div>
      <span class="shrink-0"><SongRating rating={album.rating} onRate={rateAlbum} size="sm" /></span>
    </div>
  </div>
</div>
