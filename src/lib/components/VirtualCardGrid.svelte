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
   * Rows are assumed to share one height (cards of a kind are uniform); it's
   * measured from the rows on screen and re-measured as they resize.
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
  let width = $state(0);
  // A row's height, and the column width it was measured at: cards are
  // square-ish, so a different width makes the measurement stale.
  let measured = $state<{ columnWidth: number; height: number } | null>(null);
  // The viewport's span in the grid's own coordinates.
  let viewTop = $state(0);
  let viewHeight = $state(0);

  let columns = $derived.by(() => {
    if (width <= 0) return 1;
    const tracks = Math.max(1, Math.floor((width + gap) / (minColumnWidth + gap)));
    return fit ? Math.max(1, Math.min(tracks, items.length)) : tracks;
  });
  let columnWidth = $derived((width - gap * (columns - 1)) / columns);
  let rowHeight = $derived(
    measured && Math.abs(measured.columnWidth - columnWidth) < 0.5
      ? measured.height
      : estimateRowHeight(Math.max(columnWidth, minColumnWidth))
  );
  let rowStride = $derived(rowHeight + gap);
  let rowCount = $derived(Math.ceil(items.length / columns));
  let totalHeight = $derived(rowCount > 0 ? rowCount * rowStride - gap : 0);

  let firstRow = $derived(Math.max(0, Math.floor(viewTop / rowStride) - overscanRows));
  let lastRow = $derived(
    Math.min(rowCount, Math.ceil((viewTop + viewHeight) / rowStride) + overscanRows)
  );
  let visible = $derived(items.slice(firstRow * columns, lastRow * columns));

  function findScroller(node: HTMLElement): HTMLElement | null {
    for (let el = node.parentElement; el; el = el.parentElement) {
      if (/(auto|scroll)/.test(getComputedStyle(el).overflowY)) return el;
    }
    return null;
  }

  $effect(() => {
    const node = root;
    if (!node) return;
    const scroller = findScroller(node);
    const update = () => {
      width = node.clientWidth;
      if (!scroller) {
        viewTop = 0;
        viewHeight = window.innerHeight;
        return;
      }
      const top = node.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
      viewTop = -top;
      viewHeight = scroller.clientHeight;
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(node);
    if (scroller) ro.observe(scroller);
    const target = scroller ?? window;
    target.addEventListener("scroll", update, { passive: true });
    return () => {
      ro.disconnect();
      target.removeEventListener("scroll", update);
    };
  });

  // Measure a mounted card (grid rows stretch every card to the row's
  // height) whenever the mounted rows resize: a new column width, a font
  // load changing the text's height, or different rows scrolling in.
  $effect(() => {
    const node = rowsEl;
    if (!node) return;
    const measure = () => {
      const card = node.firstElementChild as HTMLElement | null;
      if (!card) return;
      const height = card.getBoundingClientRect().height;
      const at = untrack(() => columnWidth);
      if (height > 0 && (measured?.columnWidth !== at || Math.abs(measured.height - height) > 0.5)) {
        measured = { columnWidth: at, height };
      }
    };
    const ro = new ResizeObserver(measure);
    ro.observe(node);
    return () => ro.disconnect();
  });
</script>

<div bind:this={root} style="height: {totalHeight}px; position: relative;">
  <div
    bind:this={rowsEl}
    class="grid"
    style="position: absolute; left: 0; right: 0; top: {firstRow * rowStride}px; grid-template-columns: repeat({columns}, minmax(0, 1fr)); gap: {gap}px;"
  >
    {#each visible as entry (key(entry))}
      {@render item(entry)}
    {/each}
  </div>
</div>
