<script lang="ts">
  import type { ArtistItem, AlbumItem, Song, ExtendedArtworkResponse } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import CoverArt from "./CoverArt.svelte";
  import GenreChips from "./GenreChips.svelte";
  import { getArtistCoverStack, resolveArtistPortraitUrl } from "../utils/covers";

  interface Props {
    artist: ArtistItem;
    artistAlbums: AlbumItem[];
    artistSongs?: Song[];
    onclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
    prefix?: import("svelte").Snippet;
    suffix?: import("svelte").Snippet;
    /** Proportional listening weight percentage [0, 100] for ranked charts (#1475). */
    progressPercent?: number;
  }

  let {
    artist,
    artistAlbums,
    artistSongs = [],
    onclick: customClick,
    oncontextmenu: customContextMenu,
    prefix,
    suffix,
    progressPercent,
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
  let artistProfile = $derived(collectionStore.getArtistProfile(artist.name));
  let artistPortraitUrl = $derived(
    resolveArtistPortraitUrl(artistArtwork?.artist_portrait_uri, artistProfile?.fetched_image_filename)
  );
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  role="button"
  tabindex="0"
  onclick={(e) => customClick?.(e)}
  oncontextmenu={(e) => customContextMenu?.(e)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); customClick?.(e as unknown as MouseEvent); } }}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none cursor-pointer w-full relative overflow-hidden"
>
  {#if typeof progressPercent === "number" && progressPercent > 0}
    <div
      class="accent-bar absolute inset-y-0 left-0 rounded-lg bg-brand-accent/15 pointer-events-none transition-[width] duration-300 ease-out motion-reduce:transition-none"
      style="width: {Math.min(100, Math.max(0, progressPercent))}%;"
      data-testid="stats-accent-bar"
    ></div>
  {/if}

  {#if prefix}
    <div class="relative z-10">
      {@render prefix()}
    </div>
  {/if}

  <div class="relative z-10 shrink-0 overflow-hidden">
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

  <div class="relative z-10 min-w-0 flex-1 flex flex-col gap-0.5">
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

  <p class="relative z-10 text-xs text-brand-text-secondary font-medium tabular-nums truncate shrink-0 text-right">{i18n.t('playlists.songsCount', { count: artist.song_count })}</p>

  {#if suffix}
    <div class="relative z-10">
      {@render suffix()}
    </div>
  {/if}
</div>
