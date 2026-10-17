<script lang="ts" generics="T">
  import type { Snippet } from "svelte";
  import { untrack } from "svelte";

  /**
   * A responsive card grid that only mounts the rows near the viewport (#1528).
   * Lays out like `grid-cols-[repeat(auto-fill,minmax(<min>px,1fr))]` (or
   * `auto-fit` with `fit`), but rows scrolled out of view are unmounted — so
   * their cover `<img>`s go with them — and stand-in padding keeps the
   * scrollbar honest. Scrolls with its nearest scrollable ancestor, so it can
   * sit under a sticky header in an ordinary scrolling view.
   *
   * Cards may differ in height: every row is stretched to the tallest card
   * seen at the current column width, so rows keep one height and the
   * layout holds still while scrolling.
   */
  interface Props {
    items: T[];
    key: (item: T) => string;
    item: Snippet<[T]>;
    /** Narrowest a column may get, in px — the `minmax()` minimum. */
    minColumnWidth: number;
    /** Gap between rows and columns, in px. */
    gap: number;
    /** `auto-fit`: with fewer items than columns, the items stretch to fill the row. */
    fit?: boolean;
    /** Row height (gap excluded) used until a row has been measured. */
    estimateRowHeight: (columnWidth: number) => number;
    /** Rows mounted beyond each edge of the viewport. */
    overscanRows?: number;
  }

  let {
    items,
    key,
    item,
    minColumnWidth,
    gap,
    fit = false,
    estimateRowHeight,
    overscanRows = 2,
  }: Props = $props();

  let root = $state<HTMLDivElement>();
  let rowsEl = $state<HTMLDivElement>();
  // Null until first measured: nothing mounts before then, or the first
  // cards would lay out in one full-width column and size their covers for it.
  let width = $state<number | null>(null);
  // The tallest row seen, and the column width it was measured at: cards are
  // square-ish, so a different width makes the measurement stale. It only
  // grows until then. Taking it from whichever row happened to mount first
  // (an album row with no genre chip is shorter) made the stride flip
  // between row heights as the view scrolled, jumping the content by
  // `firstRow` times the difference (#1753).
  let measured = $state<{ columnWidth: number; height: number } | null>(null);
  // The scrolling ancestor, found by the viewport effect; `measure` scrolls
  // it to compensate a growing stride. Not reactive: nothing renders from it.
  let scroller: HTMLElement | null = null;
  // The viewport's span in the grid's own coordinates.
  let viewTop = $state(0);
  let viewHeight = $state(0);

  let columns = $derived.by(() => {
    if (!width) return 1;
    const tracks = Math.max(1, Math.floor((width + gap) / (minColumnWidth + gap)));
    return fit ? Math.max(1, Math.min(tracks, items.length)) : tracks;
  });
  let columnWidth = $derived(((width ?? 0) - gap * (columns - 1)) / columns);
  // The measured height, while it was measured at the current column width.
  let freshHeight = $derived(
    measured && Math.abs(measured.columnWidth - columnWidth) < 0.5 ? measured.height : null
  );
  let rowHeight = $derived(freshHeight ?? estimateRowHeight(Math.max(columnWidth, minColumnWidth)));
  // Shorter rows stretch to the stride, so the mounted rows sit exactly where
  // `firstRow * rowStride` says. `auto` lets a taller card grow its row
  // rather than overflow into the next one; it then raises the measurement.
  // An estimate isn't imposed, so the first measurement is the cards' own.
  let rowTrack = $derived(freshHeight === null ? "auto" : `minmax(${freshHeight}px, auto)`);
  let rowStride = $derived(rowHeight + gap);
  let rowCount = $derived(Math.ceil(items.length / columns));
  let totalHeight = $derived(rowCount > 0 ? rowCount * rowStride - gap : 0);

  let firstRow = $derived(Math.max(0, Math.floor(viewTop / rowStride) - overscanRows));
  let lastRow = $derived(
    Math.min(rowCount, Math.ceil((viewTop + viewHeight) / rowStride) + overscanRows)
  );
  let visible = $derived(width === null ? [] : items.slice(firstRow * columns, lastRow * columns));

  function findScroller(node: HTMLElement): HTMLElement | null {
    for (let el = node.parentElement; el; el = el.parentElement) {
      if (/(auto|scroll)/.test(getComputedStyle(el).overflowY)) return el;
    }
    return null;
  }

  $effect(() => {
    const node = root;
    if (!node) return;
    const found = findScroller(node);
    scroller = found;
    const update = () => {
      width = node.clientWidth;
      if (!found) {
        viewTop = 0;
        viewHeight = window.innerHeight;
        return;
      }
      const top = node.getBoundingClientRect().top - found.getBoundingClientRect().top;
      viewTop = -top;
      viewHeight = found.clientHeight;
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(node);
    if (found) ro.observe(found);
    const target = found ?? window;
    target.addEventListener("scroll", update, { passive: true });
    return () => {
      ro.disconnect();
      target.removeEventListener("scroll", update);
    };
  });

  // Measure the tallest mounted row (grid rows stretch every card to the
  // row's height) whenever the mounted rows resize: a new column width, a
  // font load changing the text's height, or a taller row scrolling in.
  $effect(() => {
    const node = rowsEl;
    if (!node) return;
    const tallest = () => {
      let height = 0;
      for (const card of node.children) height = Math.max(height, card.getBoundingClientRect().height);
      return height;
    };
    const measure = () => {
      const at = untrack(() => columnWidth);
      const prev = untrack(() => measured);
      if (!prev || prev.columnWidth !== at) {
        // The rows may still carry the old width's floor, which would hold
        // a narrower card at the old height; read the cards' own heights.
        const track = node.style.gridAutoRows;
        node.style.gridAutoRows = "auto";
        const height = tallest();
        node.style.gridAutoRows = track;
        if (height > 0) measured = { columnWidth: at, height };
        return;
      }
      const height = tallest();
      if (height > prev.height + 0.5) {
        // Every row above the view grows too: scroll by as much, so the
        // rows on screen stay put instead of jumping by `firstRow` times
        // the growth.
        const top = untrack(() => viewTop);
        const shift = top > 0 ? (top * (height - prev.height)) / (prev.height + gap) : 0;
        measured = { columnWidth: at, height };
        if (shift && scroller) {
          scroller.scrollTop += shift;
          viewTop = top + shift;
        }
      }
    };
    const ro = new ResizeObserver(measure);
    ro.observe(node);
    return () => ro.disconnect();
  });
</script>

<!-- overflow-anchor: none, so the browser's own scroll anchoring doesn't
     correct a stride change on top of `measure`'s. -->
<div bind:this={root} style="height: {totalHeight}px; position: relative; overflow-anchor: none;">
  <div
    bind:this={rowsEl}
    class="grid"
    style="position: absolute; left: 0; right: 0; top: {firstRow * rowStride}px; grid-template-columns: repeat({columns}, minmax(0, 1fr)); grid-auto-rows: {rowTrack}; gap: {gap}px;"
  >
    {#each visible as entry (key(entry))}
      {@render item(entry)}
    {/each}
  </div>
</div>
