import { playerStore } from "../stores/player.svelte";
import { VIEWPORT_EDGE_PADDING_PX } from "../constants";

/**
 * Lowest y (px from the viewport top) a floating menu or flyout may extend to:
 * above the PlayerBar dock when it is showing, with the standard edge padding.
 * Reactive to the player's dock state; the window height is read at call time.
 */
export function viewportBottomLimitPx(): number {
  return window.innerHeight - playerStore.dockClearancePx - VIEWPORT_EDGE_PADDING_PX;
}
