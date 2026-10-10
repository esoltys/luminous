<script lang="ts">
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { playerStore } from "../stores/player.svelte";
  import { portal } from "../utils/portal";
  import { CONTEXT_MENU_WIDTH_PX, VIEWPORT_EDGE_PADDING_PX, PLAYER_DOCK_CLEARANCE_PX } from "../constants";

  interface Props {
    x: number;
    y: number;
    onClose: () => void;
    /** First-paint height hint; replaced by the measured height once rendered. */
    estimatedHeight?: number;
    children: Snippet;
  }

  let { x, y, onClose, estimatedHeight = 200, children }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  // Real rendered height (0 until first layout), so the clamp never depends on a guess.
  let measuredHeight = $state(0);

  // Keep menu inside viewport boundaries
  let adjustedX = $derived.by(() => {
    if (typeof window === "undefined") return x;
    return Math.min(x, window.innerWidth - CONTEXT_MENU_WIDTH_PX - VIEWPORT_EDGE_PADDING_PX);
  });

  let adjustedY = $derived.by(() => {
    if (typeof window === "undefined") return y;
    const menuHeight = measuredHeight || estimatedHeight;
    // Clamp above the floating PlayerBar dock so the menu's lower items
    // aren't hidden underneath it.
    const dockClearance = playerStore.currentSong ? PLAYER_DOCK_CLEARANCE_PX : 0;
    const maxY = window.innerHeight - menuHeight - dockClearance - VIEWPORT_EDGE_PADDING_PX;
    // Never push the top edge off-screen when the menu is taller than the window.
    return Math.max(VIEWPORT_EDGE_PADDING_PX, Math.min(y, maxY));
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
  bind:offsetHeight={measuredHeight}
  style="left: {adjustedX}px; top: {adjustedY}px;"
  class="fixed z-50 w-52 bg-brand-sidebar border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary backdrop-blur-xl select-none"
  role="menu"
  tabindex="-1"
>
  {@render children()}
</div>
