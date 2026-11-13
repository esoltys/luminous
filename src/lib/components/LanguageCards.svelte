<script lang="ts">
  import { TranslateIcon as Translate } from "phosphor-svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { getLanguageDisplayName } from "../utils/language";
  import type { LanguageSummary } from "../types";

  interface Props {
    languages?: LanguageSummary[];
    onOpenLanguage: (code: string) => void;
    sortField?: "name" | "count";
    sortAsc?: boolean;
    compact?: boolean;
  }

  let {
    languages,
    onOpenLanguage,
    sortField = "name",
    sortAsc = true,
    compact = false,
  }: Props = $props();

  let effectiveLanguages = $derived(languages ?? tagsStore.languages);

  let sortedLanguages = $derived.by(() => {
    return [...effectiveLanguages].sort((a, b) => {
      if (sortField === "name") {
        const nameA = getLanguageDisplayName(a.code);
        const nameB = getLanguageDisplayName(b.code);
        return sortAsc ? nameA.localeCompare(nameB) : nameB.localeCompare(nameA);
      }
      return sortAsc ? a.song_count - b.song_count : b.song_count - a.song_count;
    });
  });
</script>

<div
  class="grid {compact
    ? 'grid-cols-[repeat(auto-fill,minmax(280px,1fr))] gap-2'
    : 'grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-4'}"
>
  {#each sortedLanguages as item (item.code)}
    {@const displayName = getLanguageDisplayName(item.code)}
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      onclick={() => onOpenLanguage(item.code)}
      class="group rounded-lg bg-brand-sidebar border border-brand-border/60 hover:border-brand-accent/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 cursor-pointer overflow-hidden select-none"
    >
      {#if compact}
        <div class="flex items-center gap-3 px-3 py-2.5">
          <div
            class="shrink-0 w-9 h-9 rounded-md flex items-center justify-center bg-gradient-to-br from-teal-600/30 to-emerald-600/30 text-brand-text-primary border border-teal-500/20"
          >
            <Translate class="w-4 h-4" />
          </div>
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-1.5">
              <span
                class="text-sm font-semibold text-brand-text-primary truncate"
                title={displayName}
              >
                {displayName}
              </span>
              <span
                class="text-[10px] code-caps font-mono font-bold px-1.5 py-0.5 rounded bg-brand-main/70 text-brand-text-secondary border border-brand-border/40 shrink-0"
              >
                {item.code}
              </span>
            </div>
          </div>
          <div class="shrink-0 text-right">
            <span class="text-xs text-brand-text-secondary tabular-nums">
              {i18n.plural("playlists.songsCount", item.song_count)}
            </span>
          </div>
        </div>
      {:else}
        <div
          class="p-4 flex flex-col justify-between h-28 bg-gradient-to-br from-teal-600/10 via-transparent to-transparent"
        >
          <div class="flex items-start justify-between gap-2">
            <div
              class="w-10 h-10 rounded-lg flex items-center justify-center bg-gradient-to-br from-teal-600/30 to-emerald-600/30 text-brand-text-primary border border-teal-500/20 shadow-sm"
            >
              <Translate class="w-5 h-5" />
            </div>
            <span
              class="text-[11px] code-caps font-mono font-bold px-2 py-0.5 rounded-full bg-brand-main/80 text-brand-text-secondary border border-brand-border/60"
            >
              {item.code}
            </span>
          </div>
          <div>
            <h3
              class="text-base font-semibold text-brand-text-primary truncate"
              title={displayName}
            >
              {displayName}
            </h3>
            <div
              class="flex items-center gap-2 text-xs text-brand-text-secondary font-medium tabular-nums mt-0.5"
            >
              <span>{i18n.plural("playlists.songsCount", item.song_count)}</span>
              <span>•</span>
              <span>{i18n.plural("collection.albumsCount", item.album_count)}</span>
            </div>
          </div>
        </div>
      {/if}
    </div>
  {/each}
</div>
