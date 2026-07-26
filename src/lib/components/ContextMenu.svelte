<script lang="ts">
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { playerStore } from "../stores/player.svelte";
  import { portal } from "../utils/portal";

  interface Props {
    x: number;
    y: number;
    onClose: () => void;
    /** Approximate rendered height of the menu, used to clamp it inside the viewport. */
    estimatedHeight?: number;
    children: Snippet;
  }

  let { x, y, onClose, estimatedHeight = 200, children }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);

  // Keep menu inside viewport boundaries
  let adjustedX = $derived.by(() => {
    if (typeof window === "undefined") return x;
    const menuWidth = 200;
    return Math.min(x, window.innerWidth - menuWidth - 10);
  });

  let adjustedY = $derived.by(() => {
    if (typeof window === "undefined") return y;
    const menuHeight = estimatedHeight;
    // The floating PlayerBar dock (h-20 + bottom-4 inset ≈ 96px) sits on top
    // of page content whenever a song is loaded — clamp above it so the
    // menu's lower items aren't hidden underneath it.
    const dockClearance = playerStore.currentSong ? 96 : 0;
    return Math.min(y, window.innerHeight - menuHeight - dockClearance - 10);
  });

  function handleWindowClick(e: MouseEvent) {
    if (menuEl && !menuEl.contains(e.target as Node)) {
      onClose();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onClose();
    }
  }

  onMount(() => {
    window.addEventListener("mousedown", handleWindowClick);
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("mousedown", handleWindowClick);
      window.removeEventListener("keydown", handleKeydown);
    };
  });
</script>

<div
  use:portal
  bind:this={menuEl}
  style="left: {adjustedX}px; top: {adjustedY}px;"
  class="fixed z-50 w-52 bg-brand-sidebar border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary backdrop-blur-xl animate-in fade-in zoom-in-95 duration-100 select-none"
  role="menu"
  tabindex="-1"
>
  {@render children()}
</div>
