<script lang="ts">
  import {
    TagIcon,
    SquaresFourIcon as LayoutGrid,
    RowsIcon as Rows3,
    PlusIcon as Plus
  } from "phosphor-svelte";
  import { onMount } from "svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { prefs, type GenreSortField } from "../stores/prefs.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import EmptyState from "./EmptyState.svelte";
  import Select from "./Select.svelte";
  import GenreCards from "./GenreCards.svelte";
  import ArtistTagCards from "./ArtistTagCards.svelte";
  import MergeSurvivorDialog from "./MergeSurvivorDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import CreateArtistTagGroupDialog from "./CreateArtistTagGroupDialog.svelte";

  let mergeDialogNames = $state<string[] | null>(null);
  let deleteConfirmNames = $state<string[] | null>(null);
  let showNewGroupDialog = $state(false);

  let genreNames = $derived(new Set(tagsStore.allTags.map((t) => t.name.toLowerCase())));

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
    return names.size;
  });

  async function handleCreateGroup(groupName: string, tagsToReparent: string[]) {
    showNewGroupDialog = false;
    await tagsStore.createArtistTagGroup(groupName);
    for (const tag of tagsToReparent) {
      await tagsStore.reparentArtistTag(tag, groupName);
    }
    if (tagsToReparent.length > 0) {
      toastStore.show(
        i18n.t("songTags.groupToast", { count: tagsToReparent.length, name: groupName }, `Grouped ${tagsToReparent.length} tags under "${groupName}"`),
        "success"
      );
    }
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

  /** Artist tag card click. */
  function openArtistTag(tagName: string) {
    navigationStore.viewArtistTag(tagName);
  }

  // Every card/chip click routes straight through to AutoPlaylistDetailView
  // (#548). A top-level card and a sub-genre chip resolve to the exact same
  // curated-tag auto-playlist lookup (navigationStore.viewGenreTag); dragging
  // a chip between cards only moves curation metadata.

  /** Root-level card click. */
  function openMainTag(tagName: string) {
    navigationStore.viewGenreTag(tagName);
  }

  /** Sub-genre chip click. */
  function openGenreEdge(_rootTag: string, childTag: string) {
    navigationStore.viewGenreTag(childTag);
  }

  /** Songs with no genre value at all. */
  function openNoGenre() {
    navigationStore.viewAutoPlaylist({ kind: "no_genre" });
  }
</script>

<div class="flex-1 px-6 pt-4 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
    <div class="h-9 flex items-center justify-end mb-3">
      <div class="flex items-center gap-2">
        <!-- Cards / rows toggle -->
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
        <!-- Sort dropdown -->
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
      </div>
    </div>

    {#if totalArtistTagCount > 0}
      <div class="mb-6">
        <div class="flex items-center justify-between mb-2.5">
          <h2 class="text-xl font-semibold text-brand-text-primary">
            {i18n.t("songTags.artistTagsHeading", { count: totalArtistTagCount }, `Artist Tags ${totalArtistTagCount}`)}
          </h2>
          <button
            type="button"
            onclick={() => { showNewGroupDialog = true; }}
            class="inline-flex items-center gap-1 text-xs font-semibold text-brand-accent-text hover:text-brand-accent-text-hover transition-colors"
          >
            <Plus class="w-3.5 h-3.5" />
            {i18n.t("songTags.newArtistGroup", {}, "New Group")}
          </button>
        </div>
        <ArtistTagCards
          hierarchy={artistOnlyHierarchy}
          selectMode={false}
          selected={new Set()}
          onToggleSelect={() => {}}
          onOpenTag={openArtistTag}
          sortField={prefs.genreSortField}
          sortAsc={prefs.genreSortAsc}
          compact={prefs.genreCardsViewMode === "rows"}
        />
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
    {:else}
      <h2 class="text-xl font-semibold text-brand-text-primary mb-3">
        {i18n.t("songTags.songTagsHeading", { count: tagsStore.hierarchy.length }, `Song Tags ${tagsStore.hierarchy.length}`)}
      </h2>
      <GenreCards
        selectMode={false}
        selected={new Set()}
        onToggleSelect={() => {}}
        onOpenMainTag={openMainTag}
        onOpenGenreEdge={openGenreEdge}
        sortField={prefs.genreSortField}
        sortAsc={prefs.genreSortAsc}
        compact={prefs.genreCardsViewMode === "rows"}
        noGenreCount={tagsStore.noGenreCount}
        onOpenNoGenre={openNoGenre}
      />
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
