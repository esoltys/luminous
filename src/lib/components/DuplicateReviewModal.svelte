<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    CaretLeftIcon as CaretLeft,
    CaretRightIcon as CaretRight,
    CheckIcon as Check,
    FolderIcon as Folder,
    HeartIcon as Heart,
    MusicNotesIcon as MusicNote,
    StarIcon as Star,
    TagIcon as Tag,
    WarningIcon as AlertTriangle,
    XIcon as X,
  } from "phosphor-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import CoverArt from "./CoverArt.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import {
    formatBitDepth,
    formatDate,
    formatDuration,
    formatFileSize,
    formatSampleRate,
  } from "../utils/formatters";
  import type { DuplicateCandidate, DuplicateTrackGroup, FileNavigationTarget } from "../types";

  interface Props {
    open?: boolean;
    groups?: DuplicateTrackGroup[];
    initialSongId?: number | null;
    onClose: () => void;
    onResolved?: () => void;
    onEditTags?: (songId: number) => void;
  }

  let {
    open = true,
    groups,
    initialSongId = null,
    onClose,
    onResolved,
    onEditTags,
  }: Props = $props();

  let activeGroups = $state<DuplicateTrackGroup[]>([]);
  let currentGroupIndex = $state(0);
  let userSelectedKeepId = $state<number | null>(null);
  let deleteFilesFromDisk = $state(true);
  let isResolving = $state(false);
  let showConfirmMerge = $state(false);
  let tagEditorSongId = $state<number | null>(null);
  let isLoading = $state(false);

  // Sync groups from prop or fetch if not provided
  $effect(() => {
    if (groups) {
      activeGroups = [...groups];
      if (initialSongId !== null) {
        const idx = groups.findIndex((g) => g.candidates.some((c) => c.song.id === initialSongId));
        if (idx !== -1) {
          currentGroupIndex = idx;
          userSelectedKeepId = initialSongId;
        }
      }
    } else if (open) {
      isLoading = true;
      invoke<DuplicateTrackGroup[]>("get_duplicate_track_groups")
        .then((fetched) => {
          activeGroups = fetched;
          if (initialSongId !== null) {
            const idx = fetched.findIndex((g) =>
              g.candidates.some((c) => c.song.id === initialSongId),
            );
            if (idx !== -1) {
              currentGroupIndex = idx;
              userSelectedKeepId = initialSongId;
            }
          }
        })
        .catch((err) => {
          console.error("Failed to load duplicate track groups:", err);
        })
        .finally(() => {
          isLoading = false;
        });
    }
  });

  let currentGroup = $derived(activeGroups[currentGroupIndex]);
  let candidates = $derived(currentGroup?.candidates ?? []);

  let selectedKeepId = $derived.by(() => {
    if (userSelectedKeepId !== null && candidates.some((c) => c.song.id === userSelectedKeepId)) {
      return userSelectedKeepId;
    }
    return candidates[0]?.song.id ?? null;
  });

  let keptCandidate = $derived(
    candidates.find((c) => c.song.id === selectedKeepId) ?? candidates[0],
  );
  let retiredCandidates = $derived(candidates.filter((c) => c.song.id !== keptCandidate?.song.id));

  // Reassurance calculations
  let extraPlays = $derived(retiredCandidates.reduce((acc, c) => acc + (c.song.playcount || 0), 0));
  let highestRetiredRating = $derived(
    retiredCandidates.reduce((max, c) => Math.max(max, c.song.rating || 0), 0),
  );
  let ratingUpgrade = $derived(
    keptCandidate && highestRetiredRating > (keptCandidate.song.rating || 0)
      ? highestRetiredRating
      : null,
  );
  let lovedUpgrade = $derived(
    keptCandidate?.song.loved !== 1 && retiredCandidates.some((c) => c.song.loved === 1),
  );
  let extraPlaylists = $derived(
    retiredCandidates.reduce((acc, c) => acc + (c.playlist_count || 0), 0),
  );
  let additionalTags = $derived.by(() => {
    if (!keptCandidate) return [];
    const keptLower = new Set((keptCandidate.tags || []).map((t) => t.trim().toLowerCase()));
    const added: string[] = [];
    const seen = new Set<string>();
    for (const ret of retiredCandidates) {
      for (const t of ret.tags || []) {
        const norm = t.trim().toLowerCase();
        if (!keptLower.has(norm) && seen.add(norm)) {
          added.push(t.trim());
        }
      }
    }
    return added;
  });

  let hasCarryOver = $derived(
    extraPlays > 0 ||
      ratingUpgrade !== null ||
      lovedUpgrade ||
      extraPlaylists > 0 ||
      additionalTags.length > 0,
  );

  function prevGroup() {
    if (currentGroupIndex > 0) {
      currentGroupIndex -= 1;
      userSelectedKeepId = null;
    }
  }

  function nextGroup() {
    if (currentGroupIndex < activeGroups.length - 1) {
      currentGroupIndex += 1;
      userSelectedKeepId = null;
    }
  }

  async function handleViewInFiles(songId: number) {
    try {
      const target = await invoke<FileNavigationTarget | null>("locate_song_in_files", { songId });
      if (target) {
        navigationStore.openInFiles(target);
        onClose();
      }
    } catch (e) {
      console.error("Failed to locate song in Files:", e);
    }
  }

  function handleEditTags(songId: number) {
    if (onEditTags) {
      onEditTags(songId);
    } else {
      tagEditorSongId = songId;
    }
  }

  async function executeResolve() {
    if (!keptCandidate || retiredCandidates.length === 0 || isResolving) return;

    isResolving = true;
    showConfirmMerge = false;

    const keepSongId = keptCandidate.song.id;
    const removeSongIds = retiredCandidates.map((c) => c.song.id);

    try {
      await invoke("resolve_duplicate_group", {
        keepSongId,
        removeSongIds,
        deleteFilesFromDisk,
      });

      // Remove resolved group from local state
      activeGroups = activeGroups.filter((_, idx) => idx !== currentGroupIndex);
      if (currentGroupIndex >= activeGroups.length && currentGroupIndex > 0) {
        currentGroupIndex = activeGroups.length - 1;
      }

      onResolved?.();

      if (activeGroups.length === 0) {
        onClose();
      }
    } catch (err) {
      console.error("Failed to resolve duplicate group:", err);
    } finally {
      isResolving = false;
    }
  }

  function formatFormatBadge(filetype?: string): string {
    if (!filetype || filetype === "UNKNOWN") return "AUDIO";
    return filetype.replace("OGG_", "").replace("_", " ").toUpperCase();
  }
