<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    /** Renders the header's content; receives the sort-direction arrow (▲/▼/"") to place. */
    label: Snippet<[arrow: string]>;
    /** Whether this column is the currently active sort column. */
    active: boolean;
    sortAsc: boolean;
    onclick: () => void;
    class?: string;
  }

  let { label, active, sortAsc, onclick, class: className = "" }: Props = $props();

  let arrow = $derived(active ? (sortAsc ? "▲" : "▼") : "");
</script>

<button {onclick} class={className}>{@render label(arrow)}</button>
