<script lang="ts">
  import {
    TagIcon,
    CheckSquareIcon as CheckSquare,
    SquaresFourIcon as LayoutGrid,
    RowsIcon as Rows3,
    MicrophoneStageIcon as Mic,
    PlusIcon as Plus
  } from "phosphor-svelte";
  import { genreColorHsl } from "../utils/genrePalette";
  import { onMount } from "svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { prefs, type GenreViewMode, type GenreSortField } from "../stores/prefs.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import EmptyState from "./EmptyState.svelte";
  import Select from "./Select.svelte";
  import GenreCards from "./GenreCards.svelte";
  import ArtistTagCards from "./ArtistTagCards.svelte";
  import MergeSurvivorDialog from "./MergeSurvivorDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import CreateArtistTagGroupDialog from "./CreateArtistTagGroupDialog.svelte";

  let selectMode = $state(false);
  let selected = $state<Set<string>>(new Set());

  let mergeDialogNames = $state<string[] | null>(null);
  let deleteConfirmNames = $state<string[] | null>(null);
  let createGroupDialogTags = $state<string[] | null>(null);
  let showNewGroupDialog = $state(false);

  let genreNames = $derived(new Set(tagsStore.allTags.map((t) => t.name.toLowerCase())));

  let artistOnlyTags = $derived.by(() => {
    return tagsStore.artistTags.filter((t) => !genreNames.has(t.name.toLowerCase()));
  });

  let artistOnlyHierarchy = $derived.by(() => {
    return tagsStore.artistHierarchy
      .filter((g) => !genreNames.has(g.name.toLowerCase()))
      .map((g) => ({
        ...g,
        children: g.children.filter((c) => !genreNames.has(c.name.toLowerCase())),
      }));
  });

  let totalArtistTagCount = $derived.by(() => {
    const names = new Set<string>();
    for (const g of artistOnlyHierarchy) {
      names.add(g.name.toLowerCase());
      for (const c of g.children) {
        names.add(c.name.toLowerCase());
      }
    }
    for (const t of artistOnlyTags) {
      names.add(t.name.toLowerCase());
    }
    return names.size;
  });

  let genreViewElements = $state<Record<string, HTMLButtonElement>>({});
  let genreIndicatorStyle = $state({ left: 4, width: 0, opacity: 0 });
  let genreViewMounted = $state(false);

  function updateGenreIndicator() {
    const el = genreViewElements[prefs.genreViewMode];
    if (el) {
      genreIndicatorStyle = {
        left: el.offsetLeft,
        width: el.offsetWidth,
        opacity: 1
      };
    }
  }

  onMount(() => {
    const handleResize = () => updateGenreIndicator();
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
    };
  });

  $effect(() => {
    if (genreViewElements[prefs.genreViewMode]) {
      updateGenreIndicator();
      if (!genreViewMounted) {
        requestAnimationFrame(() => { genreViewMounted = true; });
      }
    }
  });

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

  function openGroupSelected() {
    if (selected.size === 0) return;
    createGroupDialogTags = Array.from(selected);
  }

  async function handleCreateGroup(groupName: string, tagsToReparent: string[]) {
    createGroupDialogTags = null;
    showNewGroupDialog = false;
    await tagsStore.createArtistTagGroup(groupName);
    for (const tag of tagsToReparent) {
      await tagsStore.reparentArtistTag(tag, groupName);
    }
    selected = new Set();
    if (tagsToReparent.length > 0) {
      toastStore.show(
        i18n.t("songTags.groupToast", { count: tagsToReparent.length, name: groupName }, `Grouped ${tagsToReparent.length} tags under "${groupName}"`),
        "success"
      );
    }
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
    let songTotal = 0;
    let artistTotal = 0;

    const isArtistTag = (name: string) =>
      tagsStore.artistTags.some((t) => t.name.toLowerCase() === name.toLowerCase()) ||
      tagsStore.artistHierarchy.some((g) => g.name.toLowerCase() === name.toLowerCase() || g.children.some((c) => c.name.toLowerCase() === name.toLowerCase()));
    const isSongTag = (name: string) =>
      tagsStore.allTags.some((t) => t.name.toLowerCase() === name.toLowerCase());

    for (const other of others) {
      if (isSongTag(other) || isSongTag(survivor)) {
        songTotal += await tagsStore.mergeTags(other, survivor);
      }
      if (isArtistTag(other) || isArtistTag(survivor)) {
        artistTotal += await tagsStore.mergeArtistTags(other, survivor);
      }
    }
    selected = new Set();
    if (artistTotal > 0 && songTotal === 0) {
      toastStore.show(
        i18n.t("songTags.artistMergeToast", { count: artistTotal, name: survivor }, `Merged into "${survivor}" (${artistTotal} artists updated)`),
        "success"
      );
    } else {
      toastStore.show(
        i18n.t("songTags.mergeToast", { count: songTotal, name: survivor }, `Merged into "${survivor}" (${songTotal} songs updated)`),
        "success"
      );
    }
  }

  async function confirmDelete() {
    const names = deleteConfirmNames ?? [];
    deleteConfirmNames = null;

    const isArtistTag = (name: string) =>
      tagsStore.artistTags.some((t) => t.name.toLowerCase() === name.toLowerCase()) ||
      tagsStore.artistHierarchy.some((g) => g.name.toLowerCase() === name.toLowerCase() || g.children.some((c) => c.name.toLowerCase() === name.toLowerCase()));
    const isSongTag = (name: string) =>
      tagsStore.allTags.some((t) => t.name.toLowerCase() === name.toLowerCase());

    const songNames = names.filter(isSongTag);
    const artistNames = names.filter(isArtistTag);

    let songTotal = 0;
    let artistTotal = 0;
    if (songNames.length > 0) {
      songTotal = await tagsStore.deleteTags(songNames);
    }
    if (artistNames.length > 0) {
      artistTotal = await tagsStore.deleteArtistTags(artistNames);
    }
    selected = new Set();
    if (artistTotal > 0 && songTotal === 0) {
      toastStore.show(
        i18n.t("songTags.artistDeleteToast", { count: artistTotal }, `Deleted (${artistTotal} artists updated)`),
        "success"
      );
    } else {
      toastStore.show(
        i18n.t("songTags.deleteToast", { count: songTotal }, `Deleted (${songTotal} songs updated)`),
        "success"
      );
    }
  }

  // This view mounts/unmounts with the tab (not a persistent singleton like
  // collectionStore), so the listener must be torn down on unmount — an
  // uncleaned Tauri listen() here would leak and re-fire once per every past
  // visit to this tab, compounding on itself.
  onMount(() => {
    let unlistenHierarchy: (() => void) | undefined;
    let unlistenArtistHierarchy: (() => void) | undefined;
    tagsStore.listenForHierarchyChanges().then((fn) => { unlistenHierarchy = fn; });
    tagsStore.listenForArtistHierarchyChanges().then((fn) => { unlistenArtistHierarchy = fn; });
    tagsStore.loadHierarchy().catch((e) => console.error("Failed to load tag hierarchy:", e));
    tagsStore.loadArtistTags().catch((e) => console.error("Failed to load artist tags:", e));
    tagsStore.loadArtistHierarchy().catch((e) => console.error("Failed to load artist tag hierarchy:", e));
    return () => {
      unlistenHierarchy?.();
      unlistenArtistHierarchy?.();
    };
  });

  /** Artist tag card/chip click — the browsable-only counterpart to
   * `openMainTag` for curated artist tags (#962/#956 follow-up). Shown
   * ahead of genre cards/chips in this view, per its own section below. */
  function openArtistTag(tagName: string) {
    navigationStore.viewArtistTag(tagName);
  }

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

