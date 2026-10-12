<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { InfoIcon as Info } from "phosphor-svelte";
  import RightPanel from "./RightPanel.svelte";
  import { fly, slide } from "../utils/motion";
  import { i18n } from "../stores/i18n.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import {
    RIGHT_PANEL_MIN_WIDTH_PX,
    RIGHT_PANEL_MAX_WIDTH_PX,
    PANEL_RESIZE_STEP_PX,
  } from "../constants";

  /**
   * Where the info sidebar lives. Pinned, it is docked beside the content. Unpinned, it
   * leaves the layout: a slim rail stays at the right edge, and the sidebar is an
   * overlay that opens when the pointer rests on the rail and closes shortly after the
   * pointer leaves it, or on Escape or a click outside.
   *
   * Must sit inside a `relative` container that spans the area the overlay should cover.
   * `instant` snaps the slide, for keyboard-driven changes.
   */
  let { instant = false }: { instant?: boolean } = $props();

  const PANEL_SLIDE_MS = 250;
  const OVERLAY_SLIDE_MS = 200;
  const OPEN_DELAY_MS = 120;
  const CLOSE_DELAY_MS = 250;

  let overlayEl = $state<HTMLElement | null>(null);
  let railEl = $state<HTMLElement | null>(null);
  // Set when a close leaves the pointer resting on the rail, which would otherwise reopen the overlay
  // straight away; cleared once the pointer moves off the rail.
  let railSuppressed = false;
  let resizing = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const layout = windowLayoutStore;
  let docked = $derived(layout.rightPanelPinned && !layout.isRightPanelAutoHidden);
  let showRail = $derived(!layout.rightPanelPinned && !layout.isRightPanelAutoHidden);
  let showOverlay = $derived(showRail && layout.rightPanelPeeking);

  function later(fn: () => void, ms: number) {
    cancelTimer();
    timer = setTimeout(fn, ms);
  }

  function cancelTimer() {
    if (timer) clearTimeout(timer);
    timer = null;
  }

  function closeOverlay() {
    cancelTimer();
    layout.setRightPanelPeeking(false);
  }

  function onRailEnter() {
    if (layout.rightPanelPeeking || railSuppressed) return;
    later(() => layout.setRightPanelPeeking(true), OPEN_DELAY_MS);
  }

  function onOverlayLeave() {
    if (!resizing) later(closeOverlay, CLOSE_DELAY_MS);
  }

  // Unpinning leaves the sidebar up as an overlay: the pointer that clicked the pin icon is still
  // over the panel, and moving it away dismisses it.
  function togglePin() {
    layout.setRightPanelPinned(!layout.rightPanelPinned);
  }

  // However the overlay closed, no timer may outlive it.
  $effect(() => {
    if (!layout.rightPanelPeeking) cancelTimer();
  });

  $effect(() => {
    if (!showRail) return;
    const onPointerMove = (e: PointerEvent) => {
      if (railSuppressed && !railEl?.contains(e.target as Node)) railSuppressed = false;
    };
    window.addEventListener("pointermove", onPointerMove);
    return () => window.removeEventListener("pointermove", onPointerMove);
  });

  $effect(() => {
    if (!showOverlay) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        railSuppressed = true;
        closeOverlay();
      }
    };
    const onPointerDown = (e: PointerEvent) => {
      if (overlayEl && !overlayEl.contains(e.target as Node)) closeOverlay();
    };
    const onBlur = () => closeOverlay();
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("blur", onBlur);
    };
  });

  // Pointer drag resizing (right-to-left increase): both modes anchor the panel to the right edge.
  function startResize(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = layout.rightPanelWidth;
    resizing = true;

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaX = moveEvent.clientX - startX;
      layout.setRightPanelWidth(Math.max(RIGHT_PANEL_MIN_WIDTH_PX, Math.min(RIGHT_PANEL_MAX_WIDTH_PX, startWidth - deltaX)));
    }

    function onPointerUp() {
      resizing = false;
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
      // The pointer may have been released outside the overlay, which fires no leave event.
      if (overlayEl && !overlayEl.matches(":hover")) onOverlayLeave();
    }

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  function handleResizeKeyDown(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      e.stopPropagation();
      layout.setRightPanelWidth(Math.min(RIGHT_PANEL_MAX_WIDTH_PX, layout.rightPanelWidth + PANEL_RESIZE_STEP_PX));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      layout.setRightPanelWidth(Math.max(RIGHT_PANEL_MIN_WIDTH_PX, layout.rightPanelWidth - PANEL_RESIZE_STEP_PX));
    }
  }
</script>

{#snippet resizeHandle()}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    role="separator"
    aria-valuenow={layout.rightPanelWidth}
    aria-valuemin={RIGHT_PANEL_MIN_WIDTH_PX}
    aria-valuemax={RIGHT_PANEL_MAX_WIDTH_PX}
    aria-label={i18n.t('topNav.resizeRightPanel')}
    tabindex="0"
    class="relative w-1 bg-brand-border hover:bg-brand-accent/50 active:bg-brand-accent cursor-col-resize transition-colors self-stretch flex-shrink-0 z-30 touch-none focus:outline-none focus:bg-brand-accent"
    onpointerdown={startResize}
    onkeydown={handleResizeKeyDown}
  >
    <!-- Expanded hover/touch area wrapper -->
    <div class="absolute left-0 -right-2 top-0 bottom-0 cursor-col-resize"></div>
  </div>
{/snippet}

{#if docked}
  <div transition:slide={{ axis: 'x', duration: instant ? 0 : PANEL_SLIDE_MS }} class="h-full flex-shrink-0 flex overflow-hidden">
    {@render resizeHandle()}
    <RightPanel isOpen={true} width={layout.rightPanelWidth} pinned={true} onTogglePin={togglePin} />
  </div>
{/if}

{#if showRail}
  <!-- A visible rail rather than an invisible hot edge: it keeps the content's own scrollbar
       clear of the trigger, and gives the pointer something to find. -->
  <div
    bind:this={railEl}
    role="group"
    aria-label={i18n.t('topNav.rightPanelRail', {}, 'Info sidebar')}
    class="h-full w-9 flex-shrink-0 flex flex-col items-center pt-2 border-l border-brand-border bg-brand-sidebar"
    onpointerenter={onRailEnter}
    onpointerleave={cancelTimer}
  >
    <button
      type="button"
      onclick={togglePin}
      aria-label={i18n.t('topNav.pinRightPanel', {}, 'Pin info sidebar')}
      title={i18n.t('topNav.pinRightPanel', {}, 'Pin info sidebar')}
      class="p-1 rounded-md text-brand-text-secondary hover:text-brand-text-primary transition-colors"
    >
      <!-- Same glyph and size as the player bar's info button. -->
      <Info class="w-5 h-5" />
    </button>
  </div>
{/if}

{#if showOverlay}
  <div
    bind:this={overlayEl}
    transition:fly={{ x: layout.rightPanelWidth, duration: instant ? 0 : OVERLAY_SLIDE_MS, easing: cubicOut }}
    role="presentation"
    class="absolute right-0 top-0 bottom-0 z-40 flex shadow-2xl"
    onpointerenter={cancelTimer}
    onpointerleave={onOverlayLeave}
  >
    {@render resizeHandle()}
    <RightPanel isOpen={true} width={layout.rightPanelWidth} pinned={false} onTogglePin={togglePin} />
  </div>
{/if}
