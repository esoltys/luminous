<script lang="ts">
  import { Tag as TagIcon, CheckSquare, LayoutGrid, Rows3 } from "lucide-svelte";
  import { genreColorHsl } from "../utils/genrePalette";
  import { onMount } from "svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { prefs, type GenreViewMode, type GenreSortField } from "../stores/prefs.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import EmptyState from "./EmptyState.svelte";
  import Select from "./Select.svelte";
  import GenreCards from "./GenreCards.svelte";
  import MergeSurvivorDialog from "./MergeSurvivorDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";

  let selectMode = $state(false);
  let selected = $state<Set<string>>(new Set());

  let mergeDialogNames = $state<string[] | null>(null);
  let deleteConfirmNames = $state<string[] | null>(null);

  function toggleSelectMode() {
    selectMode = !selectMode;
    selected = new Set();
  }

  function toggleSelect(name: string) {
    const next = new Set(selected);
    if (next.has(name)) {
      next.delete(name);
    } else {
      next.add(name);
    }
    selected = next;
  }

  function openMergeSelected() {
    if (selected.size < 2) return;
    mergeDialogNames = Array.from(selected);
  }

  function openDeleteSelected() {
    if (selected.size === 0) return;
    deleteConfirmNames = Array.from(selected);
  }

  async function confirmMerge(survivor: string) {
    const names = mergeDialogNames ?? [];
    mergeDialogNames = null;
    const others = names.filter((n) => n !== survivor);
    let total = 0;
    for (const other of others) {
      total += await tagsStore.mergeTags(other, survivor);
    }
    selected = new Set();
    toastStore.show(
      i18n.t("songTags.mergeToast", { count: total, name: survivor }, `Merged into "${survivor}" (${total} songs updated)`),
      "success"
    );
  }

  async function confirmDelete() {
    const names = deleteConfirmNames ?? [];
    deleteConfirmNames = null;
    const total = await tagsStore.deleteTags(names);
    selected = new Set();
    toastStore.show(
      i18n.t("songTags.deleteToast", { count: total }, `Deleted (${total} songs updated)`),
      "success"
    );
  }

  // This view mounts/unmounts with the tab (not a persistent singleton like
  // collectionStore), so the listener must be torn down on unmount — an
  // uncleaned Tauri listen() here would leak and re-fire once per every past
  // visit to this tab, compounding on itself.
  onMount(() => {
    let unlistenHierarchy: (() => void) | undefined;
    tagsStore.listenForHierarchyChanges().then((fn) => { unlistenHierarchy = fn; });
    tagsStore.loadHierarchy().catch((e) => console.error("Failed to load tag hierarchy:", e));
    return () => {
      unlistenHierarchy?.();
    };
  });

  // Every card/chip/tag click routes straight through to
  // AutoPlaylistDetailView (#548) — the Genres tab no longer has its own
  // song-list drill-down. A top-level card and a sub-genre chip resolve to
  // the exact same curated-tag auto-playlist lookup on the other end
  // (navigationStore.viewGenreTag); dragging a chip between cards only moves
  // curation metadata, so which card a chip is currently filed under never
  // changes which query opens it.

  /** Root-level card click, and the flat Tags view's plain tag click. */
  function openMainTag(tagName: string) {
    navigationStore.viewGenreTag(tagName);
  }

  /** Sub-genre chip click. The root/parent card name isn't needed on this
   * end — a curated tag's own auto-playlist is looked up by its name alone,
   * regardless of which card currently curates it. */
  function openGenreEdge(_rootTag: string, childTag: string) {
    navigationStore.viewGenreTag(childTag);
  }

  /** Songs with no genre value at all — the one case with no backing
   * curated tag, routed to AutoPlaylistDetailView's dedicated "no_genre"
   * kind (a direct query, no playlist row). */
  function openNoGenre() {
    navigationStore.viewAutoPlaylist({ kind: "no_genre" });
  }

  function setViewMode(mode: GenreViewMode) {
    prefs.setGenreViewMode(mode);
  }

  // Tag-cloud font sizing: log-scaled against the current min/max song
  // count so the range is actually perceptible regardless of whether counts
  // span 1-10 or 1-1000 (a plain linear/count-divided-by-N scale washes out
  // at either extreme).
  const TAG_CLOUD_MIN_REM = 0.75;
  const TAG_CLOUD_MAX_REM = 2;
  const TAG_CLOUD_MIN_WEIGHT = 500;
  const TAG_CLOUD_MAX_WEIGHT = 800;
  let tagCountRange = $derived.by(() => {
    const counts = tagsStore.allTags.map((t) => t.song_count);
    return { min: Math.min(...counts, 1), max: Math.max(...counts, 1) };
  });
  /** Padding is in `em` (not a fixed rem) so it scales together with the
   * text instead of leaving small pills looking oddly padded relative to
   * large ones. Each pill sizes to its own content — rows aren't forced to
   * a uniform height, so `items-center` (not `items-baseline`) is what
   * actually centers the label+count within its own pill. */
  function tagCloudStyle(count: number): string {
    const { min, max } = tagCountRange;
    const t = max <= min ? 0 : (Math.log(count + 1) - Math.log(min + 1)) / (Math.log(max + 1) - Math.log(min + 1));
    const fontSize = TAG_CLOUD_MIN_REM + t * (TAG_CLOUD_MAX_REM - TAG_CLOUD_MIN_REM);
    const fontWeight = Math.round(TAG_CLOUD_MIN_WEIGHT + t * (TAG_CLOUD_MAX_WEIGHT - TAG_CLOUD_MIN_WEIGHT));
    return `font-size: ${fontSize.toFixed(2)}rem; font-weight: ${fontWeight}; padding: 0.5em 0.9em;`;
  }
