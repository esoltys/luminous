<script lang="ts">
  import type { ArtistItem, AlbumItem, Song, ExtendedArtworkResponse } from "../types";
  import { getCoverArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import CoverStack from "./CoverStack.svelte";
  import GenreChips from "./GenreChips.svelte";
  import { getArtistCoverStack } from "../utils/covers";

  interface Props {
    artist: ArtistItem;
    artistAlbums: AlbumItem[];
    artistSongs?: Song[];
    fullAlbumCount?: number;
    onclick?: (e: MouseEvent) => void;
  }

  let {
    artist,
    artistAlbums,
    artistSongs = [],
    fullAlbumCount: _fullAlbumCount,
    onclick: customClick,
  }: Props = $props();

  let covers = $derived(getArtistCoverStack(artistAlbums, artistSongs));

  let hasGenre = $derived(!!artist.genre?.trim());

  // Locally-discovered artist portrait (#98/#761) — replaces the album-art
  // CoverStack composite when found, since a real photo of the artist beats
  // a collage of their albums' covers.
  let artistArtwork = $state<ExtendedArtworkResponse | null>(null);
  $effect(() => {
    const name = artist.name;
    if (!name) {
      artistArtwork = null;
      return;
    }
    let cancelled = false;
    collectionStore.getExtendedArtworkForArtist(name).then((result) => {
      if (!cancelled) artistArtwork = result;
    });
    return () => {
      cancelled = true;
    };
  });
  let artistPortraitUrl = $derived(getCoverArtUrl(artistArtwork?.artist_portrait_uri));
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  role="button"
  tabindex="0"
  onclick={(e) => customClick?.(e)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); customClick?.(e as unknown as MouseEvent); } }}
  class="artist-card group bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col items-start text-left outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
>
  <div class="aspect-square w-full mb-3 bg-brand-main relative flex items-center justify-center overflow-hidden">
    {#if artistPortraitUrl}
      <div class="w-36 h-36 relative overflow-hidden bg-brand-sidebar border border-brand-border shrink-0">
        <img
          src={artistPortraitUrl}
          alt={artist.name || i18n.t('collection.unknownArtist')}
          class="w-full h-full object-cover"
        />
      </div>
    {:else}
      <CoverStack
        covers={covers}
        fallbackName={artist.name || i18n.t('collection.unknownArtist')}
        sizeClass="w-36 h-36"
      />
    {/if}
  </div>

  <span
    class="font-semibold text-sm text-brand-text-primary group-hover:text-brand-accent-text group-hover:underline transition-all duration-150 text-left truncate w-full"
    title={i18n.t('collection.filterByArtist', { artist: artist.name || i18n.t('collection.unknownArtist') })}
  >
    {artist.name || i18n.t('collection.unknownArtist')}
  </span>
  <span class="text-xs text-brand-text-secondary font-medium truncate w-full mt-0.5 text-left">
    {i18n.t('playlists.songsCount', { count: artist.song_count })}
  </span>
  <div class="w-full mt-1.5 flex justify-start">
    {#if hasGenre}
      <GenreChips genre={artist.genre} />
    {:else}
      <span class="text-xs text-brand-text-secondary font-medium truncate text-left">
        {i18n.t('artistDetail.unknownGenre')}
      </span>
    {/if}
  </div>
</div>

<style>
  .artist-card {
    container-type: inline-size;
  }
</style>
