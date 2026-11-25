<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { isLiveSong } from "../types";
  import CoverArt from "./CoverArt.svelte";

  // The immersive cover view: the current song's art beside its details. Below `md` the details
  // are hidden rather than stacked under the art, where they'd only repeat the dock.

  // Everything that differs between a window and fullscreen. A window keeps fixed sizes; in
  // fullscreen they follow the screen, the cover's side being the smaller of 65% of the height
  // (clear of the dock) and 45% of the width (leaving the details room).
  const WINDOWED = {
    frame: "w-56 h-56 xs:w-72 xs:h-72 md:w-[380px] md:h-[380px]",
    details: "max-w-md",
    pill: "",
    title: "text-4xl md:text-6xl",
    album: "text-lg md:text-xl",
    subtitle: "text-base md:text-lg",
  };
  const FULLSCREEN: typeof WINDOWED = {
    frame: "w-[min(65vh,45vw)] h-[min(65vh,45vw)] shrink-0",
    details: "max-w-[40vw]",
    pill: "text-[clamp(0.875rem,1.6vh,1.5rem)]",
    // A huge title can be wider than its column, so it may break inside a word.
    title: "text-[clamp(3.75rem,8vh,9rem)] pb-[0.1em] break-words",
    album: "text-[clamp(1.25rem,3vh,3rem)]",
    subtitle: "text-[clamp(1.125rem,2.6vh,2.5rem)]",
  };

  let sizes = $derived(windowLayoutStore.isFullscreen ? FULLSCREEN : WINDOWED);
  let song = $derived(playerStore.currentSong);
  let album = $derived(song?.album?.trim());
</script>

{#if song}
  <div
    data-testid="immersive-cover-frame"
    class="{sizes.frame} overflow-hidden shadow-[0_25px_50px_-12px_rgba(0,0,0,0.7)] border border-brand-border/40 hover:scale-[1.02] transition-transform duration-200 bg-brand-sidebar flex items-center justify-center relative select-none"
  >
    <CoverArt
      songId={song.id}
      artEmbedded={song.art_embedded}
      artAutomatic={song.art_automatic}
      artManual={song.art_manual}
      sizeClass="w-full h-full"
      fullResolution
      contain={isLiveSong(song)}
    />
  </div>

  <div
    data-testid="immersive-cover-details"
    class="hidden md:flex flex-col text-center md:text-left w-full {sizes.details}"
  >
    <span
      class="label-heading self-start px-3 py-1 bg-brand-accent/15 text-brand-accent-text border border-brand-border rounded-full select-none {sizes.pill}"
    >
      {i18n.t("playerBar.nowPlaying")}
    </span>
    <h1
      class="mt-3 font-heading font-bold text-brand-text-primary leading-[0.95] tracking-tight select-text text-balance {sizes.title}"
    >
      {playerStore.currentSongDisplayTitle}
    </h1>
    {#if album}
      <p class="mt-3 text-brand-text-secondary select-text font-semibold {sizes.album}">
        {album}
      </p>
    {/if}
    <p
      class="{album
        ? 'mt-0.5'
        : 'mt-3'} text-brand-text-secondary select-text font-medium {sizes.subtitle}"
    >
      {playerStore.currentSongDisplaySubtitle}
    </p>
  </div>
{/if}