</script>

<div class="flex-1 px-6 pt-4 pb-6 overflow-y-auto">
    <div class="h-9 flex items-center justify-between mb-3">
      <div class="text-xs text-brand-text-secondary font-medium">
        {i18n.t("songTags.genresTabDescription", { count: tagsStore.hierarchy.length }, `Showing ${tagsStore.hierarchy.length} genres`)}
      </div>
      <div class="flex items-center gap-2">
        <button
          onclick={toggleSelectMode}
          class="flex items-center gap-1.5 px-3 h-7 rounded-full text-xs font-semibold border transition-colors {selectMode ? 'bg-brand-accent text-white border-brand-accent' : 'border-brand-border text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          <CheckSquare class="w-3.5 h-3.5" />
          {i18n.t("songTags.selectTags", {}, "Select Tags")}
        </button>
        {#if prefs.genreViewMode === "genre"}
          <div class="relative">
            <Select
              value={`${prefs.genreSortField}-${prefs.genreSortAsc}`}
              onchange={(e) => {
                const [field, asc] = e.currentTarget.value.split("-");
                prefs.setGenreSortField(field as GenreSortField);
                prefs.setGenreSortAsc(asc === "true");
              }}
              class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
            >
              <option value="name-true">▲ {i18n.t('songTags.sortName', {}, 'Name')}</option>
              <option value="name-false">▼ {i18n.t('songTags.sortName', {}, 'Name')}</option>
              <option value="count-true">▲ {i18n.t('songTags.sortSongCount', {}, 'Song Count')}</option>
              <option value="count-false">▼ {i18n.t('songTags.sortSongCount', {}, 'Song Count')}</option>
            </Select>
          </div>
          <div class="inline-flex items-center gap-0.5 bg-brand-sidebar border border-brand-border rounded-full p-1">
            <button
              onclick={() => prefs.setGenreCardsViewMode("cards")}
              class="flex items-center justify-center w-7 h-7 rounded-full transition-colors {prefs.genreCardsViewMode === 'cards' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              title={i18n.t("collection.viewCards", {}, "Card view")}
              aria-label={i18n.t("collection.viewCards", {}, "Card view")}
              aria-pressed={prefs.genreCardsViewMode === "cards"}
            >
              <LayoutGrid class="w-4 h-4" />
            </button>
            <button
              onclick={() => prefs.setGenreCardsViewMode("rows")}
              class="flex items-center justify-center w-7 h-7 rounded-full transition-colors {prefs.genreCardsViewMode === 'rows' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              title={i18n.t("collection.viewRows", {}, "Row view")}
              aria-label={i18n.t("collection.viewRows", {}, "Row view")}
              aria-pressed={prefs.genreCardsViewMode === "rows"}
            >
              <Rows3 class="w-4 h-4" />
            </button>
          </div>
        {/if}
        <div class="inline-flex items-center gap-0.5 bg-brand-sidebar border border-brand-border rounded-full p-1">
          <button
            onclick={() => setViewMode("genre")}
            class="px-3 h-7 rounded-full text-xs font-semibold transition-colors {prefs.genreViewMode === 'genre' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
            aria-pressed={prefs.genreViewMode === "genre"}
          >
            {i18n.t("songTags.viewGenre", {}, "Genre")}
          </button>
          <button
            onclick={() => setViewMode("tags")}
            class="px-3 h-7 rounded-full text-xs font-semibold transition-colors {prefs.genreViewMode === 'tags' ? 'bg-brand-accent text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
            aria-pressed={prefs.genreViewMode === "tags"}
          >
            {i18n.t("songTags.viewTags", {}, "Tags")}
          </button>
        </div>
      </div>
    </div>

    {#if selectMode}
      <div class="flex items-center justify-between mb-3 px-3 py-2 rounded-lg bg-brand-sidebar border border-brand-border/60">
        <span class="text-xs font-medium text-brand-text-secondary">
          {i18n.t("songTags.selectedCount", { count: selected.size }, `${selected.size} selected`)}
        </span>
        <div class="flex items-center gap-2">
          <button
            onclick={openMergeSelected}
            disabled={selected.size < 2}
            class="text-xs font-semibold text-brand-accent-text hover:text-brand-accent-text-hover transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {i18n.t("songTags.mergeSelected", {}, "Merge Selected")}
          </button>
          <button
            onclick={openDeleteSelected}
            disabled={selected.size === 0}
            class="text-xs font-semibold text-red-400 hover:text-red-300 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {i18n.t("songTags.deleteSelected", {}, "Delete Selected")}
          </button>
        </div>
      </div>
    {/if}

    {#if tagsStore.allTags.length === 0 && tagsStore.noGenreCount === 0}
      <div class="py-16">
        <EmptyState
          icon={TagIcon}
          title={i18n.t("songTags.emptyTitle", {}, "No tags yet")}
          subtitle={i18n.t(
            "songTags.emptySubtitle",
            {},
            "Right-click a song and choose Edit Tags to give it a genre — the first value is its main category, the rest are subgenres."
          )}
        />
      </div>
    {:else if prefs.genreViewMode === "genre"}
      <GenreCards
        {selectMode}
        {selected}
        onToggleSelect={toggleSelect}
        onOpenMainTag={openMainTag}
        onOpenGenreEdge={openGenreEdge}
        sortField={prefs.genreSortField}
        sortAsc={prefs.genreSortAsc}
        compact={prefs.genreCardsViewMode === "rows"}
      />
      {#if tagsStore.noGenreCount > 0}
        <button
          onclick={openNoGenre}
          class="mt-1.5 w-full flex items-center justify-between px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 hover:border-brand-accent/60 transition-colors text-left text-brand-text-secondary"
        >
          <span class="text-sm font-semibold">{i18n.t("songTags.noGenre", {}, "No Genre")}</span>
          <span class="text-xs tabular-nums">
            {i18n.t("songTags.songCount", { count: tagsStore.noGenreCount }, `${tagsStore.noGenreCount} songs`)}
          </span>
        </button>
      {/if}
    {:else}
      <div class="flex flex-wrap items-center gap-2">
        {#each tagsStore.allTags as tag (tag.name)}
          {@const group = tagsStore.hierarchy.find((g) => g.name === tag.name || g.children.some((c) => c.name === tag.name))}
          {@const colorIndex = group?.color_index}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span
            onclick={() => { if (selectMode) toggleSelect(tag.name); }}
            class="inline-flex items-center gap-1.5 rounded-full transition-colors {selectMode ? 'cursor-pointer' : ''} {selected.has(tag.name) ? 'ring-2 ring-brand-accent' : ''}"
            style={`${tagCloudStyle(tag.song_count)} ${
              colorIndex !== undefined
                ? `background-color: color-mix(in srgb, ${genreColorHsl(colorIndex)} 32%, var(--color-brand-sidebar)); color: color-mix(in srgb, ${genreColorHsl(colorIndex)} 85%, var(--color-brand-text-primary));`
                : "background-color: var(--color-brand-sidebar); color: var(--color-brand-text-primary);"
            }`}
          >
            {#if selectMode}
              <input type="checkbox" checked={selected.has(tag.name)} onchange={() => toggleSelect(tag.name)} class="self-center w-3 h-3 pointer-events-none" />
            {/if}
            <button onclick={() => !selectMode && openMainTag(tag.name)} class="self-center leading-none">
              {tag.name}
            </button>
            <span class="self-center text-[0.65em] font-bold opacity-70 leading-none">{tag.song_count}</span>
          </span>
        {/each}
      </div>
      {#if tagsStore.noGenreCount > 0}
        <button
          onclick={openNoGenre}
          class="mt-3 w-full flex items-center justify-between px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 hover:border-brand-accent/60 transition-colors text-left text-brand-text-secondary"
        >
          <span class="text-sm font-semibold">{i18n.t("songTags.noGenre", {}, "No Genre")}</span>
          <span class="text-xs tabular-nums">
            {i18n.t("songTags.songCount", { count: tagsStore.noGenreCount }, `${tagsStore.noGenreCount} songs`)}
          </span>
        </button>
      {/if}
    {/if}
</div>

{#if mergeDialogNames}
  <MergeSurvivorDialog
    names={mergeDialogNames}
    onConfirm={confirmMerge}
    onCancel={() => { mergeDialogNames = null; }}
  />
{/if}

{#if deleteConfirmNames}
  <ConfirmDialog
    title={i18n.t("songTags.deleteSelected", {}, "Delete Selected")}
    message={i18n.t(
      "songTags.deleteConfirmMessage",
      { count: deleteConfirmNames.length },
      `Remove ${deleteConfirmNames.length} tag(s) from every song that carries them? This can't be undone.`
    )}
    confirmLabel={i18n.t("songTags.deleteBtn", {}, "Delete")}
    cancelLabel={i18n.t("songTags.cancelBtn", {}, "Cancel")}
    onConfirm={confirmDelete}
    onCancel={() => { deleteConfirmNames = null; }}
  />
{/if}
