<script lang="ts">
  import { isRemotePath } from "../utils/remoteSource";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import {
    SlidersIcon as Sliders,
    FloppyDiskIcon as Save,
    XIcon as X,
    CircleNotchIcon as LoaderCircle,
    WarningIcon as AlertTriangle,
    LockIcon as Lock,
    ImageBrokenIcon as ImageOff,
    CloudIcon,
    FolderOpenIcon as FolderOpen,
    BroomIcon as Broom
  } from "phosphor-svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import SongRating from "./SongRating.svelte";
  import FormField from "./FormField.svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import Modal from "./Modal.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import TagCleanupDialog from "./TagCleanupDialog.svelte";
  import CoverArt from "./CoverArt.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import ChipInput from "./ChipInput.svelte";
  import PlainChipInput from "./PlainChipInput.svelte";
  import HelpTip from "./HelpTip.svelte";
  import { formatDate, formatFileSize } from "../utils/formatters";
  import type { FileSource, SongTagHistory, SongHistoryEntry, RestoreTagHistoryResult } from "../types";
  import { filesStore } from "../stores/files.svelte";
  import { scanFolder } from "./files/folderActions";

  interface Props {
    songId?: number;
    filePath?: string;
    source?: FileSource;
    folderPath?: string[];
    folderName?: string;
    fileName?: string;
    onClose: () => void;
    onSave?: () => void;
  }

  let {
    songId,
    filePath,
    source,
    folderPath,
    folderName,
    fileName,
    onClose,
    onSave
  }: Props = $props();

  let title = $state("");
  let artist = $state("");
  let album = $state("");
  let albumArtist = $state("");
  let composer = $state("");
  let genre = $state("");
  let track = $state<number | null>(null);
  let disc = $state<number | null>(null);
  let discSubtitle = $state("");
  let year = $state<number | null>(null);
  let originalYear = $state<number | null>(null);
  let grouping = $state("");
  let bpm = $state<number | null>(null);
  let initialKey = $state("");
  let path = $state("");
  let rating = $state(-1);
  let loved = $state<number | undefined>(undefined);
  // Remote songs (WebDAV #682, OpenSubsonic #916) have no local file Luminous can write lofty tags to,
  // and there's no write-back to the remote server -- edits here only ever
  // reach Luminous's own DB. Derived from the path scheme rather than a
  // dedicated field since it's the same signal media_source.rs/collection.rs
  // already key off of for "is this a remote source" checks.
  let isRemoteSource = $derived(isRemotePath(path));
  // CUE sheet tracks (#78) share one physical file's embedded tags across
  // every track cut from it, so there's nowhere to persist a per-track edit
  // back to disk yet -- the backend rejects the save outright, so keep the
  // editor read-only here instead of letting the user hit a failed save.
  let isCueTrack = $state(false);
  // Compilation is an album-level property edited via AlbumTagEditor, not
  // here — this is read-only, just so a compilation's Album Artist shows
  // the same "Various Artists" pill here as it does there instead of an
  // editable field that would suggest a per-track override is meaningful.
  let compilation = $state(false);
  let artEmbedded = $state(false);
  let coverArtVersion = $state(0);
  let showClearArtConfirm = $state(false);
  let isClearingArt = $state(false);
  let showTagCleanup = $state(false);

  let titlesort = $state("");
  let artistsort = $state("");
  let albumsort = $state("");
  let albumArtistSort = $state("");
  let composersort = $state("");
  let genresort = $state("");

  let tagHistory = $state<SongTagHistory | null>(null);
  let isClearingHistory = $state(false);
  let isRestoring = $state(false);

  let isLoading = $state(false);
  let isSaving = $state(false);
  let errorMsg = $state("");

  interface SongDetailsPayload {
    id: number;
    path: string;
    title: string;
    titlesort: string | null;
    artist: string;
    artistsort: string | null;
    album: string;
    albumsort: string | null;
    album_artist: string;
    album_artist_sort: string | null;
    composer: string;
    composersort: string | null;
    genre: string;
    genresort: string | null;
    track: number | null;
    disc: number | null;
    disc_subtitle: string | null;
    year: number | null;
    originalyear: number | null;
    grouping: string;
    bpm: number | null;
    initial_key: string;
    rating: number;
    loved: number;
    compilation: boolean;
    art_embedded: boolean;
    is_cue_track: boolean;
  }

  async function loadTagHistory(id: number) {
    try {
      const history = await invoke<SongTagHistory>("get_song_tag_history", { songId: id });
      tagHistory = history ?? null;
    } catch (e) {
      console.warn("Failed to load tag history:", e);
      tagHistory = null;
    }
  }

  async function loadMetadata() {
    isLoading = true;
    errorMsg = "";
    try {
      const details = songId != null
        ? await invoke<SongDetailsPayload>("get_song_details", { songId })
        : await invoke<SongDetailsPayload>("get_file_song_details", { path: filePath });

      title = details.title;
      titlesort = details.titlesort ?? "";
      artist = details.artist;
      artistsort = details.artistsort ?? "";
      album = details.album;
      albumsort = details.albumsort ?? "";
      albumArtist = details.album_artist;
      albumArtistSort = details.album_artist_sort ?? "";
      composer = details.composer;
      composersort = details.composersort ?? "";
      genre = details.genre;
      genresort = details.genresort ?? "";
      track = details.track;
      disc = details.disc;
      discSubtitle = details.disc_subtitle ?? "";
      year = details.year;
      originalYear = details.originalyear;
      grouping = details.grouping;
      bpm = details.bpm;
      initialKey = details.initial_key;
      path = details.path;
      rating = details.rating;
      loved = details.loved;
      compilation = details.compilation;
      artEmbedded = details.art_embedded;
      isCueTrack = details.is_cue_track;

      if (songId != null && !isRemotePath(details.path) && !details.is_cue_track) {
        await loadTagHistory(songId);
      } else {
        tagHistory = null;
      }
    } catch (e: any) {
      console.error("Failed to load metadata:", e);
      errorMsg = e.toString();
    } finally {
      isLoading = false;
    }
  }

  async function handleClearHistory() {
    if (songId == null) return;
    isClearingHistory = true;
    try {
      await invoke("clear_song_tag_history", { songId });
      if (tagHistory) {
        tagHistory = { ...tagHistory, entries: [] };
      }
    } catch (e: any) {
      console.error("Failed to clear tag history:", e);
      toastStore.show(e.toString(), "error");
    } finally {
      isClearingHistory = false;
    }
  }

  function getFieldLabel(field: string): string {
    switch (field) {
      case "title": return i18n.t("tagEditor.titleField");
      case "titlesort": return i18n.t("sortOverrides.title");
      case "artist": return i18n.t("tagEditor.artistField");
      case "artistsort": return i18n.t("sortOverrides.artist");
      case "album": return i18n.t("tagEditor.albumField");
      case "albumsort": return i18n.t("sortOverrides.album");
      case "album_artist": return i18n.t("tagEditor.albumArtistField");
      case "album_artist_sort": return i18n.t("sortOverrides.albumArtist");
      case "composer": return i18n.t("tagEditor.composerField");
      case "composersort": return i18n.t("sortOverrides.composer");
      case "genre": return i18n.t("tagEditor.genreField");
      case "genresort": return i18n.t("sortOverrides.genre");
      case "track": return i18n.t("tagEditor.trackField");
      case "disc": return i18n.t("tagEditor.discField");
      case "disc_subtitle": return i18n.t("tagEditor.discSubtitleField");
      case "year": return i18n.t("tagEditor.yearField");
      case "originalyear": return i18n.t("tagEditor.originalYearField");
      case "grouping": return i18n.t("tagEditor.groupingField");
      case "bpm": return i18n.t("tagEditor.bpmField");
      case "initial_key": return i18n.t("tagEditor.initialKeyField");
      case "compilation": return i18n.t("tagEditor.compilationField");
      case "comment": return i18n.t("tagEditor.commentField");
      default: return field;
    }
  }

  function formatHistoryValue(val: string | null | undefined): string {
    if (val === null || val === undefined || val === "") {
      return i18n.t("tagEditor.historyEmptyValue");
    }
    return val;
  }

  function getCurrentFieldValue(field: string): string | null {
    switch (field) {
      case "title": return title.trim() || null;
      case "titlesort": return titlesort.trim() || null;
      case "artist": return artist.trim() || null;
      case "artistsort": return artistsort.trim() || null;
      case "album": return album.trim() || null;
      case "albumsort": return albumsort.trim() || null;
      case "album_artist": return albumArtist.trim() || null;
      case "album_artist_sort": return albumArtistSort.trim() || null;
      case "composer": return composer.trim() || null;
      case "composersort": return composersort.trim() || null;
      case "genre": return genre.trim() || null;
      case "genresort": return genresort.trim() || null;
      case "track": return track != null ? String(track) : null;
      case "disc": return disc != null ? String(disc) : null;
      case "disc_subtitle": return discSubtitle.trim() || null;
      case "year": return year != null ? String(year) : null;
      case "originalyear": return originalYear != null ? String(originalYear) : null;
      case "grouping": return grouping.trim() || null;
      case "bpm": return bpm != null ? String(bpm) : null;
      case "initial_key": return initialKey.trim() || null;
      case "compilation": return compilation ? "true" : "false";
      case "comment": return null;
      default: return null;
    }
  }

  function isValueCurrent(field: string, targetValue: string | null | undefined): boolean {
    const current = getCurrentFieldValue(field);
    const normTarget = (targetValue ?? "").trim();
    const normCurrent = (current ?? "").trim();
    if (!normTarget && !normCurrent) return true;
    if (["artist", "album_artist", "composer", "genre"].includes(field)) {
      const splitNorm = (s: string) =>
        s.split(";").map((p) => p.trim()).filter(Boolean).sort().join("; ");
      return splitNorm(normCurrent).toLowerCase() === splitNorm(normTarget).toLowerCase();
    }
    return normCurrent.toLowerCase() === normTarget.toLowerCase();
  }

  function hasChangedSince(entry: SongHistoryEntry): boolean {
    const current = getCurrentFieldValue(entry.field);
    const normNew = (entry.newValue ?? "").trim();
    const normCurrent = (current ?? "").trim();
    if (["artist", "album_artist", "composer", "genre"].includes(entry.field)) {
      const splitNorm = (s: string) =>
        s.split(";").map((p) => p.trim()).filter(Boolean).sort().join("; ");
      return splitNorm(normCurrent).toLowerCase() !== splitNorm(normNew).toLowerCase();
    }
    return normCurrent.toLowerCase() !== normNew.toLowerCase();
  }

  async function handleRestoreEntry(entry: SongHistoryEntry) {
    if (isRestoring || isSaving || songId == null) return;
    isRestoring = true;
    try {
      const result = await invoke<RestoreTagHistoryResult>("restore_tag_history", {
        target: { type: "entry", historyId: entry.id }
      });

      if (result.restoredTracks > 0) {
        toastStore.show(
          i18n.t("tagEditor.restoreSuccess", {
            field: getFieldLabel(entry.field),
            value: formatHistoryValue(entry.oldValue)
          }),
          "success"
        );
        await loadMetadata();
        await collectionStore.refreshStats();
        await collectionStore.refreshLibrary();
        if (playlistsStore.activePlaylistId != null) {
          await playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
        }
        await playerStore.refreshPlaybackState();
      } else if (result.unchangedTracks > 0) {
        toastStore.show(i18n.t("tagEditor.restoreNoOp"), "info");
      } else if (result.skippedTracks > 0) {
        toastStore.show(
          result.errors.length > 0 ? result.errors[0] : i18n.t("tagEditor.restoreFailed"),
          "error"
        );
      }
    } catch (e: any) {
      console.error("Failed to restore tag history entry:", e);
      toastStore.show(e.toString(), "error");
    } finally {
      isRestoring = false;
    }
  }

  async function handleRestoreBatch(batchId: string) {
    if (isRestoring || isSaving || songId == null) return;
    isRestoring = true;
    try {
      const result = await invoke<RestoreTagHistoryResult>("restore_tag_history", {
        target: { type: "batch", batchId }
      });

      if (result.restoredTracks > 0) {
        if (result.skippedTracks > 0) {
          toastStore.show(
            i18n.t("tagEditor.restoreBatchPartial", {
              restored: result.restoredTracks,
              skipped: result.skippedTracks
            }),
            "warning"
          );
        } else {
          toastStore.show(
            i18n.t("tagEditor.restoreBatchSuccess", { count: result.restoredTracks }),
            "success"
          );
        }
        await loadMetadata();
        await collectionStore.refreshStats();
        await collectionStore.refreshLibrary();
        if (playlistsStore.activePlaylistId != null) {
          await playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
        }
        await playerStore.refreshPlaybackState();
      } else if (result.unchangedTracks > 0 && result.skippedTracks === 0) {
        toastStore.show(i18n.t("tagEditor.restoreNoOp"), "info");
      } else if (result.skippedTracks > 0) {
        toastStore.show(
          result.errors.length > 0 ? result.errors.join("; ") : i18n.t("tagEditor.restoreFailed"),
          "error"
        );
      }
    } catch (e: any) {
      console.error("Failed to restore tag history batch:", e);
      toastStore.show(e.toString(), "error");
    } finally {
      isRestoring = false;
    }
  }

  async function handleSave() {
    isSaving = true;
    try {
      if (songId != null) {
        await invoke("save_song_tags", {
          songId,
          title,
          titlesort: titlesort.trim() || null,
          artist,
          artistsort: artistsort.trim() || null,
          album,
          albumsort: albumsort.trim() || null,
          albumArtist,
          albumArtistSort: albumArtistSort.trim() || null,
          composer,
          composersort: composersort.trim() || null,
          genre,
          genresort: genresort.trim() || null,
          track,
          disc,
          discSubtitle,
          year,
          originalyear: originalYear,
          grouping,
          bpm,
          initialKey,
        });

        await collectionStore.refreshStats();
        await collectionStore.refreshLibrary();

        if (playlistsStore.activePlaylistId !== null && playlistsStore.activePlaylistId !== undefined) {
          await playlistsStore.selectPlaylist(playlistsStore.activePlaylistId);
        }
        await playerStore.refreshPlaybackState();
      } else if (filePath) {
        await invoke("save_file_tags", {
          path: filePath,
          title,
          titlesort: titlesort.trim() || null,
          artist,
          artistsort: artistsort.trim() || null,
          album,
          albumsort: albumsort.trim() || null,
          albumArtist,
          albumArtistSort: albumArtistSort.trim() || null,
          composer,
          composersort: composersort.trim() || null,
          genre,
          genresort: genresort.trim() || null,
          track,
          disc,
          discSubtitle,
          year,
          originalyear: originalYear,
          grouping,
          bpm,
          initialKey,
        });

        if (source && folderPath) {
          await scanFolder(source, folderPath);
          const resolvedFolder = folderName || (folderPath.length > 0 ? folderPath[folderPath.length - 1] : "");
          const resolvedFile = fileName || title || (path ? path.split(/[\/\\]/).pop() : "") || "file";
          toastStore.show(
            i18n.t("files.savedAndScanning", { folder: resolvedFolder, file: resolvedFile }),
            "success"
          );
        }
        filesStore.reloadFolder();
      }

      if (onSave) onSave();
      onClose();
    } catch (e: any) {
      console.error("Failed to save tags:", e);
      toastStore.show(i18n.t('tagEditor.saveFailedPrefix') + e.toString(), "error");
    } finally {
      isSaving = false;
    }
  }

  // Rating lives in the library database only (never written to the file),
  // so it saves immediately rather than waiting for the Save button.
  async function handleRate(value: number) {
    if (songId == null) return;
    try {
      rating = await invoke<number>("set_song_rating", { songId, rating: value });
    } catch (e) {
      console.error("Failed to save rating:", e);
    }
  }

  async function handleSetLoved(value: number) {
    if (songId == null) return;
    try {
      loved = await invoke<number>("set_song_loved", { songId, loved: value });
    } catch (e) {
      console.error("Failed to save loved:", e);
    }
  }

  let isOpeningFolder = $state(false);

  async function handleOpenFolder() {
    isOpeningFolder = true;
    try {
      if (songId != null) {
        await invoke("open_song_folder", { songIds: [songId] });
      }
    } catch (e: any) {
      console.error("Failed to open containing folder:", e);
      toastStore.show(i18n.t('tagEditor.openFolderFailedPrefix', {}, 'Failed to open folder: ') + e.toString(), "error");
    } finally {
      isOpeningFolder = false;
    }
  }

  async function handleClearArt() {
    if (songId == null) return;
    isClearingArt = true;
    try {
      await invoke("clear_song_cover_art", { songId });
      artEmbedded = false;
      coverArtVersion++;
      await collectionStore.refreshLibrary();
      toastStore.show(i18n.t('tagEditor.clearArtSuccess'), "success");
    } catch (e: any) {
      console.error("Failed to clear embedded artwork:", e);
      toastStore.show(i18n.t('tagEditor.clearArtFailedPrefix') + e.toString(), "error");
    } finally {
      isClearingArt = false;
      showClearArtConfirm = false;
    }
  }

  onMount(loadMetadata);

  function handleKeydown(e: KeyboardEvent) {
    // Keys pressed in the clean-up dialog stacked on top belong to it.
    if (showTagCleanup) return;
    if (e.key === "Enter") {
      const target = e.target as HTMLElement;
      if (target.tagName === "BUTTON" || target.tagName === "TEXTAREA") return;
      if (isSaving || isLoading) return;
      e.preventDefault();
      handleSave();
    }
  }