</script>

{#if open}
  <Modal {onClose} maxWidth="max-w-4xl" class="h-[90vh] max-h-[850px] flex flex-col">
    <!-- Modal Header -->
    <div
      class="h-16 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-sidebar"
    >
      <div class="flex items-center gap-3 min-w-0">
        <div
          class="w-8 h-8 rounded-lg bg-brand-accent/15 text-brand-accent-text flex items-center justify-center shrink-0"
        >
          <MusicNote class="w-4 h-4" />
        </div>
        <div class="min-w-0">
          <h2 class="text-sm font-bold text-brand-text-primary truncate">
            {i18n.t("curation.duplicateModalTitle")}
          </h2>
          <p class="text-xs text-brand-text-secondary truncate">
            {i18n.t("curation.duplicateModalSubtitle")}
          </p>
        </div>
      </div>

      <div class="flex items-center gap-2 shrink-0">
        {#if activeGroups.length > 1}
          <div
            class="flex items-center gap-1.5 px-2 py-1 rounded bg-brand-main border border-brand-border text-xs text-brand-text-secondary mr-2"
          >
            <span>
              {i18n.t("curation.groupNavigator", {
                current: currentGroupIndex + 1,
                total: activeGroups.length,
              })}
            </span>
            <button
              type="button"
              onclick={prevGroup}
              disabled={currentGroupIndex === 0}
              class="p-0.5 rounded hover:bg-brand-sidebar disabled:opacity-40 transition-colors"
              title={i18n.t("curation.prevGroup")}
              aria-label={i18n.t("curation.prevGroup")}
            >
              <CaretLeft class="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              onclick={nextGroup}
              disabled={currentGroupIndex === activeGroups.length - 1}
              class="p-0.5 rounded hover:bg-brand-sidebar disabled:opacity-40 transition-colors"
              title={i18n.t("curation.nextGroup")}
              aria-label={i18n.t("curation.nextGroup")}
            >
              <CaretRight class="w-3.5 h-3.5" />
            </button>
          </div>
        {/if}

        <button
          type="button"
          onclick={onClose}
          class="p-1.5 rounded-lg text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main transition-colors"
          aria-label={i18n.t("curation.closeBtn")}
        >
          <X class="w-4 h-4" />
        </button>
      </div>
    </div>

    <!-- Modal Content Body -->
    <div class="flex-1 overflow-y-auto p-6 space-y-6">
      {#if isLoading}
        <div class="h-64 flex items-center justify-center text-sm text-brand-text-secondary">
          {i18n.t("curation.loadingDuplicates")}
        </div>
      {:else if activeGroups.length === 0}
        <div class="h-64 flex flex-col items-center justify-center text-center space-y-3">
          <div
            class="w-12 h-12 rounded-full bg-brand-accent/15 text-brand-accent-text flex items-center justify-center"
          >
            <Check class="w-6 h-6" />
          </div>
          <h3 class="text-sm font-bold text-brand-text-primary">
            {i18n.t("curation.allResolvedTitle")}
          </h3>
          <p class="text-xs text-brand-text-secondary max-w-sm">
            {i18n.t("curation.allResolvedMessage")}
          </p>
          <Button onclick={onClose} variant="secondary" size="sm">
            {i18n.t("curation.closeBtn")}
          </Button>
        </div>
      {:else if currentGroup}
        <!-- Group Title Info -->
        <div class="flex items-center justify-between border-b border-brand-border/40 pb-3">
          <div>
            <span class="text-base font-bold text-brand-text-primary">
              {currentGroup.title}
            </span>
            <span class="text-sm text-brand-text-secondary ml-2">
              • {currentGroup.artist}
              {#if currentGroup.album}
                <span class="text-brand-text-secondary/80">({currentGroup.album})</span>
              {/if}
            </span>
          </div>
        </div>

        <!-- Side-by-Side Comparison Grid -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          {#each candidates as candidate, idx (candidate.song.id)}
            {@const isSelected = candidate.song.id === selectedKeepId}
            {@const isRecommended = idx === 0}
            <div
              role="button"
              tabindex="0"
              onclick={() => {
                userSelectedKeepId = candidate.song.id;
              }}
              onkeydown={(e) => {
                if (e.key === " " || e.key === "Enter") userSelectedKeepId = candidate.song.id;
              }}
              class="relative rounded-xl border p-4 transition-all text-left flex flex-col justify-between cursor-pointer {isSelected
                ? 'bg-brand-accent/5 border-brand-accent ring-1 ring-brand-accent shadow-md'
                : 'bg-brand-main border-brand-border hover:border-brand-border/80'}"
            >
              <div class="space-y-4">
                <!-- Card Header -->
                <div class="flex items-start gap-3">
                  <CoverArt
                    songId={candidate.song.id}
                    artEmbedded={candidate.song.art_embedded}
                    artAutomatic={candidate.song.art_automatic}
                    artManual={candidate.song.art_manual}
                    sizeClass="w-14 h-14 rounded-lg shrink-0 shadow-sm"
                  />
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-1.5 flex-wrap">
                      <span class="font-bold text-xs text-brand-text-primary truncate">
                        {candidate.song.title || currentGroup.title}
                      </span>
                      {#if isRecommended}
                        <span
                          class="px-1.5 py-0.5 rounded text-[10px] font-medium border border-brand-border bg-brand-sidebar text-brand-text-secondary"
                        >
                          {i18n.t("curation.recommendedBadge")}
                        </span>
                      {/if}
                      {#if isSelected}
                        <span
                          class="px-1.5 py-0.5 rounded text-[10px] font-bold bg-brand-accent text-brand-accent-contrast"
                        >
                          {i18n.t("curation.selectedToPreserve")}
                        </span>
                      {/if}
                    </div>

                    <div class="text-[11px] text-brand-text-secondary truncate mt-0.5">
                      {candidate.song.album || "—"}
                      {#if candidate.song.year}
                        <span>({candidate.song.year})</span>
                      {/if}
                    </div>

                    <div class="text-[10px] text-brand-text-secondary/70 truncate">
                      {i18n.t("curation.trackDisc", {
                        track: candidate.song.track || "—",
                        disc: candidate.song.disc || "1",
                      })}
                    </div>
                  </div>
                </div>

                <!-- Section: Audio Specs -->
                <div
                  class="bg-brand-sidebar/60 rounded-lg p-3 border border-brand-border/40 space-y-2"
                >
                  <div
                    class="text-[10px] font-bold uppercase tracking-wider text-brand-text-secondary flex items-center justify-between"
                  >
                    <span>{i18n.t("curation.sectionAudio")}</span>
                    <span
                      class="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-brand-main border border-brand-border/60 text-brand-text-primary"
                    >
                      {formatFormatBadge(candidate.song.filetype)}
                    </span>
                  </div>

                  <div class="grid grid-cols-2 gap-2 text-xs">
                    <div>
                      <span class="text-[10px] text-brand-text-secondary block"
                        >{i18n.t("curation.qualityLabel")}</span
                      >
                      <span class="font-medium text-brand-text-primary">
                        {#if candidate.song.bitdepth && candidate.song.samplerate}
                          {formatBitDepth(candidate.song.bitdepth)} / {formatSampleRate(
                            candidate.song.samplerate,
                          )}
                        {:else if candidate.song.bitrate}
                          {candidate.song.bitrate} kbps
                        {:else}
                          —
                        {/if}
                      </span>
                    </div>

                    <div>
                      <span class="text-[10px] text-brand-text-secondary block"
                        >{i18n.t("curation.sizeDurationLabel")}</span
                      >
                      <span class="font-medium text-brand-text-primary">
                        {formatFileSize(candidate.song.filesize)} • {formatDuration(
                          candidate.song.length_nanosec,
                        )}
                      </span>
                    </div>
                  </div>

                  {#if candidate.provenance && candidate.provenance !== "authentic"}
                    <div
                      class="flex items-center gap-1.5 text-[11px] text-brand-text-primary bg-brand-sidebar px-2 py-1 rounded border border-brand-border"
                    >
                      <AlertTriangle class="w-3.5 h-3.5 shrink-0 text-brand-accent-text" />
                      <span class="truncate">
                        {i18n.t("curation.provenanceWarning", { provenance: candidate.provenance })}
                      </span>
                    </div>
                  {/if}
                </div>

                <!-- Section: Location -->
                <div
                  class="bg-brand-sidebar/60 rounded-lg p-3 border border-brand-border/40 space-y-1.5"
                >
                  <div class="flex items-center justify-between">
                    <span
                      class="text-[10px] font-bold uppercase tracking-wider text-brand-text-secondary"
                    >
                      {i18n.t("curation.sectionLocation")}
                    </span>
                    <button
                      type="button"
                      onclick={(e) => {
                        e.stopPropagation();
                        handleViewInFiles(candidate.song.id);
                      }}
                      class="text-[10px] text-brand-accent hover:underline flex items-center gap-1 font-medium"
                    >
                      <Folder class="w-3 h-3" />
                      <span>{i18n.t("curation.viewInFiles")}</span>
                    </button>
                  </div>
                  <p
                    class="text-[11px] text-brand-text-secondary font-mono truncate"
                    title={candidate.song.path}
                  >
                    {candidate.song.path || "—"}
                  </p>
                </div>

                <!-- Section: History & Tags -->
                <div
                  class="bg-brand-sidebar/60 rounded-lg p-3 border border-brand-border/40 space-y-2"
                >
                  <div class="flex items-center justify-between">
                    <span
                      class="text-[10px] font-bold uppercase tracking-wider text-brand-text-secondary block"
                    >
                      {i18n.t("curation.sectionHistory")}
                    </span>
                    {#if candidate.song.loved === 1}
                      <span
                        class="flex items-center gap-1 text-[11px] text-brand-text-primary font-medium"
                      >
                        <Heart class="w-3.5 h-3.5 fill-brand-accent-text text-brand-accent-text" />
                        <span>{i18n.t("curation.favouriteLabel")}</span>
                      </span>
                    {/if}
                  </div>

                  <div class="grid grid-cols-3 gap-2 text-xs">
                    <div>
                      <span class="text-[10px] text-brand-text-secondary block"
                        >{i18n.t("curation.playCountLabel")}</span
                      >
                      <span class="font-medium text-brand-text-primary tabular-nums">
                        {candidate.song.playcount || 0}
                      </span>
                    </div>

                    <div>
                      <span class="text-[10px] text-brand-text-secondary block"
                        >{i18n.t("curation.ratingLabel")}</span
                      >
                      <span class="font-medium text-brand-text-primary flex items-center gap-0.5">
                        {#if candidate.song.rating && candidate.song.rating > 0}
                          <Star class="w-3 h-3 text-brand-accent-text fill-brand-accent-text" />
                          <span
                            >{formatNumber(candidate.song.rating * 5, {
                              minimumFractionDigits: 1,
                              maximumFractionDigits: 1,
                            })}</span
                          >
                        {:else}
                          <span class="text-brand-text-secondary">—</span>
                        {/if}
                      </span>
                    </div>

                    <div>
                      <span class="text-[10px] text-brand-text-secondary block"
                        >{i18n.t("curation.playlistCountLabel")}</span
                      >
                      <span class="font-medium text-brand-text-primary tabular-nums">
                        {candidate.playlist_count}
                      </span>
                    </div>
                  </div>

                  <div
                    class="flex items-center justify-between text-xs pt-1 border-t border-brand-border/20"
                  >
                    <span class="text-[10px] text-brand-text-secondary"
                      >{i18n.t("curation.dateAddedLabel")}</span
                    >
                    <span class="text-[11px] text-brand-text-secondary tabular-nums">
                      {formatDate(candidate.song.added)}
                    </span>
                  </div>

                  <div class="pt-1">
                    <span class="text-[10px] text-brand-text-secondary block mb-1"
                      >{i18n.t("curation.tagsLabel")}</span
                    >
                    {#if candidate.tags && candidate.tags.length > 0}
                      <div class="flex items-center gap-1 flex-wrap">
                        {#each candidate.tags as tag}
                          <span
                            class="px-1.5 py-0.5 rounded bg-brand-main border border-brand-border/60 text-[10px] text-brand-text-secondary flex items-center gap-1"
                          >
                            <Tag class="w-2.5 h-2.5" />
                            <span>{tag}</span>
                          </span>
                        {/each}
                      </div>
                    {:else}
                      <span class="text-[10px] text-brand-text-secondary/60 italic">
                        {i18n.t("curation.noTags")}
                      </span>
                    {/if}
                  </div>
                </div>
              </div>

              <!-- Select Action Button inside card -->
              <div class="pt-3 mt-3 border-t border-brand-border/40 flex justify-end">
                <Button
                  size="sm"
                  variant={isSelected ? "primary" : "secondary"}
                  onclick={(e) => {
                    e.stopPropagation();
                    userSelectedKeepId = candidate.song.id;
                  }}
                  class="w-full justify-center"
                >
                  {#if isSelected}
                    <Check class="w-3.5 h-3.5 mr-1" />
                    <span>{i18n.t("curation.selectedToPreserve")}</span>
                  {:else}
                    <span>{i18n.t("curation.selectToPreserve")}</span>
                  {/if}
                </Button>
              </div>
            </div>
          {/each}
        </div>

        <!-- Carry-over Reassurance Banner -->
        <div class="rounded-xl border border-brand-accent/30 bg-brand-accent/5 p-4 space-y-3">
          <div class="flex items-start gap-2">
            <Check class="w-4 h-4 text-brand-accent shrink-0 mt-0.5" />
            <div class="space-y-1.5 flex-1">
              <span class="text-xs font-bold text-brand-text-primary block">
                {i18n.t("curation.reassuranceHeader")}
              </span>

              {#if hasCarryOver}
                <ul class="text-xs text-brand-text-secondary space-y-1 list-disc list-inside">
                  {#if extraPlays > 0}
                    <li>
                      {i18n.plural("curation.reassurancePlays", extraPlays, { count: extraPlays })}
                    </li>
                  {/if}
                  {#if ratingUpgrade !== null}
                    <li>
                      {i18n.t("curation.reassuranceRating", {
                        rating: formatNumber(ratingUpgrade * 5, {
                          minimumFractionDigits: 1,
                          maximumFractionDigits: 1,
                        }),
                      })}
                    </li>
                  {/if}
                  {#if lovedUpgrade}
                    <li>
                      {i18n.t("curation.reassuranceLoved")}
                    </li>
                  {/if}
                  {#if extraPlaylists > 0}
                    <li>
                      {i18n.plural("curation.reassurancePlaylists", extraPlaylists, {
                        count: extraPlaylists,
                      })}
                    </li>
                  {/if}
                  {#if additionalTags.length > 0}
                    <li>
                      {i18n.t("curation.reassuranceTags", { tags: additionalTags.join(", ") })}
                    </li>
                  {/if}
                </ul>
              {:else}
                <p class="text-xs text-brand-text-secondary">
                  {i18n.t("curation.reassuranceClean")}
                </p>
              {/if}
            </div>
          </div>

          <!-- Delete from disk toggle -->
          <div class="pt-2 border-t border-brand-accent/15 flex items-center justify-between">
            <label class="flex items-center gap-2 cursor-pointer select-none">
              <input
                type="checkbox"
                bind:checked={deleteFilesFromDisk}
                class="rounded border-brand-border text-brand-accent focus:ring-brand-accent/50"
              />
              <span class="text-xs font-medium text-brand-text-primary">
                {i18n.t("curation.deleteFromDiskLabel")}
              </span>
            </label>
            <span class="text-[10px] text-brand-text-secondary hidden sm:inline">
              {i18n.t("curation.deleteFromDiskHint")}
            </span>
          </div>
        </div>
      {/if}
    </div>

    <!-- Modal Footer -->
    {#if activeGroups.length > 0 && currentGroup}
      <div
        class="px-6 py-4 border-t border-brand-border shrink-0 bg-brand-sidebar flex items-center justify-between gap-3"
      >
        <Button
          variant="secondary"
          size="sm"
          onclick={() => handleEditTags(selectedKeepId ?? candidates[0]?.song.id)}
        >
          {i18n.t("curation.keepBothEditTags")}
        </Button>

        <div class="flex items-center gap-2">
          <Button variant="secondary" size="sm" onclick={onClose}>
            {i18n.t("curation.cancelBtn")}
          </Button>
          <Button
            variant="primary"
            size="sm"
            disabled={!selectedKeepId || isResolving}
            onclick={() => {
              showConfirmMerge = true;
            }}
          >
            {i18n.t("curation.preserveEditionBtn")}
          </Button>
        </div>
      </div>
    {/if}
  </Modal>
{/if}

<!-- Confirmation Dialog before permanent merge/delete -->
{#if showConfirmMerge}
  <ConfirmDialog
    title={i18n.t("curation.confirmMergeTitle")}
    message={deleteFilesFromDisk
      ? i18n.t("curation.confirmMergeMessageDisk")
      : i18n.t("curation.confirmMergeMessageDbOnly")}
    confirmLabel={i18n.t("curation.confirmMergeBtn")}
    cancelLabel={i18n.t("curation.cancelBtn")}
    danger={deleteFilesFromDisk}
    onConfirm={executeResolve}
    onCancel={() => {
      showConfirmMerge = false;
    }}
  />
{/if}

<!-- Inline TagEditor when opened via Keep Both -->
{#if tagEditorSongId !== null}
  <TagEditor
    songId={tagEditorSongId}
    onClose={() => {
      tagEditorSongId = null;
    }}
    onSave={() => {
      tagEditorSongId = null;
      onResolved?.();
    }}
  />
{/if}
