<script lang="ts">
  import { isRemoteSource as isRemoteSourceOf } from "../utils/remoteSource";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import {
    SlidersIcon as Sliders,
    FloppyDiskIcon as Save,
    XIcon as X,
    CircleNotchIcon as LoaderCircle,
    StackIcon as Layers,
    LockIcon as Lock,
    ImageBrokenIcon as ImageOff,
    CloudIcon
  } from "phosphor-svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { tasksStore } from "../stores/tasks.svelte";
  import FormField from "./FormField.svelte";
  import Modal from "./Modal.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import ChipInput from "./ChipInput.svelte";
  import CoverArt from "./CoverArt.svelte";
  import type { FileSource, Song } from "../types";
  import { filesStore } from "../stores/files.svelte";
  import { scanFolder } from "./files/folderActions";
  import { MixedField } from "../utils/mixedField.svelte";
  import DiscFields from "./DiscFields.svelte";

  interface Props {
    songIds?: number[];
    filePaths?: string[];
    title?: string;
    source?: FileSource;
    folderPath?: string[];
    folderName?: string;
    initialAlbum?: string | null;
    initialAlbumSort?: string | null;
    initialAlbumArtist?: string | null;
    initialAlbumArtistSort?: string | null;
    initialGenre?: string | null;
    initialGenreSort?: string | null;
    initialYear?: number | null;
    /** The edited tracks' disc numbers and subtitles, when the caller already
        has them; otherwise they're read from the tracks themselves. */
    initialTracks?: Pick<Song, "disc" | "disc_subtitle">[];
    initialCompilation?: boolean;
    hasEmbeddedArt?: boolean;
    /** Album-level art overrides (see AlbumItem.art_automatic/art_manual) — take
        precedence over any single track's embedded art, same as CoverArt.svelte's
        own precedence when both are passed through. */
    initialArtAutomatic?: string | null;
    initialArtManual?: string | null;
    onClose: () => void;
    onSave?: () => void;
  }

  let {
    songIds = [],
    filePaths = [],
    title = "",
    source,
    folderPath,
    folderName,
    initialAlbum = "",
    initialAlbumSort = "",
    initialAlbumArtist = "",
    initialAlbumArtistSort = "",
    initialGenre = "",
    initialGenreSort = "",
    initialYear = null,
    initialTracks = [],
    initialCompilation = false,
    hasEmbeddedArt = false,
    initialArtAutomatic = null,
    initialArtManual = null,
    onClose,
    onSave
  }: Props = $props();

  const tracksCount = $derived((songIds?.length ?? 0) + (filePaths?.length ?? 0));

  // svelte-ignore state_referenced_locally
  let album = $state(initialAlbum ?? "");
  // svelte-ignore state_referenced_locally
  let albumArtist = $state(initialAlbumArtist ?? "");
  // svelte-ignore state_referenced_locally
  let genre = $state(initialGenre ?? "");
  // svelte-ignore state_referenced_locally
  let year = $state<number | null>(initialYear);
  // Disc and Disc Subtitle differ per track on a multi-disc album.
  const disc = new MixedField<number>();
  const discSubtitle = new MixedField<string>();
  // svelte-ignore state_referenced_locally
  disc.reset(initialTracks.map((t) => t.disc));
  // svelte-ignore state_referenced_locally
  discSubtitle.reset(initialTracks.map((t) => t.disc_subtitle));
  // svelte-ignore state_referenced_locally
  let compilation = $state(initialCompilation ?? false);
  // svelte-ignore state_referenced_locally
  let hasEmbeddedArtState = $state(hasEmbeddedArt);

  // svelte-ignore state_referenced_locally
  let albumsort = $state(initialAlbumSort ?? "");
  // svelte-ignore state_referenced_locally
  let albumArtistSort = $state(initialAlbumArtistSort ?? "");
  // svelte-ignore state_referenced_locally
  let genresort = $state(initialGenreSort ?? "");

  // Remote songs (WebDAV #682, OpenSubsonic #916) have no local file Luminous can write lofty tags to,
  // and there's no write-back to the remote server -- edits here only ever
  // reach Luminous's own DB. A representative track's path is enough since
  // an album's tracks all share one source.
  let isRemoteSource = $derived.by(() => {
    if (songIds.length > 0) {
      const sample = collectionStore.songs.find((s) => s.id === songIds[0]);
      return isRemoteSourceOf(sample);
    }
    return false;
  });

  function consensusString(list: (string | null | undefined)[]): string {
    const nonBlank = list.map((s) => s?.trim()).filter((s): s is string => !!s);
    if (nonBlank.length === 0) return "";
    const first = nonBlank[0];
    return nonBlank.every((s) => s === first) ? first : "";
  }

  function consensusNumber(list: (number | null | undefined)[]): number | null {
    const present = list.filter((n): n is number => n !== null && n !== undefined && n > 0);
    if (present.length === 0) return null;
    const first = present[0];
    return present.every((n) => n === first) ? first : null;
  }

  onMount(async () => {
    // Best-effort preload for the genre field's autocomplete — a failure
    // here shouldn't block or break the editor itself.
    if (!tagsStore.loaded) tagsStore.load().catch(() => {});
    if (!initialAlbum && !initialAlbumArtist) {
      try {
        let librarySongs: Song[] = [];
        if (songIds.length > 0) {
          const inStore = songIds
            .map((id) => collectionStore.songs.find((s) => s.id === id))
            .filter((s): s is Song => !!s);
          if (inStore.length === songIds.length) {
            librarySongs = inStore;
          } else {
            librarySongs = (await invoke<Song[]>("get_songs_by_ids", { songIds })) ?? [];
          }
        }

        const fileSongDetails: Array<{
          album?: string;
          albumsort?: string | null;
          album_artist?: string;
          album_artist_sort?: string | null;
          genre?: string;
          genresort?: string | null;
          year?: number | null;
          disc?: number | null;
          disc_subtitle?: string | null;
          compilation?: boolean;
          art_embedded?: boolean;
        }> = [];

        for (const p of filePaths) {
          try {
            const details = await invoke<{
              album: string;
              albumsort: string | null;
              album_artist: string;
              album_artist_sort: string | null;
              genre: string;
              genresort: string | null;
              year: number | null;
              disc: number | null;
              disc_subtitle: string | null;
              compilation: boolean;
              art_embedded: boolean;
            }>("get_file_song_details", { path: p });
            if (details) fileSongDetails.push(details);
          } catch (err) {
            console.error("Failed to get file tags:", err);
          }
        }

        const allItems = [
          ...librarySongs.map((s) => ({
            album: s.album,
            albumsort: s.albumsort,
            album_artist: s.album_artist || s.artist,
            album_artist_sort: s.album_artist_sort,
            genre: s.genre,
            genresort: s.genresort,
            year: s.year,
            disc: s.disc,
            disc_subtitle: s.disc_subtitle,
            compilation: s.compilation,
            art_embedded: s.art_embedded,
          })),
          ...fileSongDetails.map((d) => ({
            album: d.album,
            albumsort: d.albumsort,
            album_artist: d.album_artist,
            album_artist_sort: d.album_artist_sort,
            genre: d.genre,
            genresort: d.genresort,
            year: d.year,
            disc: d.disc,
            disc_subtitle: d.disc_subtitle,
            compilation: d.compilation,
            art_embedded: d.art_embedded,
          })),
        ];

        if (allItems.length > 0) {
          const commonAlbum = consensusString(allItems.map((i) => i.album));
          if (commonAlbum) album = commonAlbum;

          const commonAlbumSort = consensusString(allItems.map((i) => i.albumsort));
          if (commonAlbumSort) albumsort = commonAlbumSort;

          const commonAlbumArtist = consensusString(allItems.map((i) => i.album_artist));
          if (commonAlbumArtist) {
            albumArtist = commonAlbumArtist;
            previousAlbumArtist = commonAlbumArtist;
          }

          const commonAlbumArtistSort = consensusString(allItems.map((i) => i.album_artist_sort));
          if (commonAlbumArtistSort) albumArtistSort = commonAlbumArtistSort;

          const commonGenre = consensusString(allItems.map((i) => i.genre));
          if (commonGenre) genre = commonGenre;

          const commonGenreSort = consensusString(allItems.map((i) => i.genresort));
          if (commonGenreSort) genresort = commonGenreSort;

          const commonYear = consensusNumber(allItems.map((i) => i.year));
          if (commonYear !== null) year = commonYear;

          disc.reset(allItems.map((i) => i.disc));
          discSubtitle.reset(allItems.map((i) => i.disc_subtitle));

          if (allItems.some((i) => i.compilation)) {
            compilation = allItems.every((i) => i.compilation);
          }

          if (allItems.some((i) => i.art_embedded)) {
            hasEmbeddedArtState = true;
          }
        }
      } catch (err) {
        console.error("Failed to preload file metadata for batch tag editing:", err);
      }
    }
  });

  const VARIOUS_ARTISTS = "Various Artists";
  // Remembers whatever was in Album Artist before the Compilation toggle
  // overwrote it, so unchecking restores it instead of leaving "Various
  // Artists" behind.
  // svelte-ignore state_referenced_locally
  let previousAlbumArtist = $state(initialAlbumArtist ?? "");

  function handleCompilationToggle(e: Event) {
    const next = (e.currentTarget as HTMLInputElement).checked;
    if (next) {
      previousAlbumArtist = albumArtist;
      albumArtist = VARIOUS_ARTISTS;
    } else {
      albumArtist = previousAlbumArtist;
    }
    compilation = next;
  }

  let isSaving = $state(false);
  let isClearingArt = $state(false);
  let showClearArtConfirm = $state(false);

  async function handleClearArt() {
    isClearingArt = true;
    try {
      const count = await invoke<number>("clear_album_cover_art", { songIds });

      await collectionStore.refreshStats();
      await collectionStore.refreshLibrary();

      toastStore.show(i18n.plural("albumTagEditor.clearArtSuccess", count), "success");
      if (onSave) onSave();
      onClose();
    } catch (e: any) {
      console.error("Failed to clear album artwork:", e);
      toastStore.show(i18n.t("albumTagEditor.clearArtFailedPrefix") + e.toString(), "error");
    } finally {
      isClearingArt = false;
      showClearArtConfirm = false;
    }
  }

  async function handleSave() {
    isSaving = true;
    const albumName = album || i18n.t("collection.unknownAlbum");
    const taskId = "album-tag-save";
    tasksStore.startTask({
      id: taskId,
      label: i18n.t("tasks.savingAlbumTags", { album: albumName }, `Saving tags for ${albumName}...`),
      total: tracksCount,
      contextName: albumName,
    });
    try {
      if (songIds.length > 0) {
        await invoke("save_album_tags", {
          songIds,
          album: album ?? "",
          albumsort: albumsort.trim() || null,
          albumArtist: albumArtist ?? "",
          albumArtistSort: albumArtistSort.trim() || null,
          genre: genre ?? "",
          genresort: genresort.trim() || null,
          year,
          disc: disc.edit,
          discSubtitle: discSubtitle.edit,
          compilation,
        });
      }
      if (filePaths.length > 0) {
        await invoke("save_batch_file_tags", {
          paths: filePaths,
          album: album ?? "",
          albumsort: albumsort.trim() || null,
          albumArtist: albumArtist ?? "",
          albumArtistSort: albumArtistSort.trim() || null,
          genre: genre ?? "",
          genresort: genresort.trim() || null,
          year,
          disc: disc.edit,
          discSubtitle: discSubtitle.edit,
          compilation,
        });
        if (source && folderPath) {
          await scanFolder(source, folderPath);
        }
        filesStore.reloadFolder();
      }

      tasksStore.completeTask(
        taskId,
        i18n.t("tasks.albumTagsSaved", { album: albumName }, `Saved tags for ${albumName}`)
      );
      await collectionStore.refreshStats();
      await collectionStore.refreshLibrary();

      if (onSave) onSave();
      onClose();
    } catch (e: any) {
      console.error("Failed to save album tags:", e);
      const errMsg = e.toString();
      tasksStore.failTask(taskId, errMsg);
      toastStore.show(i18n.t("albumTagEditor.saveFailedPrefix") + errMsg, "error");
    } finally {
      isSaving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      const target = e.target as HTMLElement;
      if (target.tagName === "BUTTON" || target.tagName === "TEXTAREA") return;
      if (isSaving) return;
      e.preventDefault();
      handleSave();
    }
  }
