<script lang="ts">
  import type { ArtistItem, AlbumItem, Song, ExtendedArtworkResponse } from "../types";
  import { getCoverArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import CoverArt from "./CoverArt.svelte";
  import GenreChips from "./GenreChips.svelte";
  import { getArtistCoverStack } from "../utils/covers";

  interface Props {
    artist: ArtistItem;
    artistAlbums: AlbumItem[];
    artistSongs?: Song[];
    onclick?: (e: MouseEvent) => void;
    prefix?: import("svelte").Snippet;
    suffix?: import("svelte").Snippet;
  }

  let {
    artist,
    artistAlbums,
    artistSongs = [],
    onclick: customClick,
    prefix,
    suffix,
  }: Props = $props();

  // Same front-cover selection ArtistCard uses for its CoverStack (index 0 is
  // the front/topmost tile), so the row's single cover always matches it.
  let frontCover = $derived(getArtistCoverStack(artistAlbums, artistSongs, 1)[0]);
  let hasGenre = $derived(!!artist.genre?.trim());

  // Locally-discovered artist portrait (#98/#761) — same as ArtistCard,
  // replaces the album-art composite when found.
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

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  role="button"
  tabindex="0"
  onclick={(e) => customClick?.(e)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); customClick?.(e as unknown as MouseEvent); } }}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none cursor-pointer w-full"
>
  {#if prefix}
    {@render prefix()}
  {/if}

  <div class="relative shrink-0 overflow-hidden">
    {#if artistPortraitUrl}
      <div class="w-11 h-11 relative overflow-hidden bg-brand-sidebar border border-brand-border shrink-0">
        <img
          src={artistPortraitUrl}
          alt={artist.name || i18n.t('collection.unknownArtist')}
          class="w-full h-full object-cover"
        />
      </div>
    {:else}
      <CoverArt
        songId={frontCover?.songId}
        artEmbedded={frontCover?.artEmbedded ?? false}
        artAutomatic={frontCover?.artAutomatic ?? null}
        artManual={frontCover?.artManual ?? null}
        sizeClass="w-11 h-11"
      />
    {/if}
  </div>

  <div class="min-w-0 flex-1 flex flex-col gap-0.5">
    <div class="flex items-center justify-between gap-2">
      <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">{artist.name || i18n.t('collection.unknownArtist')}</p>
    </div>
    <div class="min-w-0">
      {#if hasGenre}
        <GenreChips genre={artist.genre} />
      {:else}
        <p class="truncate text-xs text-brand-text-secondary font-medium">{i18n.t('artistDetail.unknownGenre')}</p>
      {/if}
    </div>
  </div>

  <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate shrink-0 text-right">{i18n.t('playlists.songsCount', { count: artist.song_count })}</p>

  {#if suffix}
    {@render suffix()}
  {/if}
</div>
