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
    CloudIcon,
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
  import {
    SharedTagFields,
    librarySongs,
    type SharedTagValues,
  } from "../utils/sharedTagFields.svelte";
  import DiscFields from "./DiscFields.svelte";

  interface Props {
    songIds?: number[];
    filePaths?: string[];
    title?: string;
    source?: FileSource;
    folderPath?: string[];
    folderName?: string;
    /** The edited tracks' current tags, when the caller already has them;
        otherwise they're read from the tracks themselves. */
    initialTracks?: (SharedTagValues & Pick<Song, "artist">)[];
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
    initialTracks = [],
    hasEmbeddedArt = false,
    initialArtAutomatic = null,
    initialArtManual = null,
    onClose,
    onSave,
  }: Props = $props();

  const tracksCount = $derived((songIds?.length ?? 0) + (filePaths?.length ?? 0));

  const fields = new SharedTagFields();
  // svelte-ignore state_referenced_locally
  fields.reset(initialTracks);
  // svelte-ignore state_referenced_locally
  let hasEmbeddedArtState = $state(hasEmbeddedArt);

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

  onMount(async () => {
    // Best-effort preload for the genre field's autocomplete — a failure
    // here shouldn't block or break the editor itself.
    if (!tagsStore.loaded) tagsStore.load().catch(() => {});
    if (initialTracks.length > 0) return;
    try {
      const tracks: (SharedTagValues & Pick<Song, "artist" | "art_embedded">)[] =
        songIds.length > 0 ? await librarySongs(songIds) : [];
      for (const path of filePaths) {
        try {
          tracks.push(
            await invoke<SharedTagValues & Pick<Song, "artist" | "art_embedded">>(
              "get_file_song_details",
              { path },
            ),
          );
        } catch (err) {
          console.error("Failed to get file tags:", err);
        }
      }
      fields.reset(tracks);
      if (tracks.some((t) => t.art_embedded)) hasEmbeddedArtState = true;
    } catch (err) {
      console.error("Failed to preload file metadata for batch tag editing:", err);
    }
  });

  const VARIOUS_ARTISTS = "Various Artists";
  // Puts back whatever was in Album Artist before the Compilation toggle
  // overwrote it, so unchecking restores it instead of leaving "Various
  // Artists" behind.
  let restoreAlbumArtist: (() => void) | null = null;

  function handleCompilationToggle(e: Event) {
    const next = (e.currentTarget as HTMLInputElement).checked;
    if (next) {
      restoreAlbumArtist = fields.albumArtist.snapshot();
      fields.albumArtist.set(VARIOUS_ARTISTS);
    } else if (restoreAlbumArtist) {
      restoreAlbumArtist();
      restoreAlbumArtist = null;
    }
    fields.compilation.set(next);
  }

  /** A field's placeholder: "Mixed" while the tracks disagree. */
  function mixedOr(showsMixed: boolean, fallback?: string): string | undefined {
    return showsMixed ? i18n.t("albumTagEditor.mixedValues") : fallback;
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
    const albumName = fields.album.value || i18n.t("collection.unknownAlbum");
    const taskId = "album-tag-save";
    tasksStore.startTask({
      id: taskId,
      label: i18n.t(
        "tasks.savingAlbumTags",
        { album: albumName },
        `Saving tags for ${albumName}...`,
      ),
      total: tracksCount,
      contextName: albumName,
    });
    try {
      if (songIds.length > 0) {
        await invoke("save_album_tags", { songIds, edit: fields.edit });
      }
      if (filePaths.length > 0) {
        await invoke("save_batch_file_tags", { paths: filePaths, edit: fields.edit });
        if (source && folderPath) {
          await scanFolder(source, folderPath);
        }
        filesStore.reloadFolder();
      }

      tasksStore.completeTask(
        taskId,
        i18n.t("tasks.albumTagsSaved", { album: albumName }, `Saved tags for ${albumName}`),
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

<Modal {onClose} onKeydown={handleKeydown}>
  <div
    class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main"
  >
    <div class="flex items-center gap-2">
      <Sliders class="w-4 h-4 text-brand-accent-text" />
      <h3 class="text-sm font-bold">{title || i18n.t("albumTagEditor.title")}</h3>
    </div>
    <button
      onclick={onClose}
      disabled={isSaving}
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50"
    >
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-200px)]">
    <div class="flex flex-col gap-4">
      {#if isRemoteSource}
        <div
          class="flex items-start gap-2.5 bg-brand-main border border-brand-border rounded-lg p-2.5 text-brand-text-secondary text-xs"
        >
          <CloudIcon class="w-4 h-4 shrink-0 mt-0.5" />
          <span>{i18n.t("albumTagEditor.remoteSourceNote")}</span>
        </div>
      {/if}

      <div class="flex items-center gap-3">
        <CoverArt
          songId={songIds[0]}
          artEmbedded={hasEmbeddedArtState}
          artAutomatic={initialArtAutomatic}
          artManual={initialArtManual}
          sizeClass="w-12 h-12 rounded"
        />
        <div class="flex-1 flex flex-col gap-1 min-w-0">
          <span class="label-field text-brand-text-secondary"
            >{i18n.t("albumTagEditor.artworkField")}</span
          >
          <span class="text-xs text-brand-text-secondary">
            {hasEmbeddedArtState
              ? i18n.t("albumTagEditor.artworkEmbedded")
              : i18n.t("albumTagEditor.artworkNotEmbedded")}
          </span>
        </div>
        {#if !isRemoteSource}
          <Button
            onclick={() => {
              showClearArtConfirm = true;
            }}
            disabled={!hasEmbeddedArtState || isSaving || isClearingArt}
            variant="secondary"
            size="sm"
          >
            {#if isClearingArt}
              <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
              <span>{i18n.t("albumTagEditor.clearingArt")}</span>
            {:else}
              <ImageOff class="w-3.5 h-3.5" />
              <span>{i18n.t("albumTagEditor.clearArtBtn")}</span>
            {/if}
          </Button>
        {/if}
      </div>

      <div class="grid grid-cols-2 gap-4">
        <FormField label={i18n.t("albumTagEditor.albumField")} for="album-tag-album" span2>
          <Input
            id="album-tag-album"
            bind:value={() => fields.album.value ?? "", (v) => fields.album.set(v)}
            placeholder={mixedOr(fields.album.showsMixed)}
            disabled={isSaving}
            size="sm"
            class="w-full"
          />
        </FormField>

        <FormField
          label={i18n.t("albumTagEditor.albumArtistField")}
          for="album-tag-albumartist"
          span2
        >
          {#if fields.compilation.value === true && fields.albumArtist.value === VARIOUS_ARTISTS}
            <div class="flex items-center h-9">
              <span
                class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-brand-border bg-brand-main text-xs font-semibold text-brand-text-primary"
              >
                {VARIOUS_ARTISTS}
                <Lock class="w-3 h-3 text-brand-text-secondary shrink-0" />
              </span>
            </div>
          {:else}
            <ChipInput
              id="album-tag-albumartist"
              bind:value={() => fields.albumArtist.value ?? "", (v) => fields.albumArtist.set(v)}
              disabled={isSaving}
              placeholder={mixedOr(
                fields.albumArtist.showsMixed,
                i18n.t("albumTagEditor.albumArtistPlaceholder"),
              )}
              class="w-full"
            />
          {/if}
        </FormField>

        <label
          class="flex items-center gap-2 col-span-2 text-xs text-brand-text-primary select-none"
        >
          <input
            type="checkbox"
            id="album-tag-compilation"
            checked={fields.compilation.value === true}
            indeterminate={fields.compilation.showsMixed}
            onchange={handleCompilationToggle}
            disabled={isSaving}
            class="rounded accent-brand-accent"
          />
          {i18n.t("albumTagEditor.compilationField")}
        </label>

        <FormField
          label={i18n.t("albumTagEditor.genreField")}
          for="album-tag-genre"
          span2
          tooltip={i18n.t(
            "albumTagEditor.genreTooltip",
            {},
            "Drag chips to reorder — the first value is treated as the main genre in the Genres tab, the rest as subgenres of it.",
          )}
        >
          <ChipInput
            id="album-tag-genre"
            bind:value={() => fields.genre.value ?? "", (v) => fields.genre.set(v)}
            disabled={isSaving}
            placeholder={mixedOr(
              fields.genre.showsMixed,
              i18n.t("albumTagEditor.genrePlaceholder"),
            )}
            suggestions={tagsStore.allTags.map((t) => t.name)}
            class="w-full"
          />
        </FormField>

        <FormField label={i18n.t("albumTagEditor.yearField")} for="album-tag-year">
          <Input
            id="album-tag-year"
            type="number"
            value={fields.year.value ?? ""}
            disabled={isSaving}
            placeholder={mixedOr(fields.year.showsMixed)}
            oninput={(e) => {
              const val = parseInt(e.currentTarget.value, 10);
              fields.year.set(isNaN(val) ? null : val);
            }}
            size="sm"
            class="w-full"
          />
        </FormField>

        <FormField
          label={i18n.t("albumTagEditor.languageField")}
          for="album-tag-language"
          tooltip={i18n.t("albumTagEditor.languageTooltip")}
        >
          <Input
            id="album-tag-language"
            value={fields.language.value ?? ""}
            disabled={isSaving}
            placeholder={mixedOr(
              fields.language.showsMixed,
              i18n.t("albumTagEditor.languagePlaceholder"),
            )}
            oninput={(e) => fields.language.set(e.currentTarget.value)}
            size="sm"
            class="w-full"
          />
        </FormField>

        <DiscFields disc={fields.disc} subtitle={fields.discSubtitle} disabled={isSaving} />

        <!-- Sort Overrides ("Sort As") -->
        <details
          class="col-span-2 group border border-brand-border rounded-lg bg-brand-sidebar/40 overflow-hidden"
        >
          <summary
            class="flex items-center justify-between px-3 py-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors"
          >
            <span>{i18n.t("sortOverrides.heading")}</span>
            <span
              class="text-[10px] text-brand-text-secondary/70 group-open:rotate-180 transition-transform"
              >▼</span
            >
          </summary>
          <div class="p-3 pt-2 grid grid-cols-2 gap-3 border-t border-brand-border/60">
            <FormField label={i18n.t("sortOverrides.album")} for="album-tag-albumsort">
              <Input
                id="album-tag-albumsort"
                bind:value={() => fields.albumsort.value ?? "", (v) => fields.albumsort.set(v)}
                placeholder={mixedOr(fields.albumsort.showsMixed)}
                disabled={isSaving}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t("sortOverrides.albumArtist")} for="album-tag-albumartistsort">
              <Input
                id="album-tag-albumartistsort"
                bind:value={
                  () => fields.albumArtistSort.value ?? "", (v) => fields.albumArtistSort.set(v)
                }
                placeholder={mixedOr(fields.albumArtistSort.showsMixed)}
                disabled={isSaving}
                size="sm"
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t("sortOverrides.genre")} for="album-tag-genresort" span2>
              <Input
                id="album-tag-genresort"
                bind:value={() => fields.genresort.value ?? "", (v) => fields.genresort.set(v)}
                placeholder={mixedOr(fields.genresort.showsMixed)}
                disabled={isSaving}
                size="sm"
                class="w-full"
              />
            </FormField>
          </div>
        </details>
      </div>
    </div>
  </div>

  <div
    class="h-16 flex items-center justify-between px-6 border-t border-brand-border bg-brand-main shrink-0"
  >
    <div class="flex items-center gap-2 text-xs font-medium text-brand-text-secondary">
      <Layers class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
      <span>{i18n.plural("albumTagEditor.tracksAffected", tracksCount)}</span>
    </div>

    <div class="flex items-center gap-3">
      <Button onclick={onClose} disabled={isSaving} variant="secondary" size="sm">
        {i18n.t("albumTagEditor.cancelBtn")}
      </Button>
      <Button onclick={handleSave} disabled={isSaving} variant="primary" size="sm">
        {#if isSaving}
          <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
          <span>{i18n.t("albumTagEditor.saving")}</span>
        {:else}
          <Save class="w-3.5 h-3.5" />
          <span>{i18n.t("albumTagEditor.saveBtn")}</span>
        {/if}
      </Button>
    </div>
  </div>
</Modal>

{#if showClearArtConfirm}
  <ConfirmDialog
    title={i18n.t("albumTagEditor.clearArtConfirmTitle")}
    message={i18n.t("albumTagEditor.clearArtConfirmMessage")}
    confirmLabel={i18n.t("albumTagEditor.clearArtBtn")}
    cancelLabel={i18n.t("albumTagEditor.cancelBtn")}
    onConfirm={handleClearArt}
    onCancel={() => {
      showClearArtConfirm = false;
    }}
  />
{/if}
