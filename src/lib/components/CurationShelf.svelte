<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { FolderIcon as Folder } from "phosphor-svelte";
  import { onDestroy } from "svelte";
  import type {
    AlbumItem,
    CurationAlbumItem,
    IncompleteAlbumItem,
    CurationOverview,
    FileNavigationTarget,
  } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { createEventScope } from "../utils/eventScope";
  import AlbumRowCard from "./AlbumRowCard.svelte";
  import AlbumProfileEditor from "./AlbumProfileEditor.svelte";
  import DuplicateReviewModal from "./DuplicateReviewModal.svelte";
  import Button from "./Button.svelte";

  /**
   * Props for Collection Curation Shelf.
   *
   * Designed as a zero-noise component that renders only actionable curation
   * categories (albums without cover art, albums with incomplete tracklists,
   * tracks missing core tags, duplicate recordings, and suspected provenance warnings).
   *
   * When totalCurationCount is 0, nothing is rendered.
   */
  interface Props {
    /** Parent refresh trigger key; re-fetches overview when incremented. */
    refreshKey?: number;
    /** Bound count of all items across all curation categories currently needing attention. */
    totalCurationCount?: number;
    /** Callback fired when an inline editor or modal resolves or modifies data. */
    onChanged?: () => void;
  }

  let {
    refreshKey = 0,
    totalCurationCount = $bindable(0),
    onChanged,
  }: Props = $props();

  let overview = $state<CurationOverview | null>(null);
  let isLoading = $state(false);

  // Modal and editor states
  let editingAlbum = $state<CurationAlbumItem | null>(null);
  let showDuplicateModal = $state(false);

  // Section list expansion toggles
  let showAllUnillustrated = $state(false);
  let showAllIncomplete = $state(false);

  const INITIAL_VISIBLE_COUNT = 6;

  const events = createEventScope();

  onDestroy(() => {
    events.dispose();
  });

  async function fetchOverview() {
    isLoading = true;
    try {
      const data = await invoke<CurationOverview>("get_curation_overview");
      overview = data ?? null;
    } catch (err) {
      console.error("Failed to load curation overview:", err);
      overview = null;
    } finally {
      isLoading = false;
    }
  }

  // Reactive fetch on parent refreshKey changes
  $effect(() => {
    // Read refreshKey to trigger re-fetch when parent bumps it
    const _key = refreshKey;
    fetchOverview();
  });

  // Listen for backend library updates (file scan, retagging, pruning)
  $effect(() => {
    events.listen("library-changed", () => {
      fetchOverview();
    });
  });

  // Derived total attention count across all 5 curation categories
  let totalCount = $derived(
    (overview?.unillustrated_albums.length ?? 0) +
      (overview?.incomplete_albums.length ?? 0) +
      (overview?.missing_core_tags_count ?? 0) +
      (overview?.duplicate_groups.length ?? 0) +
      (overview?.suspected_provenance_songs.length ?? 0)
  );

  // Sync bindable totalCurationCount to parent OrganizeView
  $effect(() => {
    totalCurationCount = totalCount;
  });

  let displayedUnillustrated = $derived(
    overview?.unillustrated_albums
      ? showAllUnillustrated
        ? overview.unillustrated_albums
        : overview.unillustrated_albums.slice(0, INITIAL_VISIBLE_COUNT)
      : []
  );

  let displayedIncomplete = $derived(
    overview?.incomplete_albums
      ? showAllIncomplete
        ? overview.incomplete_albums
        : overview.incomplete_albums.slice(0, INITIAL_VISIBLE_COUNT)
      : []
  );

  function toAlbumItem(album: CurationAlbumItem): AlbumItem {
    return {
      album: album.album,
      artist: album.artist ?? null,
      year: album.year ?? null,
      track_count: album.track_count,
      disc_count: album.disc_count,
      art_embedded: album.art_embedded,
      art_automatic: album.art_automatic ?? null,
      art_manual: album.art_manual ?? null,
      genre: album.genre ?? null,
      sample_song_id: album.sample_song_id ?? null,
      rating: album.rating,
      total_duration_nanosec: 0,
    };
  }

  function openAlbumEditor(album: CurationAlbumItem) {
    editingAlbum = album;
  }

  async function locateInFiles(songId: number) {
    try {
      const target = await invoke<FileNavigationTarget | null>("locate_song_in_files", { songId });
      if (target) {
        navigationStore.openInFiles(target);
      }
    } catch (err) {
      console.error("Failed to locate song in files:", err);
    }
  }
</script>

