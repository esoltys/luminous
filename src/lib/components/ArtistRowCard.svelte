<script lang="ts">
  import type { ArtistItem, AlbumItem, Song } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import CoverArt from "./CoverArt.svelte";
  import GenreChips from "./GenreChips.svelte";
  import { getArtistCoverStack } from "../utils/covers";

  interface Props {
    artist: ArtistItem;
    artistAlbums: AlbumItem[];
    artistSongs?: Song[];
    onclick?: (e: MouseEvent) => void;
  }

  let { artist, artistAlbums, artistSongs = [], onclick: customClick }: Props = $props();

  // Same front-cover selection ArtistCard uses for its CoverStack (index 0 is
  // the front/topmost tile), so the row's single cover always matches it.
  let frontCover = $derived(getArtistCoverStack(artistAlbums, artistSongs, 1)[0]);
  let hasGenre = $derived(!!artist.genre?.trim());
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  role="button"
  tabindex="0"
  onclick={(e) => customClick?.(e)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); customClick?.(e as unknown as MouseEvent); } }}
  class="group grid grid-cols-[auto_1fr_auto] grid-rows-[auto_auto] items-center gap-x-3 gap-y-0.5 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
>
  <div class="row-span-2 relative overflow-hidden">
    <CoverArt
      songId={frontCover?.songId}
      artEmbedded={frontCover?.artEmbedded ?? false}
      artAutomatic={frontCover?.artAutomatic ?? null}
      artManual={frontCover?.artManual ?? null}
      sizeClass="w-11 h-11"
    />
  </div>

  <p class="col-span-2 min-w-0 truncate text-sm font-semibold text-brand-text-primary">{artist.name || i18n.t('collection.unknownArtist')}</p>

  <div class="min-w-0">
    {#if hasGenre}
      <GenreChips genre={artist.genre} />
    {:else}
      <p class="truncate text-xs text-brand-text-secondary font-medium">{i18n.t('artistDetail.unknownGenre')}</p>
    {/if}
  </div>

  <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate text-right">{i18n.t('playlists.songsCount', { count: artist.song_count })}</p>
</div>