</script>

<Modal onClose={() => { if (!showTagCleanup) onClose(); }} onKeydown={handleKeydown}>
    <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
      <div class="flex items-center gap-2">
        <Sliders class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold">{i18n.t('tagEditor.title')}</h3>
      </div>
      <button onclick={onClose} disabled={isSaving} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50">
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-200px)]">
      {#if isLoading}
        <div class="w-full py-16 flex flex-col items-center justify-center gap-3">
          <LoadingSpinner label={i18n.t('tagEditor.readingTags')} size="sm" />
        </div>
      {:else if errorMsg}
        <div class="w-full py-12 flex flex-col items-center justify-center gap-3 text-center">
          <AlertTriangle class="w-8 h-8 text-red-500" />
          <p class="text-sm font-semibold text-red-400">{i18n.t('tagEditor.readFailed')}</p>
          <p class="text-xs text-brand-text-secondary/65 max-w-xs">{errorMsg}</p>
        </div>
      {:else}
        <div class="flex flex-col gap-4">
          <div class="flex items-center gap-3">
            <div class="flex-1 flex flex-col gap-1 min-w-0">
              <span class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">{i18n.t('tagEditor.locationField')}</span>
              <span class="text-xs text-brand-text-secondary break-all select-text">{path}</span>
            </div>
            {#if !isRemoteSource}
              <Button
                onclick={handleOpenFolder}
                disabled={isSaving || isOpeningFolder}
                variant="secondary"
                size="sm"
              >
                <FolderOpen class="w-3.5 h-3.5" />
                {i18n.t('tagEditor.openFolderBtn', {}, 'Open Folder')}
              </Button>
            {/if}
          </div>

          {#if isRemoteSource}
            <div class="flex items-start gap-2.5 bg-brand-main border border-brand-border rounded-lg p-2.5 text-brand-text-secondary text-xs">
              <CloudIcon class="w-4 h-4 shrink-0 mt-0.5" />
              <span>{i18n.t('tagEditor.remoteSourceNote')}</span>
            </div>
          {/if}

          {#if isCueTrack}
            <div class="flex items-start gap-2.5 bg-brand-main border border-brand-border rounded-lg p-2.5 text-brand-text-secondary text-xs">
              <Lock class="w-4 h-4 shrink-0 mt-0.5" />
              <span>{i18n.t('tagEditor.cueTrackNote')}</span>
            </div>
          {/if}

          <div class="flex items-center gap-3">
            {#key coverArtVersion}
              <CoverArt {songId} sizeClass="w-12 h-12 rounded" />
            {/key}
            <div class="flex-1 flex flex-col gap-1 min-w-0">
              <span class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">{i18n.t('tagEditor.artworkField')}</span>
              <span class="text-xs text-brand-text-secondary">
                {artEmbedded ? i18n.t('tagEditor.artworkEmbedded') : i18n.t('tagEditor.artworkNotEmbedded')}
              </span>
            </div>
            {#if songId != null && !isRemoteSource}
              <Button
                onclick={() => { showClearArtConfirm = true; }}
                disabled={!artEmbedded || isSaving || isClearingArt}
                variant="secondary"
                size="sm"
              >
                <ImageOff class="w-3.5 h-3.5" />
                {i18n.t('tagEditor.clearArtBtn')}
              </Button>
            {/if}
          </div>

          <!-- Grid form: field order mirrors the collection table's column order (track, title, artist,
               album, composer, album artist, year, genre, grouping, bpm, initial key), with Disc paired
               alongside Track (Disc first) and Disc Subtitle under them, since neither has a column
               of its own in the table. -->
          <div class="grid grid-cols-2 gap-4">
            <FormField label={i18n.t('tagEditor.discField')} for="tag-disc">
              <Input
                id="tag-disc"
                type="number"
                value={disc ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseInt(e.currentTarget.value, 10);
                  disc = isNaN(val) ? null : val;
                }}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.trackField')} for="tag-track">
              <Input
                id="tag-track"
                type="number"
                value={track ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseInt(e.currentTarget.value, 10);
                  track = isNaN(val) ? null : val;
                }}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.discSubtitleField')} for="tag-disc-subtitle" span2>
              <Input
                id="tag-disc-subtitle"
                bind:value={discSubtitle}
                disabled={isSaving}
                placeholder={i18n.t('tagEditor.discSubtitlePlaceholder')}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.titleField')} for="tag-title" span2>
              <Input
                id="tag-title"
                bind:value={title}
                disabled={isSaving}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.artistField')} for="tag-artist">
              <ChipInput
                id="tag-artist"
                bind:value={artist}
                disabled={isSaving}
                placeholder={i18n.t('tagEditor.artistPlaceholder')}
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.albumField')} for="tag-album">
              <Input
                id="tag-album"
                bind:value={album}
                disabled={isSaving}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.composerField')} for="tag-composer">
              <ChipInput
                id="tag-composer"
                bind:value={composer}
                disabled={isSaving}
                placeholder={i18n.t('tagEditor.composerPlaceholder')}
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.albumArtistField')} for="tag-albumartist" tooltip={i18n.t('tagEditor.albumArtistTooltip')}>
              {#if compilation}
                <div class="flex items-center h-9">
                  <span class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-brand-border bg-brand-main text-xs font-semibold text-brand-text-primary">
                    {albumArtist}
                    <Lock class="w-3 h-3 text-brand-text-secondary shrink-0" />
                  </span>
                </div>
              {:else}
                <ChipInput
                  id="tag-albumartist"
                  bind:value={albumArtist}
                  disabled={isSaving}
                  placeholder={i18n.t('tagEditor.albumArtistPlaceholder')}
                  class="w-full"
                />
              {/if}
            </FormField>

            <FormField label={i18n.t('tagEditor.yearField')} for="tag-year">
              <Input
                id="tag-year"
                type="number"
                value={year ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseInt(e.currentTarget.value, 10);
                  year = isNaN(val) ? null : val;
                }}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.originalYearField')} for="tag-originalyear" tooltip={i18n.t('tagEditor.originalYearTooltip')}>
              <Input
                id="tag-originalyear"
                type="number"
                value={originalYear ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseInt(e.currentTarget.value, 10);
                  originalYear = isNaN(val) ? null : val;
                }}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.genreField')} for="tag-genre" span2 tooltip={i18n.t('tagEditor.genreTooltip', {}, 'The first value is treated as the main genre in the Genres tab, the rest as subgenres of it.')}>
              <PlainChipInput
                id="tag-genre"
                bind:value={genre}
                disabled={isSaving}
                placeholder={i18n.t('tagEditor.genrePlaceholder')}
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.groupingField')} for="tag-grouping" tooltip={i18n.t('tagEditor.groupingTooltip')}>
              <Input id="tag-grouping" bind:value={grouping} disabled={isSaving} size="sm" class="w-full" />
            </FormField>

            <FormField label={i18n.t('tagEditor.bpmField')} for="tag-bpm" tooltip={i18n.t('tagEditor.bpmTooltip')}>
              <Input
                id="tag-bpm"
                type="number"
                value={bpm ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseFloat(e.currentTarget.value);
                  bpm = isNaN(val) ? null : val;
                }}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.initialKeyField')} for="tag-initialkey" tooltip={i18n.t('tagEditor.initialKeyTooltip')}>
              <Input id="tag-initialkey" bind:value={initialKey} disabled={isSaving} size="sm" class="w-full" />
            </FormField>

            <!-- Rating (library-only, saves immediately) -->
            {#if songId != null}
              <div class="flex flex-col gap-1.5">
                <span class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">{i18n.t('rating.label')}</span>
                <SongRating {rating} {loved} onRate={handleRate} onSetLoved={handleSetLoved} size="md" />
              </div>
            {/if}

            <!-- Sort Overrides ("Sort As") -->
            <details class="col-span-2 group border border-brand-border rounded-lg bg-brand-sidebar/40 overflow-hidden mt-1">
              <summary class="flex items-center justify-between px-3 py-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors">
                <span>{i18n.t("sortOverrides.heading")}</span>
                <span class="text-[10px] text-brand-text-secondary/70 group-open:rotate-180 transition-transform">▼</span>
              </summary>
              <div class="p-3 pt-2 grid grid-cols-2 gap-3 border-t border-brand-border/60">
                <FormField label={i18n.t("sortOverrides.title")} for="tag-titlesort">
                  <Input id="tag-titlesort" bind:value={titlesort} disabled={isSaving} size="sm" placeholder={i18n.t("sortOverrides.example")} class="w-full" />
                </FormField>

                <FormField label={i18n.t("sortOverrides.artist")} for="tag-artistsort">
                  <Input id="tag-artistsort" bind:value={artistsort} disabled={isSaving} size="sm" placeholder={i18n.t("sortOverrides.example")} class="w-full" />
                </FormField>

                <FormField label={i18n.t("sortOverrides.album")} for="tag-albumsort">
                  <Input id="tag-albumsort" bind:value={albumsort} disabled={isSaving} size="sm" class="w-full" />
                </FormField>

                <FormField label={i18n.t("sortOverrides.albumArtist")} for="tag-albumartistsort">
                  <Input id="tag-albumartistsort" bind:value={albumArtistSort} disabled={isSaving} size="sm" class="w-full" />
                </FormField>

                <FormField label={i18n.t("sortOverrides.composer")} for="tag-composersort">
                  <Input id="tag-composersort" bind:value={composersort} disabled={isSaving} size="sm" class="w-full" />
                </FormField>

                <FormField label={i18n.t("sortOverrides.genre")} for="tag-genresort">
                  <Input id="tag-genresort" bind:value={genresort} disabled={isSaving} size="sm" class="w-full" />
                </FormField>
              </div>
            </details>

            <!-- Tag History -->
            {#if songId != null && !isRemoteSource && !isCueTrack && tagHistory !== null}
              <details class="col-span-2 group border border-brand-border rounded-lg bg-brand-sidebar/40 overflow-hidden mt-1">
                <summary class="flex items-center justify-between px-3 py-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors">
                  <div class="flex items-center gap-2">
                    <span>{i18n.t("tagEditor.historyHeading")}</span>
                    {#if tagHistory.entries.length > 0}
                      <span class="px-1.5 py-0.5 text-[10px] rounded-full bg-brand-surface text-brand-text-secondary font-normal">
                        {tagHistory.entries.length}
                      </span>
                    {/if}
                  </div>
                  <span class="text-[10px] text-brand-text-secondary/70 group-open:rotate-180 transition-transform">▼</span>
                </summary>
                <div class="p-3 pt-2 space-y-3 border-t border-brand-border/60 text-xs">
                  <!-- Source Block -->
                  <div class="space-y-1.5">
                    <div class="font-semibold text-brand-text-primary text-[11px] uppercase tracking-wider">
                      {i18n.t("tagEditor.historySourceHeading")}
                    </div>
                    <div class="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs bg-brand-surface/50 p-2.5 rounded border border-brand-border/40">
                      <div>
                        <span class="text-brand-text-secondary">{i18n.t("tagEditor.historyImported")}:</span>
                        <span class="ml-1 text-brand-text-primary">{tagHistory.source.added ? formatDate(tagHistory.source.added) : "—"}</span>
                      </div>
                      {#if tagHistory.source.format}
                        <div>
                          <span class="text-brand-text-secondary">{i18n.t("tagEditor.historyFormat")}:</span>
                          <span class="ml-1 text-brand-text-primary">{tagHistory.source.format}</span>
                        </div>
                      {/if}
                      <div>
                        <span class="text-brand-text-secondary">{i18n.t("tagEditor.historyFileSize")}:</span>
                        <span class="ml-1 text-brand-text-primary">{tagHistory.source.filesize != null ? formatFileSize(tagHistory.source.filesize) : "—"}</span>
                      </div>
                      <div class="col-span-2 break-all">
                        <span class="text-brand-text-secondary">{i18n.t("tagEditor.locationField")}:</span>
                        <span class="ml-1 text-brand-text-primary font-mono text-[11px]">{tagHistory.source.path}</span>
                      </div>
                    </div>
                  </div>

                  <!-- Changes Header, Clear Action & Entries or Empty State -->
                  {#if tagHistory.entries.length === 0}
                    <div class="pt-2 border-t border-brand-border/40 text-brand-text-secondary text-xs italic">
                      {tagHistory.source.added
                        ? i18n.t("tagEditor.historyEmptySince", { date: formatDate(tagHistory.source.added) })
                        : i18n.t("tagEditor.historyEmpty")}
                    </div>
                  {:else}
                    <div class="space-y-2 pt-2 border-t border-brand-border/40">
                      <div class="flex items-center justify-between">
                        <div class="flex items-center gap-1.5 text-brand-text-secondary text-xs">
                          <HelpTip
                            text={i18n.t("tagEditor.historyTimestampHint")}
                            label={i18n.t("tagEditor.historyTimestampHint")}
                          />
                        </div>
                        <Button
                          variant="secondary"
                          size="sm"
                          disabled={isClearingHistory || isSaving || isRestoring}
                          onclick={handleClearHistory}
                          class="text-xs h-7 px-2.5"
                        >
                          {i18n.t("tagEditor.clearHistoryBtn")}
                        </Button>
                      </div>

                      <div class="space-y-2 max-h-60 overflow-y-auto pr-1">
                        {#each tagHistory.entries as entry (entry.id)}
                          <div class="p-2 rounded bg-brand-surface/40 border border-brand-border/40 text-xs flex flex-col gap-1.5">
                            <div class="flex items-center justify-between text-brand-text-secondary text-[11px]">
                              <div class="flex items-center gap-1.5 flex-wrap">
                                <span>{entry.actor === "scan" ? i18n.t("tagEditor.historyActorScan") : i18n.t("tagEditor.historyActorLuminous")}</span>
                                {#if entry.batchOtherTracks > 0}
                                  <span class="text-brand-text-secondary/60">•</span>
                                  <span>{i18n.t("tagEditor.historyBatchNote", { count: entry.batchOtherTracks })}</span>
                                {/if}
                              </div>
                              <div class="flex items-center gap-2 shrink-0">
                                <span class="text-brand-text-secondary">{formatDate(entry.at)}</span>
                                {#if entry.batchId}
                                  <Button
                                    variant="secondary"
                                    size="sm"
                                    disabled={isRestoring || isSaving}
                                    onclick={() => handleRestoreBatch(entry.batchId!)}
                                    class="text-[10px] h-5 px-1.5 py-0"
                                    title={i18n.t("tagEditor.restoreBatchTooltip")}
                                  >
                                    {i18n.t("tagEditor.restoreBatchBtn")}
                                  </Button>
                                {/if}
                              </div>
                            </div>
                            <div class="flex items-center justify-between gap-2">
                              <div class="flex items-center gap-1.5 text-brand-text-primary break-all flex-wrap">
                                <span class="font-medium text-brand-text-primary">{getFieldLabel(entry.field)}:</span>
                                <span class="text-brand-text-secondary line-through opacity-80">{formatHistoryValue(entry.oldValue)}</span>
                                <span class="text-brand-text-secondary/70">→</span>
                                <span class="font-medium">{formatHistoryValue(entry.newValue)}</span>
                                {#if hasChangedSince(entry)}
                                  <span class="text-[10px] text-brand-accent px-1.5 py-0.2 rounded bg-brand-accent/10 border border-brand-accent/20">
                                    {i18n.t("tagEditor.currentValuePrefix")}: {formatHistoryValue(getCurrentFieldValue(entry.field))}
                                  </span>
                                {/if}
                              </div>
                              <Button
                                variant="secondary"
                                size="sm"
                                disabled={isRestoring || isSaving || isValueCurrent(entry.field, entry.oldValue)}
                                onclick={() => handleRestoreEntry(entry)}
                                class="text-xs h-6 px-2 shrink-0"
                                title={isValueCurrent(entry.field, entry.oldValue) ? i18n.t("tagEditor.restoreAlreadyCurrent") : undefined}
                              >
                                {i18n.t("tagEditor.restoreAction")}
                              </Button>
                            </div>
                          </div>
                        {/each}
                      </div>
                    </div>
                  {/if}
                </div>
              </details>
            {/if}
          </div>
        </div>
      {/if}
    </div>

    <div class="h-16 flex items-center justify-between px-6 border-t border-brand-border shrink-0 bg-brand-main">
      <div>
        {#if songId != null && !isRemoteSource && !isCueTrack}
          <Button
            onclick={() => { showTagCleanup = true; }}
            disabled={isLoading || !!errorMsg || isSaving || isRestoring}
            variant="secondary"
            size="sm"
          >
            <Broom class="w-3.5 h-3.5" />
            {i18n.t('tagCleanup.action')}
          </Button>
        {/if}
      </div>

      <div class="flex items-center gap-2">
        <Button onclick={onClose} disabled={isSaving || isRestoring} variant="secondary" size="sm">
          {i18n.t('tagEditor.cancelBtn')}
        </Button>
        <Button onclick={handleSave} disabled={isLoading || !!errorMsg || isSaving || isRestoring || isCueTrack} variant="primary" size="sm">
          {#if isSaving}
            <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
            {i18n.t('tagEditor.updatingTags')}
          {:else}
            <Save class="w-3.5 h-3.5" />
            {i18n.t('tagEditor.saveBtn')}
          {/if}
        </Button>
      </div>
    </div>
</Modal>

{#if showTagCleanup && songId != null}
  <TagCleanupDialog
    songIds={[songId]}
    onClose={() => {
      showTagCleanup = false;
      if (songId != null && !isRemoteSource && !isCueTrack) void loadTagHistory(songId);
    }}
  />
{/if}

{#if showClearArtConfirm}
  <ConfirmDialog
    title={i18n.t('tagEditor.clearArtConfirmTitle')}
    message={i18n.t('tagEditor.clearArtConfirmMessage')}
    confirmLabel={i18n.t('tagEditor.clearArtBtn')}
    cancelLabel={i18n.t('tagEditor.cancelBtn')}
    onConfirm={handleClearArt}
    onCancel={() => { showClearArtConfirm = false; }}
  />
{/if}
