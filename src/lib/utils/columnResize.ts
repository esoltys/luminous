import type { VisibleColumns } from "../stores/collection.svelte";

/** Minimum column width in pixels — users can intentionally squeeze columns very small. */
const MIN_WIDTH_PX = 40;

/** Singleton style injection so the CSS is added only once per page load. */
let styleInjected = false;
function ensureStyles() {
  if (styleInjected || typeof document === "undefined") return;
  styleInjected = true;
  const style = document.createElement("style");
  style.textContent = `
    .col-resize-handle {
      position: absolute;
      top: 0;
      right: -3px;
      width: 6px;
      height: 100%;
      cursor: col-resize;
      z-index: 20;
      border-right: 2px solid transparent;
      transition: border-color 120ms ease;
    }
    .col-resize-handle:hover,
    .col-resize-handle.col-resize-dragging {
      border-right-color: var(--color-brand-accent, #6366f1);
    }
  `;
  document.head.appendChild(style);
}

export interface ColumnResizeOptions {
  /** The column key — passed back to `onResize` and `onReset`. */
  column: keyof VisibleColumns;
  /**
   * Called when the user finishes dragging with the new pixel width.
   * Note: if the column was previously `fr`-based, the caller must read the
   * *rendered* width at drag start (passed as the initial width) and switch
   * to a `px` value from that point on.
   */
  onResize: (column: keyof VisibleColumns, widthPx: number) => void;
  /** Called when the user double-clicks the handle to reset the column to its default width. */
  onReset?: (column: keyof VisibleColumns) => void;
}

/**
 * Svelte action that attaches a drag handle to the right edge of a header cell.
 *
 * Usage:
 * ```svelte
 * <div use:columnResize={{ column: "title", onResize: handleResize, onReset: handleReset }}
 *      style="position: relative;">
 *   Title
 * </div>
 * ```
 *
 * The header cell must have `position: relative` (or any non-static positioning)
 * for the absolutely-positioned handle overlay to work correctly.
 */
export function columnResize(
  node: HTMLElement,
  opts: ColumnResizeOptions
): { update: (newOpts: ColumnResizeOptions) => void; destroy: () => void } {
  ensureStyles();

  let currentOpts = opts;

  // Inject the drag handle div
  const handle = document.createElement("div");
  handle.className = "col-resize-handle";
  handle.setAttribute("aria-hidden", "true");
  node.appendChild(handle);

  // --- Drag state ---
  let dragging = false;
  let startX = 0;
  let startWidth = 0;

  function onPointerDown(e: PointerEvent) {
    // Only respond to primary button
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();

    dragging = true;
    startX = e.clientX;
    // Measure the current rendered width of the header cell (works for both px and fr columns)
    startWidth = node.getBoundingClientRect().width;

    handle.classList.add("col-resize-dragging");
    handle.setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    const delta = e.clientX - startX;
    const newWidth = Math.max(MIN_WIDTH_PX, startWidth + delta);
    // Live-update the node width so the user sees feedback immediately;
    // the actual grid column value is updated only on pointerup to avoid
    // triggering excessive store writes and re-renders.
    node.style.width = `${newWidth}px`;
  }

  function onPointerUp(e: PointerEvent) {
    if (!dragging) return;
    dragging = false;
    handle.classList.remove("col-resize-dragging");

    const delta = e.clientX - startX;
    const finalWidth = Math.max(MIN_WIDTH_PX, startWidth + delta);
    // Clear the inline width — the grid will now take over with the stored px value
    node.style.width = "";

    currentOpts.onResize(currentOpts.column, Math.round(finalWidth));
  }

  function onDoubleClick(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    currentOpts.onReset?.(currentOpts.column);
  }

  handle.addEventListener("pointerdown", onPointerDown);
  handle.addEventListener("pointermove", onPointerMove);
  handle.addEventListener("pointerup", onPointerUp);
  handle.addEventListener("dblclick", onDoubleClick);

  return {
    update(newOpts: ColumnResizeOptions) {
      currentOpts = newOpts;
    },
    destroy() {
      handle.removeEventListener("pointerdown", onPointerDown);
      handle.removeEventListener("pointermove", onPointerMove);
      handle.removeEventListener("pointerup", onPointerUp);
      handle.removeEventListener("dblclick", onDoubleClick);
      handle.remove();
    },
  };
}
