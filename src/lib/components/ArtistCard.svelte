<script lang="ts">
  import type { ArtistItem, AlbumItem } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import CoverStack from "./CoverStack.svelte";

  interface Props {
    artist: ArtistItem;
    artistAlbums: AlbumItem[];
    fullAlbumCount?: number;
    onclick?: (e: MouseEvent) => void;
  }

  let {
    artist,
    artistAlbums,
    fullAlbumCount: _fullAlbumCount,
    onclick: customClick,
  }: Props = $props();

  let covers = $derived(
    artistAlbums.map((album) => ({
      artEmbedded: album.art_embedded,
      artAutomatic: album.art_automatic,
      artManual: album.art_manual,
    }))
  );

  let genreLabel = $derived(artist.genre?.trim() || i18n.t('artistDetail.unknownGenre'));
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  role="button"
  tabindex="0"
  onclick={(e) => customClick?.(e)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); customClick?.(e as unknown as MouseEvent); } }}
  class="artist-card group bg-brand-sidebar border border-brand-border/60 rounded-xl p-4 flex flex-col items-center text-center hover:border-brand-accent/40 transition-all duration-200 cursor-pointer select-none"
>
  <div class="w-full flex justify-center items-center mt-2 mb-4">
    <CoverStack
      covers={covers}
      fallbackName={artist.name || i18n.t('collection.unknownArtist')}
      sizeClass="w-24 h-24"
    />
  </div>

  <span
    class="font-semibold text-sm text-brand-text-primary group-hover:text-brand-accent-text group-hover:underline transition-all duration-150 text-center truncate w-full"
    title={i18n.t('collection.filterByArtist', { artist: artist.name || i18n.t('collection.unknownArtist') })}
  >
    {artist.name || i18n.t('collection.unknownArtist')}
  </span>
  <div class="flex gap-2 justify-center mt-2 text-[10px] text-brand-text-secondary/50">
    <span>{genreLabel}</span>
    <span>•</span>
    <span>{i18n.t('playlists.songsCount', { count: artist.song_count })}</span>
  </div>
</div>

<style>
  .artist-card {
    container-type: inline-size;
  }
</style>
