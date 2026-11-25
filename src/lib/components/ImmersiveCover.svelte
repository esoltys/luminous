<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { isLiveSong } from "../types";
  import CoverArt from "./CoverArt.svelte";

  // The immersive cover view: the current song's art beside its details. In a window both keep
  // a fixed size; in fullscreen they scale with the screen. The cover's side is the smaller of
  // 65% of the height (clear of the dock) and 45% of the width (leaving the details room).
  let fullscreen = $derived(windowLayoutStore.isFullscreen);
</script>

{#if playerStore.currentSong}
  <!-- Floating Cover Art Frame -->
  <div
    data-testid="immersive-cover-frame"
    class="{fullscreen
      ? 'w-[min(65vh,45vw)] h-[min(65vh,45vw)] shrink-0'
      : 'w-56 h-56 xs:w-72 xs:h-72 md:w-[380px] md:h-[380px]'} overflow-hidden shadow-[0_25px_50px_-12px_rgba(0,0,0,0.7)] border border-brand-border/40 hover:scale-[1.02] transition-transform duration-200 bg-brand-sidebar flex items-center justify-center relative select-none"
  >
    <CoverArt
      songId={playerStore.currentSong.id}
      artEmbedded={playerStore.currentSong.art_embedded}
      artAutomatic={playerStore.currentSong.art_automatic}
      artManual={playerStore.currentSong.art_manual}
      sizeClass="w-full h-full"
      fullResolution
      contain={isLiveSong(playerStore.currentSong)}
    />
  </div>

  <!-- Song Details Info: hidden below md, where it would stack under the cover art instead of
     sitting beside it. -->
  <div
    data-testid="immersive-cover-details"
    class="hidden md:flex flex-col text-center md:text-left w-full {fullscreen
      ? 'max-w-[40vw]'
      : 'max-w-md'}"
  >
    <span
      class="label-heading self-start px-3 py-1 bg-brand-accent/15 text-brand-accent-text border border-brand-border rounded-full select-none {fullscreen
        ? 'text-[clamp(0.875rem,1.6vh,1.5rem)]'
        : ''}"
    >
      {i18n.t("playerBar.nowPlaying")}
    </span>
    <h1
      class="mt-3 font-heading font-bold text-brand-text-primary leading-[0.95] tracking-tight select-text text-balance {fullscreen
        ? 'text-[clamp(3.75rem,8vh,9rem)] pb-[0.1em] break-words'
        : 'text-4xl md:text-6xl'}"
    >
      {playerStore.currentSongDisplayTitle}
    </h1>
    {#if playerStore.currentSong.album?.trim()}
      <p
        class="mt-3 text-brand-text-secondary select-text font-semibold {fullscreen
          ? 'text-[clamp(1.25rem,3vh,3rem)]'
          : 'text-lg md:text-xl'}"
      >
        {playerStore.currentSong.album}
      </p>
    {/if}
    <p
      class="{playerStore.currentSong.album?.trim()
        ? 'mt-0.5'
        : 'mt-3'} text-brand-text-secondary select-text font-medium {fullscreen
        ? 'text-[clamp(1.125rem,2.6vh,2.5rem)]'
        : 'text-base md:text-lg'}"
    >
      {playerStore.currentSongDisplaySubtitle}
    </p>
  </div>
{/if}
