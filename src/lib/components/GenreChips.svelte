<script lang="ts">
  import { MicrophoneStageIcon as Mic } from "phosphor-svelte";
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

  interface ChipItem {
    label: string;
    isCurated: boolean;
  }

  interface Props {
    /** Raw `; `-delimited `genre` column value (song, album, or otherwise). */
    genre?: string | null;
    /** Curated profile tags to display first before file genres */
    curatedTags?: string[];
    /** Custom click handler when a curated tag is clicked. If omitted, falls back to navigationStore.viewGenreTag. */
    onCuratedTagClick?: (tag: string) => void;
    /** Optional tooltip function for curated tags. */
    curatedTagTitle?: (tag: string) => string;
    /** "compact" (default) shows only the main (first) value plus a "+N"
        indicator, for narrow contexts like table columns. "full" shows every
        value as its own chip, for wide contexts like detail-view headers. */
    variant?: "compact" | "full";
    /** Optional maximum number of chips to display when variant is "full".
        When specified and genres exceed this limit, a "+N" overflow badge is shown. */
    limit?: number;
    class?: string;
  }

  let {
    genre,
    curatedTags,
    onCuratedTagClick,
    curatedTagTitle,
    variant = "compact",
    limit,
    class: className = "",
  }: Props = $props();

  let items = $derived.by<ChipItem[]>(() => {
    const list: ChipItem[] = [];
    const seen = new Set<string>();

    if (curatedTags) {
      for (const tag of curatedTags) {
        const trimmed = tag.trim();
        if (trimmed && !seen.has(trimmed.toLowerCase())) {
          seen.add(trimmed.toLowerCase());
          list.push({ label: trimmed, isCurated: true });
        }
      }
    }

    if (genre) {
      const parsed = parseMultiValue(genre);
      for (const g of parsed) {
        const trimmed = g.trim();
        if (trimmed && !seen.has(trimmed.toLowerCase())) {
          seen.add(trimmed.toLowerCase());
          list.push({ label: trimmed, isCurated: false });
        }
      }
    }

    return list;
  });

  let displayedItems = $derived(limit && limit > 0 ? items.slice(0, limit) : items);
  let remainingCount = $derived(items.length - displayedItems.length);
  let remainingItems = $derived(remainingCount > 0 ? items.slice(displayedItems.length) : []);

  $effect(() => {
    tagsStore.ensureHierarchyLoaded();
  });

  let isLightTheme = $derived(isLightColor(themeStore.resolvedColors["bg-main"]));
  let genreColorHslFg = $derived(isLightTheme ? genreColorHslDark : genreColorHslBright);

  function getChipStyle(value: string): string | undefined {
    const colorIndex = resolveGenreColorIndex(tagsStore.hierarchy, value);
    if (colorIndex === undefined) return undefined;
    return `background-color: color-mix(in srgb, ${genreColorHsl(colorIndex)} 38%, var(--color-brand-sidebar)); border-color: color-mix(in srgb, ${genreColorHslFg(colorIndex)} 70%, var(--color-brand-sidebar)); color: ${genreColorHslFg(colorIndex)};`;
  }

  function getChipClass(value: string): string {
    const colorIndex = resolveGenreColorIndex(tagsStore.hierarchy, value);
    const padding = variant === "compact" ? "px-2 py-0.5" : "px-2.5 py-1";
    const base = `inline-flex items-center rounded-full border-2 text-xs font-medium select-none transition-[opacity,box-shadow,transform,filter,colors] hover:brightness-110 active:scale-[0.98] ${padding}`;
    if (colorIndex !== undefined) {
      return base;
    }
    return `${base} bg-[color-mix(in_srgb,var(--color-brand-accent)_15%,var(--color-brand-sidebar))] text-brand-text-primary border-[color-mix(in_srgb,var(--color-brand-accent)_40%,var(--color-brand-sidebar))] hover:border-brand-accent/60`;
  }

  function getItemTitle(item: ChipItem): string {
    if (item.isCurated && curatedTagTitle) {
      return curatedTagTitle(item.label);
    }
    return i18n.t('songTags.goToGenreTooltip', { genre: item.label }, `Browse ${item.label}`);
  }

  function handleItemClick(e: MouseEvent, item: ChipItem) {
    e.stopPropagation();
    if (item.isCurated && onCuratedTagClick) {
      onCuratedTagClick(item.label);
    } else {
      navigationStore.viewGenreTag(item.label);
    }
  }
</script>

{#if items.length > 0}
  {#if variant === "compact"}
    <button
      type="button"
      onclick={(e) => handleItemClick(e, items[0])}
      title={getItemTitle(items[0])}
      class="{getChipClass(items[0].label)} gap-1 min-w-0 max-w-full {className}"
      style={getChipStyle(items[0].label)}
    >
      {#if items[0].isCurated}
        <Mic class="w-3 h-3 shrink-0 opacity-70" />
      {/if}
      <span class="truncate min-w-0">{items[0].label}</span>
      {#if items.length > 1}
        <span class="opacity-70 shrink-0 text-[0.85em]">+{items.length - 1}</span>
      {/if}
    </button>
  {:else}
    <div class="flex flex-wrap gap-1 {className}">
      {#each displayedItems as item (item.label)}
        <button
          type="button"
          onclick={(e) => handleItemClick(e, item)}
          title={getItemTitle(item)}
          class="{getChipClass(item.label)} gap-1 max-w-64 cursor-pointer"
          style={getChipStyle(item.label)}
        >
          {#if item.isCurated}
            <Mic class="w-3 h-3 shrink-0 opacity-70" />
          {/if}
          <span class="truncate">{item.label}</span>
        </button>
      {/each}
      {#if remainingCount > 0}
        <span
          class="inline-flex items-center px-2 py-1 rounded-full bg-brand-sidebar text-brand-text-secondary border border-brand-border text-xs font-medium select-none shrink-0"
          title={remainingItems.map(i => i.label).join(", ")}
        >
          +{remainingCount}
        </span>
      {/if}
    </div>
  {/if}
{/if}
