<script lang="ts">
  import type { Snippet } from "svelte";
  import { Info } from "lucide-svelte";

  interface Props {
    label: string;
    for: string;
    /** Spans both columns of the enclosing 2-column form grid. */
    span2?: boolean;
    /** Brief explanation shown via an info icon next to the label. */
    tooltip?: string;
    children: Snippet;
  }

  let { label, for: htmlFor, span2 = false, tooltip, children }: Props = $props();
</script>

<div class="flex flex-col gap-1 {span2 ? 'col-span-2' : ''}">
  <div class="flex items-center gap-1">
    <label for={htmlFor} class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{label}</label>
    {#if tooltip}
      <span title={tooltip} class="inline-flex cursor-help">
        <Info class="w-3 h-3 text-brand-text-secondary/50 hover:text-brand-text-secondary" />
      </span>
    {/if}
  </div>
  {@render children()}
</div>
