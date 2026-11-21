<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { visualizerStore } from "../stores/visualizer.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { isLiveSong } from "../types";
  import CoverArt from "./CoverArt.svelte";
  import VisualizerCanvas from "./VisualizerCanvas.svelte";
  import VisualizerPresetMenu from "./VisualizerPresetMenu.svelte";

  let { active = true }: { active?: boolean } = $props();

  let detailsHeight = $state(0);
  let presetMenu = $state<{ x: number; y: number } | null>(null);

  function openPresetMenu(e: MouseEvent) {
    if (!visualizerStore.canSwitch) return;
    e.preventDefault();
    presetMenu = { x: e.clientX, y: e.clientY };
  }
</script>

<div
  data-testid="immersive-visualizer"
  class="relative z-10 flex flex-col w-full max-w-5xl h-full max-h-[600px] gap-4 md:gap-6 justify-center"
>
  {#if playerStore.currentSong}
    <!-- Compact Song Context Header: Now playing pill sits above the heading, while the square
         album artwork spans from the cap-top of the heading to the descender-bottom of the artist line.
         The leading-none truncated lines carry pb/-mb 0.2em so their overflow clip box reaches below
         the 1em line box and descenders (g, y, p) are not cut off, without changing the layout. -->
    <div class="flex items-end gap-4 px-1">
      <button
        type="button"
        onclick={() => windowLayoutStore.toggleImmersiveMode("cover")}
        title={i18n.t("playerBar.immersiveTitle", {}, "Immersive Mode")}
        style={detailsHeight > 0
          ? `width: ${detailsHeight}px; height: ${detailsHeight}px;`
          : undefined}
        class="w-16 h-16 md:w-[4.5rem] md:h-[4.5rem] rounded-xl overflow-hidden border border-brand-border/50 bg-brand-sidebar flex-shrink-0 shadow-md hover:scale-105 active:scale-95 transition-transform duration-200 focus:outline-hidden focus-visible:ring-2 focus-visible:ring-brand-accent"
      >
        <CoverArt
          songId={playerStore.currentSong.id}
          artEmbedded={playerStore.currentSong.art_embedded}
          artAutomatic={playerStore.currentSong.art_automatic}
          artManual={playerStore.currentSong.art_manual}
          sizeClass="w-full h-full"
          contain={isLiveSong(playerStore.currentSong)}
        />
      </button>

      <div class="flex flex-col min-w-0 flex-1">
        <span
          class="label-small self-start mb-2 px-2.5 py-0.5 bg-brand-accent/15 text-brand-accent-text border border-brand-border rounded-full select-none"
        >
          {i18n.t("playerBar.nowPlaying")}
        </span>
        <div
          bind:clientHeight={detailsHeight}
          data-testid="immersive-visualizer-song-details"
          class="flex flex-col min-w-0"
        >
          <h1
            class="text-2xl md:text-3xl font-heading font-bold text-brand-text-primary leading-none -mt-[0.12em] pb-[0.2em] -mb-[0.2em] tracking-tight truncate select-text"
          >
            {playerStore.currentSongDisplayTitle}
          </h1>
          {#if playerStore.currentSong.album?.trim()}
            <p
              class="mt-1.5 text-sm md:text-base text-brand-text-secondary font-semibold leading-tight truncate select-text"
            >
              {playerStore.currentSong.album}
            </p>
          {/if}
          <p
            class="{playerStore.currentSong.album?.trim()
              ? 'mt-1'
              : 'mt-1.5'} text-xs md:text-sm text-brand-text-secondary/70 font-medium leading-none pb-[0.2em] -mb-[0.2em] truncate select-text"
          >
            {playerStore.currentSongDisplaySubtitle}
          </p>
        </div>
      </div>
    </div>
  {/if}

  <!-- Visualizer stage: draws the selected preset; right-click lists the presets -->
  <div
    oncontextmenu={openPresetMenu}
    role="presentation"
    class="flex-1 min-h-[220px] md:min-h-[320px] w-full rounded-2xl border border-brand-border/40 bg-brand-sidebar/65 p-4 md:p-6 flex items-center justify-center shadow-[0_25px_50px_-12px_rgba(0,0,0,0.45)]"
  >
    <VisualizerCanvas
      preset={visualizerStore.preset}
      size="full"
      {active}
      testId="immersive-visualizer-canvas"
      class="w-full h-full block"
    />
  </div>
</div>

{#if presetMenu}
  <VisualizerPresetMenu x={presetMenu.x} y={presetMenu.y} onClose={() => (presetMenu = null)} />
{/if}