{#if totalCount > 0 && overview}
  <div class="@container max-w-3xl mx-auto space-y-4">
    <!-- Category 1: Albums Missing Cover Art -->
    {#if overview.unillustrated_albums.length > 0}
      <div
        class="bg-brand-sidebar/40 border border-brand-border/60 rounded-xl p-4 space-y-3"
        data-testid="curation-unillustrated-section"
      >
        <div class="flex items-center justify-between gap-3">
          <div>
            <div class="flex items-center gap-2">
              <h3 class="text-sm font-bold text-brand-text-primary">
                {i18n.t("curation.shelfUnillustratedTitle")}
              </h3>
              <span class="text-xs px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-accent-text font-semibold">
                {overview.unillustrated_albums.length}
              </span>
            </div>
            <p class="text-xs text-brand-text-secondary mt-0.5">
              {i18n.t("curation.shelfUnillustratedHint")}
            </p>
          </div>
        </div>

        <div class="grid grid-cols-1 @3xl:grid-cols-2 gap-3">
          {#each displayedUnillustrated as album (album.album_key)}
            <AlbumRowCard
              album={toAlbumItem(album)}
              onclick={() => openAlbumEditor(album)}
            >
              {#snippet suffix()}
                <Button
                  variant="secondary"
                  size="sm"
                  onclick={(e) => {
                    e.stopPropagation();
                    openAlbumEditor(album);
                  }}
                >
                  {i18n.t("curation.addArtworkBtn")}
                </Button>
              {/snippet}
            </AlbumRowCard>
          {/each}
        </div>

        {#if overview.unillustrated_albums.length > INITIAL_VISIBLE_COUNT}
          <div class="flex justify-center pt-1">
            <Button
              variant="secondary"
              size="sm"
              onclick={() => { showAllUnillustrated = !showAllUnillustrated; }}
            >
              {showAllUnillustrated
                ? i18n.t("curation.showLess")
                : i18n.t("curation.showAllWithCount", { count: overview.unillustrated_albums.length })}
            </Button>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Category 2: Albums with Incomplete Tracklists -->
    {#if overview.incomplete_albums.length > 0}
      <div
        class="bg-brand-sidebar/40 border border-brand-border/60 rounded-xl p-4 space-y-3"
        data-testid="curation-incomplete-section"
      >
        <div class="flex items-center justify-between gap-3">
          <div>
            <div class="flex items-center gap-2">
              <h3 class="text-sm font-bold text-brand-text-primary">
                {i18n.t("curation.shelfIncompleteTitle")}
              </h3>
              <span class="text-xs px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-accent-text font-semibold">
                {overview.incomplete_albums.length}
              </span>
            </div>
            <p class="text-xs text-brand-text-secondary mt-0.5">
              {i18n.t("curation.shelfIncompleteHint")}
            </p>
          </div>
        </div>

        <div class="grid grid-cols-1 @3xl:grid-cols-2 gap-3">
          {#each displayedIncomplete as album (album.album_key)}
            <AlbumRowCard
              album={toAlbumItem(album)}
              onclick={() => navigationStore.viewAlbum(album.album)}
            >
              {#snippet suffix()}
                <div class="flex items-center gap-2">
                  {#if album.unnumbered_count > 0}
                    <span class="text-xs px-2 py-0.5 rounded-full border border-brand-border bg-brand-main text-brand-text-secondary font-medium whitespace-nowrap">
                      {i18n.plural("curation.unnumberedTracksBadge", album.unnumbered_count, { count: album.unnumbered_count })}
                    </span>
                  {:else if album.duplicate_track_number_count > 0}
                    <span class="text-xs px-2 py-0.5 rounded-full border border-brand-border bg-brand-main text-brand-text-secondary font-medium whitespace-nowrap">
                      {i18n.plural("curation.duplicateTrackNumbersBadge", album.duplicate_track_number_count, { count: album.duplicate_track_number_count })}
                    </span>
                  {/if}

                  {#if picardStore.available}
                    <Button
                      variant="secondary"
                      size="sm"
                      onclick={(e) => {
                        e.stopPropagation();
                        picardStore.openSongs(album.song_ids);
                      }}
                      title={i18n.t("curation.openInPicardBtn")}
                    >
                      {i18n.t("curation.openInPicardBtn")}
                    </Button>
                  {/if}
                </div>
              {/snippet}
            </AlbumRowCard>
          {/each}
        </div>

        {#if overview.incomplete_albums.length > INITIAL_VISIBLE_COUNT}
          <div class="flex justify-center pt-1">
            <Button
              variant="secondary"
              size="sm"
              onclick={() => { showAllIncomplete = !showAllIncomplete; }}
            >
              {showAllIncomplete
                ? i18n.t("curation.showLess")
                : i18n.t("curation.showAllWithCount", { count: overview.incomplete_albums.length })}
            </Button>
          </div>
        {/if}
      </div>
    {/if}

    <!-- Category 3: Tracks Missing Core Metadata -->
    {#if overview.missing_core_tags_count > 0}
      <div
        class="flex flex-col @sm:flex-row items-start @sm:items-center justify-between gap-3 p-4 rounded-xl bg-brand-sidebar/40 border border-brand-border/60"
        data-testid="curation-missing-core-section"
      >
        <div class="min-w-0 flex-1">
          <h3 class="text-sm font-bold text-brand-text-primary">
            {i18n.plural("curation.shelfMissingCoreTitle", overview.missing_core_tags_count, { count: overview.missing_core_tags_count })}
          </h3>
          <p class="text-xs text-brand-text-secondary mt-0.5">
            {i18n.t("curation.shelfMissingCoreHint")}
          </p>
        </div>
        <Button
          variant="secondary"
          size="sm"
          onclick={() => navigationStore.viewAutoPlaylist({ kind: "missing_metadata" })}
        >
          {i18n.t("curation.reviewMissingMetadataBtn")}
        </Button>
      </div>
    {/if}

    <!-- Category 4: Duplicate Recordings -->
    {#if overview.duplicate_groups.length > 0}
      <div
        class="flex flex-col @sm:flex-row items-start @sm:items-center justify-between gap-3 p-4 rounded-xl bg-brand-sidebar/40 border border-brand-border/60"
        data-testid="curation-duplicates-section"
      >
        <div class="min-w-0 flex-1">
          <h3 class="text-sm font-bold text-brand-text-primary">
            {i18n.plural("curation.shelfDuplicatesTitle", overview.duplicate_groups.length, { count: overview.duplicate_groups.length })}
          </h3>
          <p class="text-xs text-brand-text-secondary mt-0.5">
            {i18n.t("curation.shelfDuplicatesHint")}
          </p>
        </div>
        <Button
          variant="secondary"
          size="sm"
          onclick={() => { showDuplicateModal = true; }}
        >
          {i18n.t("curation.reviewDuplicatesBtn")}
        </Button>
      </div>
    {/if}

    <!-- Category 5: Audio Provenance Notices -->
    {#if overview.suspected_provenance_songs.length > 0}
      <div
        class="bg-brand-sidebar/40 border border-brand-border/60 rounded-xl p-4 space-y-3"
        data-testid="curation-provenance-section"
      >
        <div class="flex items-center justify-between gap-3">
          <div>
            <div class="flex items-center gap-2">
              <h3 class="text-sm font-bold text-brand-text-primary">
                {i18n.t("curation.shelfProvenanceTitle")}
              </h3>
              <span class="text-xs px-2 py-0.5 rounded-full bg-brand-accent/15 text-brand-accent-text font-semibold">
                {overview.suspected_provenance_songs.length}
              </span>
            </div>
            <p class="text-xs text-brand-text-secondary mt-0.5">
              {i18n.t("curation.shelfProvenanceHint")}
            </p>
          </div>
        </div>

        <div class="divide-y divide-brand-border/40">
          {#each overview.suspected_provenance_songs as song (song.id)}
            <div class="flex items-center justify-between gap-3 py-2.5 first:pt-0 last:pb-0">
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-2">
                  <span class="text-sm font-medium text-brand-text-primary truncate">
                    {song.title || i18n.t("collection.unknownTitle")}
                  </span>
                  <span class="text-xs px-2 py-0.5 rounded-full border border-brand-border bg-brand-main text-brand-text-secondary font-medium whitespace-nowrap">
                    {song.provenance || i18n.t("curation.shelfProvenanceBadge")}
                  </span>
                </div>
                <p class="text-xs text-brand-text-secondary truncate mt-0.5">
                  {song.artist || i18n.t("collection.unknownArtist")}
                </p>
              </div>
              <Button
                variant="secondary"
                size="sm"
                onclick={() => locateInFiles(song.id)}
              >
                <Folder class="w-3.5 h-3.5" />
                <span>{i18n.t("curation.viewInFiles")}</span>
              </Button>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
{/if}

<!-- Modals & Sub-Editors -->
{#if editingAlbum}
  <AlbumProfileEditor
    albumName={editingAlbum.album}
    artistName={editingAlbum.artist}
    songIds={editingAlbum.song_ids}
    initialAlbum={editingAlbum.album}
    initialAlbumArtist={editingAlbum.artist}
    initialGenre={editingAlbum.genre}
    initialYear={editingAlbum.year}
    hasEmbeddedArt={editingAlbum.art_embedded}
    initialArtAutomatic={editingAlbum.art_automatic}
    initialArtManual={editingAlbum.art_manual}
    isOpen={true}
    onClose={() => { editingAlbum = null; }}
    onSaved={() => {
      editingAlbum = null;
      fetchOverview();
      onChanged?.();
    }}
  />
{/if}

{#if showDuplicateModal && overview?.duplicate_groups}
  <DuplicateReviewModal
    groups={overview.duplicate_groups}
    onClose={() => { showDuplicateModal = false; }}
    onResolved={() => {
      showDuplicateModal = false;
      fetchOverview();
      onChanged?.();
    }}
  />
{/if}
