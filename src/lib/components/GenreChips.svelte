<script lang="ts">
  import { parseMultiValue } from "../utils/multiValue";
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    /** Raw `; `-delimited `genre` column value (song, album, or otherwise). */
    genre?: string | null;
    /** "compact" (default) shows only the main (first) value plus a "+N"
        indicator, for narrow contexts like table columns. "full" shows every
        value as its own chip, for wide contexts like detail-view headers. */
    variant?: "compact" | "full";
    class?: string;
  }

  let { genre, variant = "compact", class: className = "" }: Props = $props();

  let values = $derived(parseMultiValue(genre || ""));

  const chipClass =
    "inline-flex items-center pl-2 pr-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25 text-xs font-medium hover:bg-brand-accent/25 hover:border-brand-accent/50 transition-colors";

  function goToTag(e: MouseEvent, value: string) {
    e.stopPropagation();
    collectionStore.viewGenreTag(value);
  }
</script>

{#if values.length > 0}
  {#if variant === "compact"}
    <button
      type="button"
      onclick={(e) => goToTag(e, values[0])}
      title={i18n.t('songTags.goToGenreTooltip', { genre: values[0] }, `Browse ${values[0]}`)}
      class="{chipClass} gap-1 min-w-0 max-w-full {className}"
    >
      <span class="truncate min-w-0">{values[0]}</span>
      {#if values.length > 1}
        <span class="text-brand-text-secondary shrink-0">+{values.length - 1}</span>
      {/if}
    </button>
  {:else}
    <div class="flex flex-wrap gap-1 {className}">
      {#each values as value (value)}
        <button
          type="button"
          onclick={(e) => goToTag(e, value)}
          title={i18n.t('songTags.goToGenreTooltip', { genre: value }, `Browse ${value}`)}
          class="{chipClass} max-w-64"
        >
          <span class="truncate">{value}</span>
        </button>
      {/each}
    </div>
  {/if}
{/if}
