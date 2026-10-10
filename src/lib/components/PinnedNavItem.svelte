<script lang="ts">
  import type { NavigablePin } from "../utils/pinnedNav";
  import CoverArt from "./CoverArt.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { resolveArtistPortraitUrl, getArtistCoverStack } from "../utils/covers";
  import { getArtistAlbums, getArtistSongs } from "../utils/artist";
  import type { ExtendedArtworkResponse } from "../types";

  interface Props {
    pin: NavigablePin;
    collapsed?: boolean;
    index?: number;
    isDragged?: boolean;
    isDragOver?: boolean;
    dropIndicatorPosition?: "top" | "bottom";
    onclick?: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
    onpointerdown?: (e: PointerEvent) => void;
    onkeydown?: (e: KeyboardEvent) => void;
  }

  let {
    pin,
    collapsed = false,
    index,
    isDragged = false,
    isDragOver = false,
    dropIndicatorPosition = "top",
    onclick,
    oncontextmenu,
    onpointerdown,
    onkeydown,
  }: Props = $props();

  let artistArtwork = $state<ExtendedArtworkResponse | null>(null);

  $effect(() => {
    if (pin.type !== "artist" || !pin.artist?.name) {
      artistArtwork = null;
      return;
    }
    let cancelled = false;
    collectionStore.getExtendedArtworkForArtist(pin.artist.name).then((res) => {
      if (!cancelled) artistArtwork = res;
    });
    return () => {
      cancelled = true;
    };
  });

  let artistProfile = $derived(pin.artist?.name ? collectionStore.getArtistProfile(pin.artist.name) : null);
  let artistPortraitUrl = $derived(
    pin.type === "artist"
      ? resolveArtistPortraitUrl(artistArtwork?.artist_portrait_uri, artistProfile?.fetched_image_filename)
      : null
  );

  let artistFrontCover = $derived.by(() => {
    if (pin.type !== "artist" || !pin.artist?.name) return null;
    const albums = getArtistAlbums(collectionStore.albums, pin.artist.name);
    const songs = getArtistSongs(collectionStore.songs, pin.artist.name);
    return getArtistCoverStack(albums, songs, 1)[0] ?? null;
  });

  const IconComponent = $derived(pin.icon);
  const tooltipText = $derived(pin.subtitle ? `${pin.title} • ${pin.subtitle}` : pin.title);
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  role="button"
  tabindex="0"
  data-pinned-nav-index={index}
  {onclick}
  {oncontextmenu}
  {onpointerdown}
  {onkeydown}
  ondragstart={(e) => e.preventDefault()}
  title={tooltipText}
  class="relative group transition-colors select-none text-left cursor-grab active:cursor-grabbing {isDragged ? 'opacity-40' : ''} {collapsed
    ? 'w-10 h-10 flex items-center justify-center rounded-xl mx-auto'
    : 'w-full flex items-center gap-3 px-3 py-1 rounded-lg'} {pin.isActive
    ? 'bg-brand-accent/20 text-brand-accent-text font-semibold'
    : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'}"
>
  {#if isDragOver}
    {#if collapsed}
      <div class="absolute inset-0 rounded-xl ring-2 ring-brand-accent pointer-events-none z-10"></div>
    {:else}
      <div
        class="absolute left-1 right-1 h-0.5 bg-brand-accent rounded-full pointer-events-none z-10 {dropIndicatorPosition === 'bottom' ? '-bottom-0.5' : '-top-0.5'}"
      ></div>
    {/if}
  {/if}

  <!-- Artwork / Thumbnail Container (square with no rounded corners) -->
  <div
    class="shrink-0 relative overflow-hidden bg-brand-main/60 flex items-center justify-center {collapsed
      ? 'w-8 h-8'
      : 'w-7 h-7'}"
  >
    {#if pin.type === "song" || pin.type === "album"}
      <CoverArt
        songId={pin.coverArt?.songId}
        artEmbedded={pin.coverArt?.artEmbedded}
        artAutomatic={pin.coverArt?.artAutomatic}
        artManual={pin.coverArt?.artManual}
        sizeClass={collapsed ? "w-8 h-8" : "w-7 h-7"}
      />
    {:else if pin.type === "artist"}
      {#if artistPortraitUrl}
        <img
          src={artistPortraitUrl}
          alt={pin.title}
          class="w-full h-full object-cover"
        />
      {:else if artistFrontCover}
        <CoverArt
          songId={artistFrontCover.songId}
          artEmbedded={artistFrontCover.artEmbedded}
          artAutomatic={artistFrontCover.artAutomatic}
          artManual={artistFrontCover.artManual}
          sizeClass={collapsed ? "w-8 h-8" : "w-7 h-7"}
        />
      {:else}
        <IconComponent class={collapsed ? "w-4 h-4 text-brand-text-secondary" : "w-3.5 h-3.5 text-brand-text-secondary"} />
      {/if}
    {:else}
      {@const iconColor = pin.iconColorClass ?? "text-brand-accent-text"}
      <IconComponent class="{collapsed ? 'w-4 h-4' : 'w-3.5 h-3.5'} {iconColor} {pin.autoPlaylist?.kind === 'favourites' ? 'fill-current' : ''}" />
    {/if}
  </div>

  <!-- Text metadata (expanded only) -->
  {#if !collapsed}
    <div class="flex-1 min-w-0 flex flex-col justify-center leading-tight">
      <span class="truncate text-xs font-medium text-brand-text-primary">
        {pin.title}
      </span>
      {#if pin.subtitle}
        <span class="truncate text-[11px] text-brand-text-secondary/80">
          {pin.subtitle}
        </span>
      {/if}
    </div>
  {/if}
</div>
