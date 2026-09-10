<script lang="ts">
  import { parseMultiValue } from "../utils/multiValue";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    /** Raw `; `-delimited `genre` column value (song, album, or otherwise). */
    genre?: string | null;
    /** "compact" (default) shows only the main (first) value plus a "+N"
        indicator, for narrow contexts like table columns. "full" shows every
        value as its own chip, for wide contexts like detail-view headers. */
    variant?: "compact" | "full";
    /** Optional maximum number of chips to display when variant is "full".
        When specified and genres exceed this limit, a "+N" overflow badge is shown. */
    limit?: number;
    class?: string;
  }

  let { genre, variant = "compact", limit, class: className = "" }: Props = $props();

  let values = $derived(parseMultiValue(genre || ""));
  let displayedValues = $derived(limit && limit > 0 ? values.slice(0, limit) : values);
  let remainingCount = $derived(values.length - displayedValues.length);
  let remainingValues = $derived(remainingCount > 0 ? values.slice(displayedValues.length) : []);

  const chipClass =
    "inline-flex items-center pl-2 pr-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border-2 border-brand-accent/25 text-xs font-medium hover:bg-brand-accent/25 hover:border-brand-accent/50 transition-colors";

  function goToTag(e: MouseEvent, value: string) {
    e.stopPropagation();
    navigationStore.viewGenreTag(value);
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
      {#each displayedValues as value (value)}
        <button
          type="button"
          onclick={(e) => goToTag(e, value)}
          title={i18n.t('songTags.goToGenreTooltip', { genre: value }, `Browse ${value}`)}
          class="{chipClass} max-w-64"
        >
          <span class="truncate">{value}</span>
        </button>
      {/each}
      {#if remainingCount > 0}
        <span
          class="inline-flex items-center px-2 py-0.5 rounded-full bg-brand-sidebar/80 text-brand-text-secondary border border-brand-border/60 text-xs font-medium select-none shrink-0"
          title={remainingValues.join(", ")}
        >
          +{remainingCount}
        </span>
      {/if}
    </div>
  {/if}
{/if}
