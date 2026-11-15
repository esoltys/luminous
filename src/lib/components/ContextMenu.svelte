<script lang="ts">
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import { viewportBottomLimitPx } from "../utils/viewport";
  import { portal } from "../utils/portal";
  import {
    CONTEXT_MENU_WIDTH_PX,
    CONTEXT_MENU_LAYER_ATTR,
    VIEWPORT_EDGE_PADDING_PX,
  } from "../constants";

  interface Props {
    x: number;
    y: number;
    onClose: () => void;
    children: Snippet;
  }

  let { x, y, onClose, children }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  // Real rendered height measured on mount, so callers never supply a height.
  let measuredHeight = $state(0);

  // Keep menu inside viewport boundaries
  let adjustedX = $derived.by(() => {
    if (typeof window === "undefined") return x;
    return Math.min(x, window.innerWidth - CONTEXT_MENU_WIDTH_PX - VIEWPORT_EDGE_PADDING_PX);
  });

  // Tallest the menu may be: a menu taller than this scrolls instead of
  // running off the bottom of a short window. Flyouts can't be children of
  // this scroll container; see ContextMenuSubmenu.
  let maxHeight = $derived(
    typeof window === "undefined"
      ? undefined
      : Math.max(0, viewportBottomLimitPx() - VIEWPORT_EDGE_PADDING_PX),
  );

  let adjustedY = $derived.by(() => {
    if (typeof window === "undefined") return y;
    const maxY = viewportBottomLimitPx() - measuredHeight;
    // Never push the top edge off-screen when the menu is taller than the window.
    return Math.max(VIEWPORT_EDGE_PADDING_PX, Math.min(y, maxY));
  });

  function handleWindowClick(e: MouseEvent) {
    const target = e.target as Element;
    if (menuEl && !menuEl.contains(target) && !target.closest?.(`[${CONTEXT_MENU_LAYER_ATTR}]`)) {
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
  style="left: {adjustedX}px; top: {adjustedY}px;{maxHeight === undefined
    ? ''
    : ` max-height: ${maxHeight}px;`}"
  class="fixed z-50 w-52 overflow-y-auto acrylic-float border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary select-none"
  role="menu"
  tabindex="-1"
>
  {@render children()}
</div>
