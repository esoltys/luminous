/**
 * Shared gesture for every drag source that uses pointer events instead of HTML5 drag-and-drop
 * (native drag events are swallowed by Tauri's `dragDropEnabled` window option, which OS file-drop
 * import needs). A component calls {@link startPointerDrag} from its `onpointerdown` after its own
 * "may this press start a drag" checks, and supplies only policy: what the payload is, where the
 * drop lands, how reordering works. Everything about the gesture itself lives here:
 *
 * - the primary-button check and the interactive-descendant guard,
 * - pointer capture, so WebView2 can't hijack the gesture into a native OS drag,
 * - the single click-versus-drag threshold,
 * - the window listeners and their cleanup,
 * - swallowing the click the browser sends after a drag is released.
 */

/** Pointer travel (CSS px) below which a press-and-release is a click, not a drag. */
export const POINTER_DRAG_THRESHOLD_PX = 4;

/** Descendants that handle their own pointer input; a press on one never starts a drag. */
export const INTERACTIVE_SELECTOR = "button, a, input, select, textarea, [data-interactive]";

export interface Point {
  x: number;
  y: number;
}

export interface PointerDragOptions {
  /**
   * When to take pointer capture. Capture retargets the click that follows the gesture to the
   * capturing element, so `"press"` (the default) only suits an element whose own click handler
   * lives on it. An element whose click handlers sit on nested children must use `"arm"`, which
   * captures only once the threshold is crossed, so a plain click still reaches those children.
   */
  captureOn?: "press" | "arm";
  /** Presses inside a match for this selector are ignored. Defaults to {@link INTERACTIVE_SELECTOR}. */
  ignoreSelector?: string;
  /** The pointer has travelled past the threshold; the drag is now live. */
  onArm?(pos: Point, e: PointerEvent): void;
  /** Every pointer move while the drag is live, including the one that armed it. */
  onMove?(pos: Point, e: PointerEvent): void;
  /** Released after the drag was armed. */
  onDrop?(pos: Point, e: PointerEvent): void | Promise<void>;
  /** Released without ever arming: a plain click. */
  onTap?(e: PointerEvent): void;
  /** `pointercancel`, or `cancel()` on the handle. `wasArmed` says whether a live drag was aborted. */
  onCancel?(wasArmed: boolean): void;
}

export interface PointerDragHandle {
  /** Aborts the gesture (e.g. on unmount). A no-op once the gesture has ended. */
  cancel(): void;
}

// The click that follows a released drag must not also select, open or reorder something. The
// guard is scoped to the next press: a release that produces no click can't leave it armed to
// swallow an unrelated click later. At most one guard exists at a time.
let retractClickGuard: (() => void) | null = null;

function swallowNextClick() {
  retractClickGuard?.();
  const onClick = (ev: MouseEvent) => {
    ev.preventDefault();
    ev.stopPropagation();
    retract();
  };
  const retract = () => {
    window.removeEventListener("click", onClick, true);
    window.removeEventListener("pointerdown", retract, true);
    window.removeEventListener("keydown", retract, true);
    retractClickGuard = null;
  };
  window.addEventListener("click", onClick, true);
  window.addEventListener("pointerdown", retract, true);
  window.addEventListener("keydown", retract, true);
  retractClickGuard = retract;
}

/**
 * Starts tracking a pointer gesture from a `pointerdown` event. Returns `null` when the press is
 * ignored (non-primary button, or inside an interactive descendant); otherwise a handle whose
 * `cancel()` the owner calls on teardown.
 */
export function startPointerDrag(e: PointerEvent, opts: PointerDragOptions): PointerDragHandle | null {
  if (e.button !== 0) return null;
  const selector = opts.ignoreSelector ?? INTERACTIVE_SELECTOR;
  if ((e.target as Element | null)?.closest?.(selector)) return null;

  retractClickGuard?.();

  const el = e.currentTarget as HTMLElement | null;
  const pointerId = e.pointerId;
  const startX = e.clientX;
  const startY = e.clientY;
  let armed = false;
  let ended = false;

  // Capture can throw (stale pointer id) or be missing (jsdom); the gesture still works without it.
  const capture = () => {
    try {
      el?.setPointerCapture?.(pointerId);
    } catch {
      // ignored
    }
  };
  if (opts.captureOn !== "arm") capture();

  const stop = () => {
    ended = true;
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onCancel);
    try {
      el?.releasePointerCapture?.(pointerId);
    } catch {
      // ignored
    }
  };

  function onMove(ev: PointerEvent) {
    const pos = { x: ev.clientX, y: ev.clientY };
    if (!armed) {
      if (Math.hypot(pos.x - startX, pos.y - startY) < POINTER_DRAG_THRESHOLD_PX) return;
      armed = true;
      if (opts.captureOn === "arm") capture();
      opts.onArm?.(pos, ev);
    }
    opts.onMove?.(pos, ev);
  }

  function onUp(ev: PointerEvent) {
    stop();
    if (!armed) {
      opts.onTap?.(ev);
      return;
    }
    swallowNextClick();
    return opts.onDrop?.({ x: ev.clientX, y: ev.clientY }, ev);
  }

  function onCancel() {
    if (ended) return;
    const wasArmed = armed;
    stop();
    opts.onCancel?.(wasArmed);
  }

  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onCancel);

  return { cancel: onCancel };
}
