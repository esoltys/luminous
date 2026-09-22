<script lang="ts">
  import { playerStore } from "$lib/stores/player.svelte";
  import { i18n } from "$lib/stores/i18n.svelte";
  import type { QualityTier } from "$lib/types";
  import AudioPipelinePopover from "./AudioPipelinePopover.svelte";

  interface Props {
    class?: string;
    interactive?: boolean;
  }

  let { class: className = "", interactive = true }: Props = $props();

  let buttonEl = $state<HTMLButtonElement | null>(null);
  let isPopoverOpen = $state(false);

  const LOSSLESS_FORMATS = new Set(["flac", "alac", "wav", "wave", "aiff", "aif", "dsf", "dff"]);

  let tier = $derived.by<QualityTier>(() => {
    if (playerStore.audioPipeline?.quality_tier) {
      return playerStore.audioPipeline.quality_tier;
    }

    const song = playerStore.currentSong;
    if (!song) return "lq";

    const format = (song.filetype || "").toLowerCase().trim();
    const isLossless = LOSSLESS_FORMATS.has(format);

    if (isLossless) {
      const isHiRes = (song.samplerate ?? 0) > 48000 || (song.bitdepth != null && song.bitdepth > 16);
      return isHiRes ? "hi-res" : "hq";
    }

    const isSq = (song.bitrate ?? 0) >= 256;
    return isSq ? "sq" : "lq";
  });

  let label = $derived.by(() => {
    switch (tier) {
      case "hi-res":
        return "Hi-Res";
      case "hq":
        return "HQ";
      case "sq":
        return "SQ";
      case "lq":
      default:
        return "LQ";
    }
  });

  let tierDescription = $derived.by(() => {
    switch (tier) {
      case "hi-res":
        return i18n.t("audioPipeline.tierHiResDesc", {}, "Lossless format with sample rate above 48 kHz or bit depth above 16-bit");
      case "hq":
        return i18n.t("audioPipeline.tierHqDesc", {}, "Lossless format");
      case "sq":
        return i18n.t("audioPipeline.tierSqDesc", {}, "Lossy format at 256 kbps or higher");
      case "lq":
      default:
        return i18n.t("audioPipeline.tierLqDesc", {}, "Lossy format below 256 kbps");
    }
  });

  let styleClasses = $derived.by(() => {
    switch (tier) {
      case "hi-res":
        return "bg-brand-accent text-brand-accent-contrast font-black tracking-wider shadow-sm hover:brightness-110";
      case "hq":
        return "bg-brand-accent/20 text-brand-accent-text border border-brand-accent/40 font-bold hover:bg-brand-accent/30";
      case "sq":
        return "bg-brand-border/40 text-brand-text-primary border border-brand-border/70 font-semibold hover:border-brand-accent/50";
      case "lq":
      default:
        return "bg-brand-border/20 text-brand-text-secondary/80 border border-brand-border/40 font-medium hover:text-brand-text-primary";
    }
  });

  function togglePopover(e: MouseEvent) {
    e.stopPropagation();
    if (interactive) {
      isPopoverOpen = !isPopoverOpen;
    }
  }
</script>

{#if playerStore.currentSong}
  {#if interactive}
    <button
      bind:this={buttonEl}
      type="button"
      onclick={togglePopover}
      title={i18n.t("audioPipeline.badgeTooltip", { tier: label }, `Audio quality: ${label} — click to view audio pipeline`)}
      class="inline-flex items-center justify-center px-1.5 py-0.5 text-[9px] uppercase leading-none rounded transition-all cursor-pointer select-none active:scale-95 {styleClasses} {className}"
      aria-label={i18n.t("audioPipeline.badgeTooltip", { tier: label }, `Audio quality: ${label} — click to view audio pipeline`)}
      aria-expanded={isPopoverOpen}
      aria-haspopup="dialog"
    >
      {label}
    </button>

    <AudioPipelinePopover
      isOpen={isPopoverOpen}
      anchorEl={buttonEl}
      onClose={() => (isPopoverOpen = false)}
    />
  {:else}
    <span
      title={tierDescription}
      class="inline-flex items-center justify-center px-1.5 py-0.5 text-[9px] uppercase leading-none rounded select-none {styleClasses} {className}"
    >
      {label}
    </span>
  {/if}
{/if}
