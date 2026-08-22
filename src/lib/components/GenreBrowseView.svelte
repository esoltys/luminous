<script lang="ts">
  import { Tag as TagIcon, ArrowLeft, Music, DiscAlbum, GitMerge, X as XIcon, CheckSquare } from "lucide-svelte";
  import { genreColorHsl } from "../utils/genrePalette";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { prefs, type GenreViewMode } from "../stores/prefs.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore, type GenreDrillDown } from "../stores/collection.svelte";
  import EmptyState from "./EmptyState.svelte";
  import Select from "./Select.svelte";
  import GenreChips from "./GenreChips.svelte";
  import GenreCards from "./GenreCards.svelte";
  import MergeSurvivorDialog from "./MergeSurvivorDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import TagEditor from "./TagEditor.svelte";
  import AlbumTagEditor from "./AlbumTagEditor.svelte";
  import type { Song } from "../types";

  let selectMode = $state(false);
  let selected = $state<Set<string>>(new Set());
  let mergeDialogNames = $state<string[] | null>(null);
  let mergeDialogSuggestionPair = $state<[string, string] | null>(null);
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
    mergeDialogSuggestionPair = null;
    mergeDialogNames = Array.from(selected);
  }

  function openDeleteSelected() {
    if (selected.size === 0) return;
    deleteConfirmNames = Array.from(selected);
  }

  function acceptSuggestion(a: string, b: string) {
    mergeDialogSuggestionPair = [a, b];
    mergeDialogNames = [a, b];
  }

  async function confirmMerge(survivor: string) {
    const names = mergeDialogNames ?? [];
    const suggestionPair = mergeDialogSuggestionPair;
    mergeDialogNames = null;
    mergeDialogSuggestionPair = null;
    const others = names.filter((n) => n !== survivor);
    let total = 0;
    for (const other of others) {
      total += await tagsStore.mergeTags(other, survivor);
    }
    if (suggestionPair) tagsStore.dismissSuggestion(suggestionPair[0], suggestionPair[1]);
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

  let drillDownSongs = $state<Song[]>([]);
  let drillDownLoading = $state(false);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);
  let editingSongId = $state<number | null>(null);
  let editingAlbumSongs = $state<Song[] | null>(null);

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  function handleContextMenu(e: MouseEvent, song: Song) {
    e.preventDefault();
    contextMenuState = { x: e.clientX, y: e.clientY, song };
  }

  async function openAlbumEditor(song: Song) {
    if (!song.album) return;
    editingAlbumSongs = await invoke<Song[]>("get_songs_by_album", { album: song.album });
  }

  // Persisted on collectionStore (not local state) so Back/Forward restores
  // the Genres tab's drill-down the same way it already does for the
  // Artist/Album/Playlist detail views.
  let selectedTag = $derived(collectionStore.selectedGenreDrillDown?.displayTag ?? null);

  function fetchForContext(ctx: GenreDrillDown) {
    if (ctx.kind === "tag") return tagsStore.getSongsByTag(ctx.tag!, 500);
    if (ctx.kind === "main") return tagsStore.getSongsByMainTag(ctx.tag!, 500);
    if (ctx.kind === "none") return tagsStore.getSongsWithoutGenre(500);
    return tagsStore.getSongsByGenreEdge(ctx.root!, ctx.tag!, 500);
  }

  function refreshDrillDown() {
    const ctx = collectionStore.selectedGenreDrillDown;
    if (ctx) {
      fetchForContext(ctx).then((songs) => { drillDownSongs = songs; });
    }
  }

  function handleEditorSaved() {
    tagsStore.load();
    refreshDrillDown();
  }

  // This view mounts/unmounts with the tab (not a persistent singleton like
  // collectionStore), so the listener must be torn down on unmount — an
  // uncleaned Tauri listen() here would leak and re-fire once per every past
  // visit to this tab, compounding on itself.
  onMount(() => {
    let unlisten: (() => void) | undefined;
    let unlistenHierarchy: (() => void) | undefined;
    listen("library-changed", refreshDrillDown).then((fn) => { unlisten = fn; });
    tagsStore.listenForHierarchyChanges().then((fn) => { unlistenHierarchy = fn; });
    tagsStore.loadHierarchy();
    tagsStore.loadMergeSuggestions();
    // Restore a drill-down carried over from a previous visit/session (e.g.
    // Back/Forward or app relaunch) by re-fetching its songs.
    refreshDrillDown();
    return () => {
      unlisten?.();
      unlistenHierarchy?.();
    };
  });

  async function loadDrillDown(ctx: GenreDrillDown) {
    collectionStore.selectedGenreDrillDown = ctx;
    drillDownLoading = true;
    try {
      drillDownSongs = await fetchForContext(ctx);
    } finally {
      drillDownLoading = false;
    }
  }

  /** Any-position match — flat Tags view, and external "browse this value"
   * navigation (e.g. a GenreChips chip clicked elsewhere in the app) where
   * there's no specific root/child relationship to narrow by. */
  function openTag(tagName: string) {
    return loadDrillDown({ kind: "tag", tag: tagName, displayTag: tagName });
  }

  /** Strict main-tag match — clicking a root in the Genre view. */
  function openMainTag(tagName: string) {
    return loadDrillDown({ kind: "main", tag: tagName, displayTag: tagName });
  }

  /** Exact root/child edge match — clicking a child under a specific root in
   * the Genre view, so a reordered song (no longer under this root) drops
   * out immediately, and a tag shared under multiple roots only shows the
   * songs for *this* relationship. */
  function openGenreEdge(rootTag: string, childTag: string) {
    return loadDrillDown({ kind: "edge", root: rootTag, tag: childTag, displayTag: childTag });
  }

  /** Songs with no genre value at all. */
  function openNoGenre() {
    return loadDrillDown({ kind: "none", displayTag: i18n.t("songTags.noGenre", {}, "No Genre") });
  }

  function closeDrillDown() {
    collectionStore.selectedGenreDrillDown = null;
  }

  // Consumes collectionStore.viewGenreTag()'s one-shot "open this tag" signal
  // — e.g. a genre chip clicked elsewhere in the app.
  $effect(() => {
    const tag = collectionStore.pendingGenreTag;
    if (tag) {
      collectionStore.pendingGenreTag = null;
      openTag(tag);
    }
  });

  function setViewMode(mode: GenreViewMode) {
    prefs.setGenreViewMode(mode);
    closeDrillDown();
  }

  async function playAllInTag() {
    if (sortedDrillDownSongs.length === 0) return;
    await playerStore.playSongs(
      sortedDrillDownSongs.map((s) => s.id),
      0,
      undefined,
      { type: "song" },
      selectedTag ?? undefined
    );
  }

  type DrillDownSortField = "title" | "artist" | "album" | "added";
  let sortField = $state<DrillDownSortField>("artist");
  let sortAsc = $state(true);

  let sortedDrillDownSongs = $derived.by(() => {
    const sorted = [...drillDownSongs];
    sorted.sort((a, b) => {
      if (sortField === "added") {
        const diff = (a.added ?? 0) - (b.added ?? 0);
        return sortAsc ? diff : -diff;
      }
      const valA = (a[sortField] || "").toString();
      const valB = (b[sortField] || "").toString();
      const cmp = valA.localeCompare(valB);
      return sortAsc ? cmp : -cmp;
    });
    return sorted;
  });

  // Tag-cloud font sizing: log-scaled against the current min/max song
  // count so the range is actually perceptible regardless of whether counts
  // span 1-10 or 1-1000 (a plain linear/count-divided-by-N scale washes out
  // at either extreme).
  const TAG_CLOUD_MIN_REM = 0.75;
  const TAG_CLOUD_MAX_REM = 2;
  let tagCountRange = $derived.by(() => {
    const counts = tagsStore.allTags.map((t) => t.song_count);
    return { min: Math.min(...counts, 1), max: Math.max(...counts, 1) };
  });
  function tagCloudFontSize(count: number): string {
    const { min, max } = tagCountRange;
    if (max <= min) return `${TAG_CLOUD_MIN_REM}rem`;
    const t = (Math.log(count + 1) - Math.log(min + 1)) / (Math.log(max + 1) - Math.log(min + 1));
    return `${(TAG_CLOUD_MIN_REM + t * (TAG_CLOUD_MAX_REM - TAG_CLOUD_MIN_REM)).toFixed(2)}rem`;
  }