<div class="flex-1 px-6 pt-4 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
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
          <div class="relative inline-flex items-center gap-0.5 bg-brand-sidebar border border-brand-border rounded-full p-1">
            <!-- Sliding background indicator -->
            <span
              class="absolute top-1 bottom-1 left-1 w-7 h-7 rounded-full bg-brand-accent shadow-sm pointer-events-none transition-transform duration-200 ease-out {prefs.genreCardsViewMode === 'rows' ? 'translate-x-[30px]' : 'translate-x-0'}"
              aria-hidden="true"
            ></span>
            <button
              onclick={() => prefs.setGenreCardsViewMode("cards")}
              class="relative z-10 flex items-center justify-center w-7 h-7 rounded-full transition-colors duration-200 {prefs.genreCardsViewMode === 'cards' ? 'text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              title={i18n.t("collection.viewCards", {}, "Card view")}
              aria-label={i18n.t("collection.viewCards", {}, "Card view")}
              aria-pressed={prefs.genreCardsViewMode === "cards"}
            >
              <LayoutGrid class="w-4 h-4" />
            </button>
            <button
              onclick={() => prefs.setGenreCardsViewMode("rows")}
              class="relative z-10 flex items-center justify-center w-7 h-7 rounded-full transition-colors duration-200 {prefs.genreCardsViewMode === 'rows' ? 'text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
              title={i18n.t("collection.viewRows", {}, "Row view")}
              aria-label={i18n.t("collection.viewRows", {}, "Row view")}
              aria-pressed={prefs.genreCardsViewMode === "rows"}
            >
              <Rows3 class="w-4 h-4" />
            </button>
          </div>
        {/if}
        <div class="relative inline-flex items-center gap-0.5 bg-brand-sidebar border border-brand-border rounded-full p-1">
          <!-- Sliding background indicator -->
          <span
            class="absolute top-1 bottom-1 bg-brand-accent rounded-full shadow-sm pointer-events-none {genreViewMounted ? 'transition-[left,width] duration-200 ease-out' : 'transition-none'}"
            style="left: {genreIndicatorStyle.left}px; width: {genreIndicatorStyle.width}px; opacity: {genreIndicatorStyle.opacity};"
            aria-hidden="true"
          ></span>
          <button
            bind:this={genreViewElements["genre"]}
            onclick={() => setViewMode("genre")}
            class="relative z-10 px-3 h-7 rounded-full text-xs font-semibold transition-colors duration-200 {prefs.genreViewMode === 'genre' ? 'text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
            aria-pressed={prefs.genreViewMode === "genre"}
          >
            {i18n.t("songTags.viewGenre", {}, "Genre")}
          </button>
          <button
            bind:this={genreViewElements["tags"]}
            onclick={() => setViewMode("tags")}
            class="relative z-10 px-3 h-7 rounded-full text-xs font-semibold transition-colors duration-200 {prefs.genreViewMode === 'tags' ? 'text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
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
            onclick={openGroupSelected}
            disabled={selected.size === 0}
            class="text-xs font-semibold text-brand-accent-text hover:text-brand-accent-text-hover transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {i18n.t("songTags.groupSelected", {}, "Group Selected")}
          </button>
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

    {#if totalArtistTagCount > 0}
      <div class="mb-6">
        <div class="flex items-center justify-between mb-2.5">
          <div class="text-xs text-brand-text-secondary font-medium">
            {i18n.t("songTags.artistTagsSectionTitle", { count: totalArtistTagCount }, `Artist Only Tags (${totalArtistTagCount})`)}
          </div>
          {#if prefs.genreViewMode === "genre"}
            <button
              type="button"
              onclick={() => { showNewGroupDialog = true; }}
              class="inline-flex items-center gap-1 text-xs font-semibold text-brand-accent-text hover:text-brand-accent-text-hover transition-colors"
            >
              <Plus class="w-3.5 h-3.5" />
              {i18n.t("songTags.newArtistGroup", {}, "New Group")}
            </button>
          {/if}
        </div>
        {#if prefs.genreViewMode === "genre"}
          <ArtistTagCards
            hierarchy={artistOnlyHierarchy}
            {selectMode}
            {selected}
            onToggleSelect={toggleSelect}
            onOpenTag={openArtistTag}
            sortField={prefs.genreSortField}
            sortAsc={prefs.genreSortAsc}
            compact={prefs.genreCardsViewMode === "rows"}
          />
        {:else}
          <div class="flex flex-wrap gap-1.5">
            {#each artistOnlyTags as tag (tag.name)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <span
                onclick={() => { if (selectMode) toggleSelect(tag.name); }}
                class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full border-2 border-brand-border bg-brand-sidebar text-brand-text-primary text-xs font-medium select-none transition-colors hover:border-brand-accent/60 {selectMode ? 'cursor-pointer' : ''} {selected.has(tag.name) ? 'ring-2 ring-brand-accent' : ''}"
              >
                {#if selectMode}
                  <input type="checkbox" checked={selected.has(tag.name)} onchange={() => toggleSelect(tag.name)} class="self-center w-3 h-3 pointer-events-none" />
                {/if}
                <button
                  type="button"
                  onclick={() => !selectMode && openArtistTag(tag.name)}
                  class="inline-flex items-center gap-1.5 cursor-pointer leading-none"
                  title={i18n.t("songTags.goToArtistTagTooltip", { tag: tag.name }, `Browse ${tag.name}`)}
                >
                  <Mic class="w-3 h-3 shrink-0 opacity-70" />
                  <span>{tag.name}</span>
                </button>
                <span class="opacity-70 text-[0.85em] font-bold leading-none">{tag.song_count}</span>
              </span>
            {/each}
          </div>
        {/if}
      </div>
    {/if}

    {#if tagsStore.allTags.length === 0 && tagsStore.noGenreCount === 0 && tagsStore.artistTags.length === 0}
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

{#if showNewGroupDialog}
  <CreateArtistTagGroupDialog
    onConfirm={(name) => handleCreateGroup(name, [])}
    onCancel={() => { showNewGroupDialog = false; }}
  />
{/if}

{#if createGroupDialogTags}
  <CreateArtistTagGroupDialog
    initialTags={createGroupDialogTags}
    onConfirm={(name) => handleCreateGroup(name, createGroupDialogTags!)}
    onCancel={() => { createGroupDialogTags = null; }}
  />
{/if}

