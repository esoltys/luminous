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
  import Button from "./Button.svelte";
  import GenreCards from "./GenreCards.svelte";
  import ArtistTagCards from "./ArtistTagCards.svelte";
  import CreateArtistTagGroupDialog from "./CreateArtistTagGroupDialog.svelte";

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
    <div class="h-10 flex items-center justify-between mb-3">
      {#if totalArtistTagCount > 0}
        <Button onclick={() => { showNewGroupDialog = true; }} variant="primary" title={i18n.t('songTags.newArtistGroup', {}, 'New Group')}>
          <Plus class="w-4 h-4" />
          <span>{i18n.t('songTags.newArtistGroup', {}, 'New Group')}</span>
        </Button>
      {:else}
        <div></div>
      {/if}
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
        <h2 class="text-xl font-semibold text-brand-text-primary mb-2.5">
          {i18n.t("songTags.artistTagsHeading", { count: totalArtistTagCount }, `Artist Tags ${totalArtistTagCount}`)}
        </h2>
        <ArtistTagCards
          hierarchy={artistOnlyHierarchy}
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

{#if showNewGroupDialog}
  <CreateArtistTagGroupDialog
    onConfirm={(name) => handleCreateGroup(name, [])}
    onCancel={() => { showNewGroupDialog = false; }}
  />
{/if}
