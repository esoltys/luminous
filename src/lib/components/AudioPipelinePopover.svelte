<script lang="ts">
  import { onMount, tick } from "svelte";
  import { cubicIn, cubicOut } from "svelte/easing";
  import { portal } from "$lib/utils/portal";
  import { playerStore } from "$lib/stores/player.svelte";
  import { i18n } from "$lib/stores/i18n.svelte";
  import AudioPipelineStages from "./AudioPipelineStages.svelte";
  import {
    XIcon as X,
    PulseIcon as Activity,
  } from "phosphor-svelte";

  interface Props {
    isOpen: boolean;
    anchorEl: HTMLElement | null;
    onClose: () => void;
  }

  let { isOpen, anchorEl, onClose }: Props = $props();

  let popoverEl = $state<HTMLDivElement | null>(null);
  let coords = $state<{
    left: number;
    top: number | undefined;
    bottom: number | undefined;
    maxHeight: number;
    originX: number;
    isAbove: boolean;
  }>({
    left: 0,
    top: undefined,
    bottom: undefined,
    maxHeight: 520,
    originX: 190,
    isAbove: true,
  });

  function updatePosition() {
    if (!anchorEl || typeof window === "undefined") return;
    const rect = anchorEl.getBoundingClientRect();
    const popoverWidth = popoverEl?.offsetWidth || 380;

    // Prefer opening upwards if the anchor is in the lower half of the screen
    // or has insufficient clearance below for the pipeline drawer
    const spaceBelow = window.innerHeight - rect.bottom;
    const placeAbove = spaceBelow < 480 || rect.top > window.innerHeight / 2;

    // Horizontally center directly over the anchor badge
    let left = rect.left + rect.width / 2 - popoverWidth / 2;
    // Clamp horizontally inside viewport with safe margins
    left = Math.max(16, Math.min(left, window.innerWidth - popoverWidth - 16));

    // Anchor transform-origin directly on the triggering badge's horizontal center
    const originX = Math.max(20, Math.min(Math.round(rect.left + rect.width / 2 - left), popoverWidth - 20));

    if (placeAbove) {
      // Anchor bottom edge above the trigger element with a clean margin
      const bottom = Math.max(12, window.innerHeight - rect.top + 12);
      const maxHeight = Math.max(260, rect.top - 28);
      coords = { left, top: undefined, bottom, maxHeight, originX, isAbove: true };
    } else {
      // Anchor top edge below the trigger element
      const top = Math.max(12, rect.bottom + 8);
      const maxHeight = Math.max(260, window.innerHeight - rect.bottom - 24);
      coords = { left, top, bottom: undefined, maxHeight, originX, isAbove: false };
    }
  }

  let popoverStyle = $derived.by(() => {
    const parts: string[] = ['width: 380px'];
    if (coords.bottom !== undefined) {
      parts.push(`bottom: ${coords.bottom}px`);
      parts.push(`transform-origin: ${coords.originX}px bottom`);
    } else if (coords.top !== undefined) {
      parts.push(`top: ${coords.top}px`);
      parts.push(`transform-origin: ${coords.originX}px top`);
    }
    parts.push(`left: ${coords.left}px`);
    parts.push(`max-height: ${coords.maxHeight}px`);
    return parts.join('; ');
  });

  function bloomIn(_node: HTMLElement) {
    const isAbove = coords.isAbove;
    return {
      duration: 180,
      easing: cubicOut,
      css: (t: number) => {
        const scale = 0.88 + 0.12 * t;
        const y = (isAbove ? 8 : -8) * (1 - t);
        return `transform: scale(${scale}) translateY(${y}px); opacity: ${t};`;
      },
    };
  }

  function bloomOut(_node: HTMLElement) {
    const isAbove = coords.isAbove;
    return {
      duration: 150,
      easing: cubicIn,
      css: (t: number) => {
        const scale = 0.88 + 0.12 * t;
        const y = (isAbove ? 8 : -8) * (1 - t);
        return `transform: scale(${scale}) translateY(${y}px); opacity: ${t};`;
      },
    };
  }

  $effect(() => {
    if (isOpen) {
      updatePosition();
      tick().then(() => {
        updatePosition();
      });
    }
  });

  function handleWindowClick(e: MouseEvent) {
    if (!isOpen) return;
    const target = e.target as Node;
    if (popoverEl && !popoverEl.contains(target) && anchorEl && !anchorEl.contains(target)) {
      onClose();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!isOpen) return;
    if (e.key === "Escape") {
      onClose();
    }
  }

  onMount(() => {
    window.addEventListener("mousedown", handleWindowClick);
    window.addEventListener("keydown", handleKeydown);
    window.addEventListener("resize", updatePosition);
    return () => {
      window.removeEventListener("mousedown", handleWindowClick);
      window.removeEventListener("keydown", handleKeydown);
      window.removeEventListener("resize", updatePosition);
    };
  });
</script>

{#if isOpen}
  <div
    use:portal
    bind:this={popoverEl}
    style={popoverStyle}
    in:bloomIn
    out:bloomOut
    class="fixed z-[100] bg-brand-surface/95 backdrop-blur-xl border border-brand-border/60 rounded-xl shadow-2xl overflow-hidden flex flex-col"
    role="dialog"
    aria-modal="true"
    aria-label={i18n.t('audioPipeline.title', {}, 'Audio Pipeline')}
  >
    <!-- Header -->
    <div class="flex items-center justify-between px-4 py-3 border-b border-brand-border/40 bg-brand-bg/50 shrink-0">
      <div class="flex items-center gap-2">
        <Activity class="w-4 h-4 text-brand-text-primary" />
        <h2 class="text-xs font-bold uppercase tracking-wider text-brand-text-primary">
          {i18n.t('audioPipeline.title', {}, 'Audio Pipeline')}
        </h2>
      </div>

      <button
        type="button"
        onclick={onClose}
        class="p-1 rounded-md text-brand-text-primary hover:bg-brand-border/40 transition-colors"
        title={i18n.t('audioPipeline.close', {}, 'Close audio pipeline')}
        aria-label={i18n.t('audioPipeline.close', {}, 'Close audio pipeline')}
      >
        <X class="w-3.5 h-3.5" />
      </button>
    </div>

    <!-- Body -->
    <div class="p-4 overflow-y-auto custom-scrollbar flex-1 min-h-0">
      <AudioPipelineStages pipeline={playerStore.audioPipeline} />
    </div>
  </div>
{/if}
