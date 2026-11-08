<script lang="ts">
  import type { Snippet } from "svelte";

  /** Gap between the cursor and the ghost's left edge, so the pill never sits under the pointer. */
  const CURSOR_OFFSET_PX = 14;

  interface Props {
    /** Pointer position in viewport coordinates (`clientX`/`clientY`). */
    x: number;
    y: number;
    /** True while the pointer is over a valid drop target; the pill lights up to confirm the drop. */
    overTarget?: boolean;
    /** Border colour override (e.g. the dragged genre's own hue); defaults to the brand accent. */
    color?: string;
    testId?: string;
    children: Snippet;
  }

  let { x, y, overTarget = false, color, testId, children }: Props = $props();
</script>

<!--
  Floating label that follows the pointer during a pointer-based drag. Native HTML5 drag images
  aren't available (Tauri's dragDropEnabled swallows dragstart), so in-app drags draw this instead.
  Dense single-axis lists (table rows, chips) can skip it: the dimmed item and drop marker suffice.
-->
<div
  class="fixed z-[9999] pointer-events-none select-none px-3 py-1.5 rounded-full border-2 bg-brand-surface/95 backdrop-blur-md text-brand-text-primary text-xs font-medium shadow-2xl flex items-center gap-2 -translate-y-1/2 transition-[border-color,background-color,box-shadow] duration-150 {overTarget ? 'ring-2 ring-brand-accent/40 bg-brand-accent/20' : ''} {color ? '' : overTarget ? 'border-brand-accent' : 'border-brand-accent/60'}"
  style="left: {x + CURSOR_OFFSET_PX}px; top: {y}px;{color ? ` border-color: ${color};` : ''}"
  data-testid={testId}
  data-over-target={overTarget ? "" : undefined}
>
  {@render children()}
</div>