</script>

<div class="flex-1 px-6 pt-4 overflow-y-auto {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
  {#if selectedTag !== null}
    <div class="h-9 flex items-center justify-between gap-2 mb-3">
      <div class="flex items-center gap-2 min-w-0">
        <button
          onclick={closeDrillDown}
          class="flex items-center gap-1.5 text-xs font-medium text-brand-text-secondary hover:text-brand-text-primary transition-colors shrink-0"
        >
          <ArrowLeft class="w-3.5 h-3.5" />
          {i18n.t("common.back", {}, "Back")}
        </button>
        <span class="text-brand-text-secondary/40 shrink-0">/</span>
        <h2 class="text-sm font-bold text-brand-text-primary truncate">{selectedTag}</h2>
      </div>
      <div class="relative shrink-0">
        <Select
          value={`${sortField}-${sortAsc}`}
          onchange={(e) => {
            const [field, asc] = e.currentTarget.value.split("-");
            sortField = field as DrillDownSortField;
            sortAsc = asc === "true";
          }}
          class="bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-brand-text-secondary text-xs rounded-full pl-3.5 pr-8 py-1.5 focus:outline-none focus:border-brand-accent transition-all font-medium"
        >
          <option value="artist-true">▲ {i18n.t('collection.tableHeaderArtist')}</option>
          <option value="artist-false">▼ {i18n.t('collection.tableHeaderArtist')}</option>
          <option value="album-true">▲ {i18n.t('collection.tableHeaderAlbum')}</option>
          <option value="album-false">▼ {i18n.t('collection.tableHeaderAlbum')}</option>
          <option value="title-true">▲ {i18n.t('collection.tableHeaderTitle')}</option>
          <option value="title-false">▼ {i18n.t('collection.tableHeaderTitle')}</option>
          <option value="added-true">▲ {i18n.t('collection.sortDateAddedLabel')}</option>
          <option value="added-false">▼ {i18n.t('collection.sortDateAddedLabel')}</option>
        </Select>
      </div>
    </div>

    {#if drillDownLoading}
      <div class="py-16 text-center text-sm text-brand-text-secondary">{i18n.t("common.loading", {}, "Loading…")}</div>
    {:else}
      {#if sortedDrillDownSongs.length > 0}
        <button
          onclick={playAllInTag}
          class="mb-3 text-xs font-semibold text-brand-accent-text hover:text-brand-accent-text-hover transition-colors"
        >
          {i18n.t("songTags.playAll", { count: sortedDrillDownSongs.length }, `Play all ${sortedDrillDownSongs.length} songs`)}
        </button>
      {/if}
      <div class="flex flex-col gap-2">
        {#each sortedDrillDownSongs as song (song.id)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            role="button"
            tabindex="0"
            onclick={() => playerStore.playSong(song.id)}
            oncontextmenu={(e) => handleContextMenu(e, song)}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); playerStore.playSong(song.id); } }}
            class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
          >
            <CoverArt
              songId={song.id}
              artEmbedded={song.art_embedded}
              artAutomatic={song.art_automatic}
              artManual={song.art_manual}
              sizeClass="w-11 h-11 shrink-0"
            />
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-2">
                <p class="truncate text-sm font-semibold text-brand-text-primary">{song.title || i18n.t('collection.unknownSong')}</p>
                <span class="shrink-0">
                  <SongRating rating={song.rating} onRate={(r) => rateSong(song, r)} />
                </span>
              </div>
              <p class="truncate text-xs text-brand-text-secondary font-medium">{song.artist || i18n.t('collection.unknownArtist')}</p>
            </div>
            {#if song.genre}
              <div class="shrink-0 hidden sm:flex items-center max-w-40">
                <GenreChips genre={song.genre} />
              </div>
            {/if}
            <div class="shrink-0 flex items-center gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity">
              <button
                onclick={(e) => { e.stopPropagation(); editingSongId = song.id; }}
                class="text-brand-text-secondary hover:text-brand-accent-text transition-colors"
                title={i18n.t('songTags.editSongTooltip', {}, 'Edit Song')}
              >
                <Music class="w-4 h-4" />
              </button>
              <button
                onclick={(e) => { e.stopPropagation(); openAlbumEditor(song); }}
                class="text-brand-text-secondary hover:text-brand-accent-text transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                disabled={!song.album}
                title={i18n.t('songTags.editAlbumTooltip', {}, 'Edit Album')}
              >
                <DiscAlbum class="w-4 h-4" />
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {:else}
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

    {#each tagsStore.mergeSuggestions as [a, b] (`${a}|${b}`)}
      <div class="flex items-center justify-between gap-3 mb-3 px-3 py-2 rounded-lg bg-brand-accent/10 border border-brand-accent/25">
        <div class="flex items-center gap-2 min-w-0 text-xs text-brand-text-primary">
          <GitMerge class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
          <span class="truncate">
            {i18n.t("songTags.mergeSuggestion", { a, b }, `"${a}" and "${b}" look similar`)}
          </span>
        </div>
        <div class="flex items-center gap-3 shrink-0">
          <button
            onclick={() => acceptSuggestion(a, b)}
            class="text-xs font-semibold text-brand-accent-text hover:text-brand-accent-text-hover transition-colors"
          >
            {i18n.t("songTags.mergeConfirm", {}, "Merge")}
          </button>
          <button
            onclick={() => tagsStore.dismissSuggestion(a, b)}
            class="text-brand-text-secondary hover:text-brand-text-primary transition-colors"
            aria-label={i18n.t("songTags.dismissSuggestion", {}, "Dismiss")}
          >
            <XIcon class="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    {/each}

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
      <GenreCards {selectMode} {selected} onToggleSelect={toggleSelect} onOpenMainTag={openMainTag} onOpenGenreEdge={openGenreEdge} />
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
      <div class="flex flex-wrap gap-2">
        {#each tagsStore.allTags as tag (tag.name)}
          {@const group = tagsStore.hierarchy.find((g) => g.name === tag.name || g.children.some((c) => c.name === tag.name))}
          {@const colorIndex = group?.color_index}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span
            onclick={() => { if (selectMode) toggleSelect(tag.name); }}
            class="inline-flex items-baseline gap-1.5 px-3 py-1.5 rounded-full font-bold transition-colors {selectMode ? 'cursor-pointer' : ''} {selected.has(tag.name) ? 'ring-2 ring-brand-accent' : ''}"
            style={`font-size: ${tagCloudFontSize(tag.song_count)}; ${
              colorIndex !== undefined
                ? `background-color: color-mix(in srgb, ${genreColorHsl(colorIndex)} 32%, black); color: color-mix(in srgb, ${genreColorHsl(colorIndex)} 85%, white);`
                : "background-color: var(--color-brand-sidebar); color: var(--color-brand-text-primary);"
            }`}
          >
            {#if selectMode}
              <input type="checkbox" checked={selected.has(tag.name)} onchange={() => toggleSelect(tag.name)} class="self-center w-3 h-3 pointer-events-none" />
            {/if}
            <button onclick={() => !selectMode && openTag(tag.name)} class="leading-none">
              {tag.name}
            </button>
            <span class="text-[0.65em] font-bold opacity-70 leading-none">{tag.song_count}</span>
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
  {/if}
</div>

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    onPlay={() => playerStore.playSong(song.id)}
    onGoToArtist={() => collectionStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => collectionStore.viewAlbum(song.album || "")}
    onEditTags={() => { editingSongId = song.id; }}
    onClose={() => { contextMenuState = null; }}
  />
{/if}

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleEditorSaved}
  />
{/if}

{#if editingAlbumSongs !== null && editingAlbumSongs.length > 0}
  <AlbumTagEditor
    songIds={editingAlbumSongs.map((s) => s.id)}
    initialAlbum={editingAlbumSongs[0].album}
    initialAlbumArtist={editingAlbumSongs[0].album_artist || editingAlbumSongs[0].artist}
    initialGenre={editingAlbumSongs[0].genre}
    initialYear={editingAlbumSongs[0].year}
    initialDisc={editingAlbumSongs[0].disc}
    initialCompilation={editingAlbumSongs[0].compilation}
    hasEmbeddedArt={editingAlbumSongs.some((s) => s.art_embedded)}
    onClose={() => { editingAlbumSongs = null; }}
    onSave={handleEditorSaved}
  />
{/if}

{#if mergeDialogNames}
  <MergeSurvivorDialog
    names={mergeDialogNames}
    onConfirm={confirmMerge}
    onCancel={() => { mergeDialogNames = null; mergeDialogSuggestionPair = null; }}
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
