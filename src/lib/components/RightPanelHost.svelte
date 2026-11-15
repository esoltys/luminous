<script lang="ts">
  import { cubicOut } from "svelte/easing";
  import { InfoIcon as Info } from "phosphor-svelte";
  import RightPanel from "./RightPanel.svelte";
  import { fly, slide } from "../utils/motion";
  import { i18n } from "../stores/i18n.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import {
    RIGHT_PANEL_MIN_WIDTH_PX,
    RIGHT_PANEL_MAX_WIDTH_PX,
    PANEL_RESIZE_STEP_PX,
    PLAYER_DOCK_CLEARANCE_PX,
  } from "../constants";

  /**
   * Where the info sidebar lives. Pinned, it is docked beside the content. Unpinned, it
   * leaves the layout: a compact notch stays centered at the right edge (between the top bar
   * and the player dock when `dockVisible`), and the sidebar is an overlay that opens when
   * the pointer rests on the notch and closes shortly after the pointer leaves it, or on
   * Escape or a click outside.
   *
   * The host paints the panel's surface in both places, so RightPanel itself is only content.
   *
   * Must sit inside a `relative` container that spans the area the overlay should cover.
   * `instant` snaps the slide, for keyboard-driven changes.
   */
  let { instant = false, dockVisible = false }: { instant?: boolean; dockVisible?: boolean } =
    $props();

  const PANEL_SLIDE_MS = 250;
  const OVERLAY_SLIDE_MS = 200;
  const OPEN_DELAY_MS = 120;
  const CLOSE_DELAY_MS = 250;

  // Continuous S-curve silhouette for the 34x88 right-edge notch: 17px circular-arc
  // concave flares into the window edge, inflecting seamlessly into 17px convex corners.
  const NOTCH_EDGE_PATH =
    "M 34 0 C 34 9.4 26.4 17 17 17 C 7.6 17 0 24.6 0 34 L 0 54 C 0 63.4 7.6 71 17 71 C 26.4 71 34 78.6 34 88";
  const NOTCH_SILHOUETTE_PATH = `${NOTCH_EDGE_PATH} Z`;

  let overlayEl = $state<HTMLElement | null>(null);
  let notchEl = $state<HTMLElement | null>(null);
  // Set when a close leaves the pointer resting on the notch, which would otherwise reopen the
  // overlay straight away; cleared once the pointer moves off the notch.
  let notchSuppressed = false;
  let resizing = false;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const layout = windowLayoutStore;
  let docked = $derived(layout.rightPanelPinned && !layout.isRightPanelAutoHidden);
  let showNotch = $derived(layout.hasRightNotch);
  let showOverlay = $derived(showNotch && layout.rightPanelPeeking);

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

  function onNotchEnter() {
    if (layout.rightPanelPeeking || notchSuppressed) return;
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
    if (!showNotch) return;
    const onPointerMove = (e: PointerEvent) => {
      if (notchSuppressed && !notchEl?.contains(e.target as Node)) notchSuppressed = false;
    };
    window.addEventListener("pointermove", onPointerMove);
    return () => window.removeEventListener("pointermove", onPointerMove);
  });

  $effect(() => {
    if (!showOverlay) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        notchSuppressed = true;
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
      layout.setRightPanelWidth(
        Math.max(RIGHT_PANEL_MIN_WIDTH_PX, Math.min(RIGHT_PANEL_MAX_WIDTH_PX, startWidth - deltaX)),
      );
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
      layout.setRightPanelWidth(
        Math.min(RIGHT_PANEL_MAX_WIDTH_PX, layout.rightPanelWidth + PANEL_RESIZE_STEP_PX),
      );
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      layout.setRightPanelWidth(
        Math.max(RIGHT_PANEL_MIN_WIDTH_PX, layout.rightPanelWidth - PANEL_RESIZE_STEP_PX),
      );
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
    aria-label={i18n.t("topNav.resizeRightPanel")}
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
  <div
    transition:slide={{ axis: "x", duration: instant ? 0 : PANEL_SLIDE_MS }}
    class="docked-surface h-full flex-shrink-0 flex overflow-hidden bg-brand-sidebar {themeStore.isGlassTheme
      ? 'glass-surface'
      : ''}"
  >
    {@render resizeHandle()}
    <RightPanel
      isOpen={true}
      width={layout.rightPanelWidth}
      pinned={true}
      onTogglePin={togglePin}
    />
  </div>
{/if}

{#if showNotch}
  <!-- A compact floating notch at the right edge, vertically centered between the top bar and
       the player dock (when visible), so the content keeps full horizontal width and its
       scrollbar stays clear of the trigger. -->
  <div
    bind:this={notchEl}
    role="group"
    aria-label={i18n.t("topNav.rightPanelRail", {}, "Info sidebar")}
    style:top={dockVisible ? `calc(50% - ${PLAYER_DOCK_CLEARANCE_PX / 2}px)` : "50%"}
    style:clip-path={`path('${NOTCH_SILHOUETTE_PATH}')`}
    class="notch-surface absolute right-0 -translate-y-1/2 z-30 h-[88px] w-[34px] flex items-center justify-center bg-brand-sidebar {themeStore.isGlassTheme
      ? 'glass-surface'
      : ''}"
    onpointerenter={onNotchEnter}
    onpointerleave={cancelTimer}
  >
    <!-- Stroke along the exact clip-path boundary: width 2 leaves a 1px inner border after clipping. -->
    <svg
      viewBox="0 0 34 88"
      class="pointer-events-none absolute inset-0 w-full h-full"
      aria-hidden="true"
    >
      <path d={NOTCH_EDGE_PATH} fill="none" stroke="var(--color-border)" stroke-width="2" />
    </svg>
    <button
      type="button"
      onclick={togglePin}
      aria-label={i18n.t("topNav.pinRightPanel", {}, "Pin info sidebar")}
      title={i18n.t("topNav.pinRightPanel", {}, "Pin info sidebar")}
      class="relative p-1 rounded-md text-brand-text-secondary hover:text-brand-text-primary transition-colors"
    >
      <!-- Same glyph and size as the player bar's info button. -->
      <Info class="w-5 h-5" />
    </button>
  </div>
{/if}

{#if showOverlay}
  <!-- Acrylic on the overlay itself, not on anything inside it: fly fades this element's
       opacity, which would cut a descendant's backdrop-filter off from the page for the
       length of the slide. -->
  <div
    bind:this={overlayEl}
    transition:fly={{
      x: layout.rightPanelWidth,
      duration: instant ? 0 : OVERLAY_SLIDE_MS,
      easing: cubicOut,
    }}
    role="presentation"
    class="absolute right-0 top-0 bottom-0 z-40 flex shadow-2xl acrylic-float"
    onpointerenter={cancelTimer}
    onpointerleave={onOverlayLeave}
  >
    {@render resizeHandle()}
    <RightPanel
      isOpen={true}
      width={layout.rightPanelWidth}
      pinned={false}
      onTogglePin={togglePin}
    />
  </div>
{/if}

<style>
  /* Keep the notch out of flex flow even when .glass-surface (which sets
     `position: relative` in app.css) is applied, and match the pinned
     sidebar's solid-glass background so bg-main doesn't bleed through. */
  .notch-surface {
    position: absolute;
  }

  .notch-surface.glass-surface {
    -webkit-backdrop-filter: none !important;
    backdrop-filter: none !important;
    background-color: var(--glass-solid-sidebar) !important;
  }

  /* Docked, nothing renders behind the panel but the flat bg-main canvas, so
     it paints the glass result as a solid color instead of running a
     backdrop-filter (see flatGlassColor() in theme.svelte.ts). Only the inset
     highlight of --glass-shadow: as before, when the panel inside this
     overflow-hidden wrapper carried it, the elevation stays off the content. */
  .docked-surface.glass-surface {
    -webkit-backdrop-filter: none !important;
    backdrop-filter: none !important;
    background-color: var(--glass-solid-sidebar) !important;
    border-color: var(--glass-border-color, var(--color-border)) !important;
    box-shadow: var(--glass-highlight, none);
  }
</style>
