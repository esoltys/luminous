<script lang="ts">
  import { parseMultiValue } from "../utils/multiValue";
  import { navigationStore } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { isLightColor } from "../utils/colorUtils";
  import {
    genreColorHsl,
    genreColorHslBright,
    genreColorHslDark,
    resolveGenreColorIndex,
  } from "../utils/genrePalette";

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

  $effect(() => {
    tagsStore.ensureHierarchyLoaded();
  });

  let isLightTheme = $derived(isLightColor(themeStore.resolvedColors["bg-main"]));
  let genreColorHslFg = $derived(isLightTheme ? genreColorHslDark : genreColorHslBright);

  function getChipStyle(value: string): string | undefined {
    const colorIndex = resolveGenreColorIndex(tagsStore.hierarchy, value);
    if (colorIndex === undefined) return undefined;
    return `background-color: color-mix(in srgb, ${genreColorHsl(colorIndex)} 38%, transparent); border-color: color-mix(in srgb, ${genreColorHslFg(colorIndex)} 70%, transparent); color: ${genreColorHslFg(colorIndex)};`;
  }

  function getChipClass(value: string): string {
    const colorIndex = resolveGenreColorIndex(tagsStore.hierarchy, value);
    const padding = variant === "compact" ? "px-2 py-0.5" : "px-2.5 py-1";
    const base = `inline-flex items-center rounded-full border-2 text-xs font-medium select-none transition-[opacity,box-shadow,transform,filter,colors] hover:brightness-110 active:scale-[0.98] ${padding}`;
    if (colorIndex !== undefined) {
      return base;
    }
    return `${base} bg-brand-accent/15 text-brand-text-primary border-brand-accent/25 hover:bg-brand-accent/25 hover:border-brand-accent/50`;
  }

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
      class="{getChipClass(values[0])} gap-1 min-w-0 max-w-full {className}"
      style={getChipStyle(values[0])}
    >
      <span class="truncate min-w-0">{values[0]}</span>
      {#if values.length > 1}
        <span class="opacity-70 shrink-0 text-[0.85em]">+{values.length - 1}</span>
      {/if}
    </button>
  {:else}
    <div class="flex flex-wrap gap-1 {className}">
      {#each displayedValues as value (value)}
        <button
          type="button"
          onclick={(e) => goToTag(e, value)}
          title={i18n.t('songTags.goToGenreTooltip', { genre: value }, `Browse ${value}`)}
          class="{getChipClass(value)} max-w-64"
          style={getChipStyle(value)}
        >
          <span class="truncate">{value}</span>
        </button>
      {/each}
      {#if remainingCount > 0}
        <span
          class="inline-flex items-center px-2 py-1 rounded-full bg-brand-sidebar/80 text-brand-text-secondary border border-brand-border/60 text-xs font-medium select-none shrink-0"
          title={remainingValues.join(", ")}
        >
          +{remainingCount}
        </span>
      {/if}
    </div>
  {/if}
{/if}
