<script lang="ts">
  import type { ArtistItem, AlbumItem, Song } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import CoverArt from "./CoverArt.svelte";
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
  let genreLabel = $derived(artist.genre?.trim() || i18n.t('artistDetail.unknownGenre'));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  role="button"
  tabindex="0"
  onclick={(e) => customClick?.(e)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); customClick?.(e as unknown as MouseEvent); } }}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 hover:border-brand-accent/40 hover:shadow-md hover:shadow-brand-accent/10 transition-all duration-200 cursor-pointer select-none"
>
  <div class="relative shrink-0 overflow-hidden">
    <CoverArt
      songId={frontCover?.songId}
      artEmbedded={frontCover?.artEmbedded ?? false}
      artAutomatic={frontCover?.artAutomatic ?? null}
      artManual={frontCover?.artManual ?? null}
      sizeClass="w-11 h-11"
    />
  </div>

  <div class="min-w-0 flex-1">
    <p class="truncate text-sm font-semibold text-brand-text-primary">{artist.name || i18n.t('collection.unknownArtist')}</p>
    <p class="truncate text-xs text-brand-text-secondary font-medium">{genreLabel}</p>
  </div>

  <div class="shrink-0 max-w-40 text-right">
    <p class="text-xs text-brand-text-secondary font-medium tabular-nums truncate">{i18n.t('playlists.songsCount', { count: artist.song_count })}</p>
  </div>
</div>