</script>

<Modal onClose={onClose} onKeydown={handleKeydown}>
    <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
      <div class="flex items-center gap-2">
        <Sliders class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold">{title || i18n.t('albumTagEditor.title')}</h3>
      </div>
      <button onclick={onClose} disabled={isSaving} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50">
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-200px)]">
      <div class="flex flex-col gap-4">
        {#if isRemoteSource}
          <div class="flex items-start gap-2.5 bg-brand-main border border-brand-border rounded-lg p-2.5 text-brand-text-secondary text-xs">
            <CloudIcon class="w-4 h-4 shrink-0 mt-0.5" />
            <span>{i18n.t('albumTagEditor.remoteSourceNote')}</span>
          </div>
        {/if}

        <div class="flex items-center gap-3">
          <CoverArt songId={songIds[0]} artEmbedded={hasEmbeddedArtState} artAutomatic={initialArtAutomatic} artManual={initialArtManual} sizeClass="w-12 h-12 rounded" />
          <div class="flex-1 flex flex-col gap-1 min-w-0">
            <span class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">{i18n.t('albumTagEditor.artworkField')}</span>
            <span class="text-xs text-brand-text-secondary">
              {hasEmbeddedArtState ? i18n.t('albumTagEditor.artworkEmbedded') : i18n.t('albumTagEditor.artworkNotEmbedded')}
            </span>
          </div>
          {#if !isRemoteSource}
            <Button
              onclick={() => { showClearArtConfirm = true; }}
              disabled={!hasEmbeddedArtState || isSaving || isClearingArt}
              variant="secondary"
              size="sm"
            >
              {#if isClearingArt}
                <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
                <span>{i18n.t('albumTagEditor.clearingArt')}</span>
              {:else}
                <ImageOff class="w-3.5 h-3.5" />
                <span>{i18n.t('albumTagEditor.clearArtBtn')}</span>
              {/if}
            </Button>
          {/if}
        </div>

        <div class="grid grid-cols-2 gap-4">
          <FormField label={i18n.t('albumTagEditor.albumField')} for="album-tag-album" span2>
            <Input id="album-tag-album" bind:value={album} disabled={isSaving} size="sm" class="w-full" />
          </FormField>

          <FormField label={i18n.t('albumTagEditor.albumArtistField')} for="album-tag-albumartist" span2>
            {#if compilation && albumArtist === VARIOUS_ARTISTS}
              <div class="flex items-center h-9">
                <span class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-brand-border bg-brand-main text-xs font-semibold text-brand-text-primary">
                  {VARIOUS_ARTISTS}
                  <Lock class="w-3 h-3 text-brand-text-secondary shrink-0" />
                </span>
              </div>
            {:else}
              <ChipInput
                id="album-tag-albumartist"
                bind:value={albumArtist}
                disabled={isSaving}
                placeholder={i18n.t('albumTagEditor.albumArtistPlaceholder')}
                class="w-full"
              />
            {/if}
          </FormField>

          <label class="flex items-center gap-2 col-span-2 text-xs text-brand-text-primary select-none">
            <input
              type="checkbox"
              id="album-tag-compilation"
              checked={compilation}
              onchange={handleCompilationToggle}
              disabled={isSaving}
              class="rounded accent-brand-accent"
            />
            {i18n.t('albumTagEditor.compilationField')}
          </label>

          <FormField label={i18n.t('albumTagEditor.genreField')} for="album-tag-genre" span2 tooltip={i18n.t('albumTagEditor.genreTooltip', {}, 'Drag chips to reorder — the first value is treated as the main genre in the Genres tab, the rest as subgenres of it.')}>
            <ChipInput
              id="album-tag-genre"
              bind:value={genre}
              disabled={isSaving}
              placeholder={i18n.t('albumTagEditor.genrePlaceholder')}
              suggestions={tagsStore.allTags.map((t) => t.name)}
              class="w-full"
            />
          </FormField>

          <FormField label={i18n.t('albumTagEditor.yearField')} for="album-tag-year">
            <Input
              id="album-tag-year"
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

          <DiscFields {disc} subtitle={discSubtitle} disabled={isSaving} />

          <!-- Sort Overrides ("Sort As") -->
          <details class="col-span-2 group border border-brand-border rounded-lg bg-brand-sidebar/40 overflow-hidden">
            <summary class="flex items-center justify-between px-3 py-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors">
              <span>{i18n.t("sortOverrides.heading")}</span>
              <span class="text-[10px] text-brand-text-secondary/70 group-open:rotate-180 transition-transform">▼</span>
            </summary>
            <div class="p-3 pt-2 grid grid-cols-2 gap-3 border-t border-brand-border/60">
              <FormField label={i18n.t("sortOverrides.album")} for="album-tag-albumsort">
                <Input id="album-tag-albumsort" bind:value={albumsort} disabled={isSaving} size="sm" class="w-full" />
              </FormField>

              <FormField label={i18n.t("sortOverrides.albumArtist")} for="album-tag-albumartistsort">
                <Input id="album-tag-albumartistsort" bind:value={albumArtistSort} disabled={isSaving} size="sm" class="w-full" />
              </FormField>

              <FormField label={i18n.t("sortOverrides.genre")} for="album-tag-genresort" span2>
                <Input id="album-tag-genresort" bind:value={genresort} disabled={isSaving} size="sm" class="w-full" />
              </FormField>
            </div>
          </details>
        </div>
      </div>
    </div>

    <div class="h-16 flex items-center justify-between px-6 border-t border-brand-border bg-brand-main shrink-0">
      <div class="flex items-center gap-2 text-xs font-medium text-brand-text-secondary">
        <Layers class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
        <span>{i18n.plural("albumTagEditor.tracksAffected", tracksCount)}</span>
      </div>

      <div class="flex items-center gap-3">
        <Button onclick={onClose} disabled={isSaving} variant="secondary" size="sm">
          {i18n.t('albumTagEditor.cancelBtn')}
        </Button>
        <Button onclick={handleSave} disabled={isSaving} variant="primary" size="sm">
          {#if isSaving}
            <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
            <span>{i18n.t('albumTagEditor.saving')}</span>
          {:else}
            <Save class="w-3.5 h-3.5" />
            <span>{i18n.t('albumTagEditor.saveBtn')}</span>
          {/if}
        </Button>
      </div>
    </div>
</Modal>

{#if showClearArtConfirm}
  <ConfirmDialog
    title={i18n.t('albumTagEditor.clearArtConfirmTitle')}
    message={i18n.t('albumTagEditor.clearArtConfirmMessage')}
    confirmLabel={i18n.t('albumTagEditor.clearArtBtn')}
    cancelLabel={i18n.t('albumTagEditor.cancelBtn')}
    onConfirm={handleClearArt}
    onCancel={() => { showClearArtConfirm = false; }}
  />
{/if}
