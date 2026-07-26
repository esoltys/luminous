<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    label: string;
    for: string;
    /** Spans both columns of the enclosing 2-column form grid. */
    span2?: boolean;
    children: Snippet;
  }

  let { label, for: htmlFor, span2 = false, children }: Props = $props();
</script>

<div class="flex flex-col gap-1 {span2 ? 'col-span-2' : ''}">
  <label for={htmlFor} class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{label}</label>
  {@render children()}
</div>
