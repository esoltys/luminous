import { windowLayoutStore } from "./windowLayout.svelte";
import { PLAYER_DOCK_CONTENT_RESERVE_PX } from "../constants";

/**
 * Manages the auto-hide and reveal lifecycle of the floating PlayerBar dock in
 * immersive mode, including pointer hover/hold tracking and DOM/window event
 * wiring via `use:immersiveDock.attach`.
 */
class ImmersiveDockStore {
  static readonly HIDE_DELAY_MS = 2500;

  private hidden = $state<boolean>(false);
  private held = false;
  private hovered = false;
  private hideTimer: ReturnType<typeof setTimeout> | null = null;
  private releaseHoldListeners: (() => void) | null = null;

  constructor() {
    $effect.root(() => {
      $effect(() => {
        const isImmersive = windowLayoutStore.effectiveImmersiveMode;
        // Read immersiveView so switching between cover and visualizer reveals the dock.
        const _view = windowLayoutStore.immersiveView;
        if (isImmersive) {
          this.reveal();
        } else {
          this.reset();
        }
      });
    });
  }

  get isHidden(): boolean {
    return windowLayoutStore.effectiveImmersiveMode && this.hidden;
  }

  private clearTimer() {
    if (this.hideTimer) {
      clearTimeout(this.hideTimer);
      this.hideTimer = null;
    }
  }

  private scheduleHide() {
    this.clearTimer();
    if (!windowLayoutStore.effectiveImmersiveMode || this.held || this.hovered) return;
    this.hideTimer = setTimeout(() => {
      this.hideTimer = null;
      if (windowLayoutStore.effectiveImmersiveMode && !this.held && !this.hovered) {
        this.hidden = true;
      }
    }, ImmersiveDockStore.HIDE_DELAY_MS);
  }

  /**
   * Reveals the floating PlayerBar dock in immersive mode and starts the
   * auto-hide timer (unless held or hovered).
   */
  reveal() {
    this.hidden = false;
    if (windowLayoutStore.effectiveImmersiveMode) {
      this.scheduleHide();
    } else {
      this.clearTimer();
    }
  }

  private syncPinState(pinned: boolean) {
    if (pinned) {
      this.hidden = false;
      this.clearTimer();
    } else if (windowLayoutStore.effectiveImmersiveMode) {
      this.scheduleHide();
    }
  }

  /**
   * Tracks whether the pointer is resting inside the playerbar area. While
   * hovered, the dock stays visible indefinitely; leaving starts the hide countdown.
   */
  setHovered(hovered: boolean) {
    if (this.hovered === hovered && (!hovered || !this.hidden)) return;
    this.hovered = hovered;
    this.syncPinState(hovered);
  }

  /**
   * Holds the immersive dock visible while an interaction (pointer drag) is active;
   * releasing the hold restarts the auto-hide timer if the pointer is no longer hovering.
   */
  setHeld(held: boolean) {
    this.held = held;
    this.syncPinState(held);
  }

  /**
   * Clears any pending hide timer and resets dock state when leaving immersive mode.
   */
  reset() {
    this.clearTimer();
    this.releaseHoldListeners?.();
    this.hidden = false;
    this.hovered = false;
    this.held = false;
  }

  private startHold() {
    if (!windowLayoutStore.effectiveImmersiveMode) return;
    this.releaseHoldListeners?.();
    this.setHeld(true);
    const onPointerUp = () => this.endHold();
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
    this.releaseHoldListeners = () => {
      window.removeEventListener("pointerup", onPointerUp);
      window.removeEventListener("pointercancel", onPointerUp);
      this.releaseHoldListeners = null;
    };
  }

  private endHold() {
    this.releaseHoldListeners?.();
    if (this.held) {
      this.setHeld(false);
    }
  }

  /**
   * Svelte action that wires dock pointer/focus listeners and the window-level
   * bottom-zone `pointermove` detector onto the floating dock element.
   */
  attach = (node: HTMLElement) => {
    const onPointerEnter = () => this.setHovered(true);
    const onPointerMove = () => this.setHovered(true);
    const onPointerLeave = () => this.setHovered(false);
    const onPointerDown = () => this.startHold();
    const onFocusIn = () => this.reveal();
    // When hidden, `.immersive-dock-hidden` sets `pointer-events: none` on the dock,
    // so a window-level listener is required to detect entry into the bottom zone.
    const onWindowPointerMove = (e: PointerEvent) => {
      if (!windowLayoutStore.effectiveImmersiveMode) return;
      const inDockZone = e.clientY >= window.innerHeight - PLAYER_DOCK_CONTENT_RESERVE_PX;
      this.setHovered(inDockZone);
    };

    node.addEventListener("pointerenter", onPointerEnter);
    node.addEventListener("pointermove", onPointerMove);
    node.addEventListener("pointerleave", onPointerLeave);
    node.addEventListener("pointerdown", onPointerDown);
    node.addEventListener("focusin", onFocusIn);
    window.addEventListener("pointermove", onWindowPointerMove, { passive: true });

    return {
      destroy: () => {
        node.removeEventListener("pointerenter", onPointerEnter);
        node.removeEventListener("pointermove", onPointerMove);
        node.removeEventListener("pointerleave", onPointerLeave);
        node.removeEventListener("pointerdown", onPointerDown);
        node.removeEventListener("focusin", onFocusIn);
        window.removeEventListener("pointermove", onWindowPointerMove);
        this.endHold();
      },
    };
  };
}

export const immersiveDock = new ImmersiveDockStore();
