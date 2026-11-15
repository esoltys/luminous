<script lang="ts">
  import { onDestroy } from "svelte";
  import type { Component, Snippet } from "svelte";
  import { CaretRightIcon as CaretRight } from "phosphor-svelte";
  import { viewportBottomLimitPx } from "../utils/viewport";
  import { portal } from "../utils/portal";
  import {
    CONTEXT_SUBMENU_WIDTH_PX,
    VIEWPORT_EDGE_PADDING_PX,
    CONTEXT_MENU_LAYER_ATTR,
  } from "../constants";

  interface Props {
    icon: Component<{ class?: string }>;
    label: string;
    /** Flyout width in px. */
    width?: number;
    /** Flyout content; receives `close` for item handlers that dismiss the flyout. */
    children: Snippet<[() => void]>;
  }

  let { icon: Icon, label, width = CONTEXT_SUBMENU_WIDTH_PX, children }: Props = $props();

  // A flyout cannot live inside its parent menu: the menu scrolls (overflow
  // clips absolute children) and is `.acrylic-float` (transform +
  // backdrop-filter make it the containing block of fixed children). So the
  // flyout is portaled to <body> and placed from the trigger's rect, clamped
  // to the viewport above the player dock; it scrolls itself when too tall.
  let isOpen = $state(false);
  let closeTimer: ReturnType<typeof setTimeout> | null = null;
  let triggerEl = $state<HTMLDivElement | null>(null);
  let triggerRect = $state<DOMRect | null>(null);
  let flyoutHeight = $state(0);

  const bottomLimit = $derived(typeof window === "undefined" ? 0 : viewportBottomLimitPx());
  const maxHeight = $derived(Math.max(0, bottomLimit - VIEWPORT_EDGE_PADDING_PX));
  const left = $derived.by(() => {
    if (!triggerRect || typeof window === "undefined") return 0;
    const opensLeft = triggerRect.right + width + VIEWPORT_EDGE_PADDING_PX > window.innerWidth;
    // Overlap the trigger by 2px so the pointer never crosses a gap that
    // would fire mouseleave before it reaches the flyout.
    const x = opensLeft ? triggerRect.left + 2 - width : triggerRect.right - 2;
    return Math.max(VIEWPORT_EDGE_PADDING_PX, x);
  });
  const top = $derived(
    triggerRect
      ? Math.max(VIEWPORT_EDGE_PADDING_PX, Math.min(triggerRect.top, bottomLimit - flyoutHeight))
      : 0,
  );

  function cancelClose() {
    if (closeTimer) clearTimeout(closeTimer);
    closeTimer = null;
  }

  function open() {
    cancelClose();
    if (!isOpen && triggerEl) triggerRect = triggerEl.getBoundingClientRect();
    isOpen = true;
  }

  function scheduleClose() {
    cancelClose();
    closeTimer = setTimeout(() => (isOpen = false), 150);
  }

  function close() {
    cancelClose();
    isOpen = false;
  }

  // Scrolling the parent menu moves the trigger out from under the flyout.
  function handleScroll(e: Event) {
    if (isOpen && !(e.target as Element | null)?.closest?.(`[${CONTEXT_MENU_LAYER_ATTR}]`)) {
      close();
    }
  }

  onDestroy(cancelClose);
</script>

<svelte:window onscrollcapture={handleScroll} />

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div bind:this={triggerEl} onmouseenter={open} onmouseleave={scheduleClose}>
  <button
    type="button"
    class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 transition-colors hover:bg-brand-main hover:text-brand-text-primary {isOpen
      ? 'bg-brand-main text-brand-text-primary'
      : 'text-brand-text-primary'}"
    role="menuitem"
    aria-haspopup="menu"
    aria-expanded={isOpen}
    onclick={() => (isOpen ? close() : open())}
    onkeydown={(e) => {
      if (e.key === "ArrowRight" || e.key === "Enter") {
        e.preventDefault();
        open();
      }
    }}
  >
    <Icon class="w-3.5 h-3.5 shrink-0 text-brand-text-secondary" />
    <span class="flex-1">{label}</span>
    <CaretRight
      class="w-3 h-3 shrink-0 text-brand-text-secondary/70 {isOpen
        ? 'text-brand-text-primary'
        : ''}"
    />
  </button>
</div>

{#if isOpen}
  <!-- Opaque, not acrylic: a backdrop-filter here could only sample the menu, not the page. -->
  <div
    use:portal
    {...{ [CONTEXT_MENU_LAYER_ATTR]: "" }}
    bind:offsetHeight={flyoutHeight}
    style="left: {left}px; top: {top}px; width: {width}px; max-height: {maxHeight}px;"
    class="fixed z-50 overflow-y-auto bg-brand-sidebar border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary select-none"
    role="menu"
    tabindex="-1"
    onmouseenter={open}
    onmouseleave={scheduleClose}
    onkeydown={(e) => {
      if (e.key === "ArrowLeft" || e.key === "Escape") {
        e.preventDefault();
        close();
      }
    }}
  >
    {@render children(close)}
  </div>
{/if}
