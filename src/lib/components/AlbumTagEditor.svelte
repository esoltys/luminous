<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Sliders, Save, X, LoaderCircle, Layers, Lock, ImageOff } from "lucide-svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import FormField from "./FormField.svelte";
  import Modal from "./Modal.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import ChipInput from "./ChipInput.svelte";
  import CoverArt from "./CoverArt.svelte";

  interface Props {
    songIds: number[];
    initialAlbum?: string | null;
    initialAlbumSort?: string | null;
    initialAlbumArtist?: string | null;
    initialAlbumArtistSort?: string | null;
    initialGenre?: string | null;
    initialGenreSort?: string | null;
    initialYear?: number | null;
    initialDisc?: number | null;
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
    songIds,
    initialAlbum = "",
    initialAlbumSort = "",
    initialAlbumArtist = "",
    initialAlbumArtistSort = "",
    initialGenre = "",
    initialGenreSort = "",
    initialYear = null,
    initialDisc = null,
    initialCompilation = false,
    hasEmbeddedArt = false,
    initialArtAutomatic = null,
    initialArtManual = null,
    onClose,
    onSave
  }: Props = $props();

  // svelte-ignore state_referenced_locally
  let album = $state(initialAlbum ?? "");
  // svelte-ignore state_referenced_locally
  let albumArtist = $state(initialAlbumArtist ?? "");
  // svelte-ignore state_referenced_locally
  let genre = $state(initialGenre ?? "");
  // svelte-ignore state_referenced_locally
  let year = $state<number | null>(initialYear);
  // svelte-ignore state_referenced_locally
  let disc = $state<number | null>(initialDisc);
  // svelte-ignore state_referenced_locally
  let compilation = $state(initialCompilation ?? false);

  // svelte-ignore state_referenced_locally
  let albumsort = $state(initialAlbumSort ?? "");
  // svelte-ignore state_referenced_locally
  let albumArtistSort = $state(initialAlbumArtistSort ?? "");
  // svelte-ignore state_referenced_locally
  let genresort = $state(initialGenreSort ?? "");

  onMount(() => {
    // Best-effort preload for the genre field's autocomplete — a failure
    // here shouldn't block or break the editor itself.
    if (!tagsStore.loaded) tagsStore.load().catch(() => {});
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

      toastStore.show(i18n.t("albumTagEditor.clearArtSuccess", { count }), "success");
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
    try {
      await invoke("save_album_tags", {
        songIds,
        album: album ?? "",
        albumsort: albumsort.trim() || null,
        albumArtist: albumArtist ?? "",
        albumArtistSort: albumArtistSort.trim() || null,
        genre: genre ?? "",
        genresort: genresort.trim() || null,
        year,
        disc,
        compilation,
      });

      await collectionStore.refreshStats();
      await collectionStore.refreshLibrary();

      if (onSave) onSave();
      onClose();
    } catch (e: any) {
      console.error("Failed to save album tags:", e);
      toastStore.show(i18n.t("albumTagEditor.saveFailedPrefix") + e.toString(), "error");
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
        <h3 class="text-sm font-bold">{i18n.t('albumTagEditor.title')}</h3>
      </div>
      <button onclick={onClose} disabled={isSaving} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50">
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-200px)]">
      <div class="flex flex-col gap-4">
        <div class="flex items-center gap-3 bg-brand-main border border-brand-border rounded-lg p-2.5">
          <CoverArt songId={songIds[0]} artEmbedded={hasEmbeddedArt} artAutomatic={initialArtAutomatic} artManual={initialArtManual} sizeClass="w-12 h-12 rounded" />
          <div class="flex-1 flex flex-col gap-0.5 min-w-0">
            <span class="text-[9px] font-bold text-brand-text-secondary/60 uppercase font-mono">{i18n.t('albumTagEditor.artworkField')}</span>
            <span class="text-[10px] text-brand-text-secondary font-mono">
              {hasEmbeddedArt ? i18n.t('albumTagEditor.artworkEmbedded') : i18n.t('albumTagEditor.artworkNotEmbedded')}
            </span>
          </div>
          <Button
            onclick={() => { showClearArtConfirm = true; }}
            disabled={!hasEmbeddedArt || isSaving || isClearingArt}
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
        </div>

        <div class="grid grid-cols-2 gap-4">
          <FormField label={i18n.t('albumTagEditor.albumField')} for="album-tag-album" span2>
            <Input id="album-tag-album" bind:value={album} disabled={isSaving} size="sm" class="w-full" />
          </FormField>

          <FormField label={i18n.t('albumTagEditor.albumArtistField')} for="album-tag-albumartist" span2>
            {#if compilation}
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

          <FormField label={i18n.t('albumTagEditor.discField')} for="album-tag-disc">
            <Input
              id="album-tag-disc"
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

          <!-- Sort Overrides ("Sort As") -->
          <details class="col-span-2 group border border-brand-border rounded-lg bg-brand-sidebar/40 overflow-hidden">
            <summary class="flex items-center justify-between px-3 py-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors">
              <span>Sort Overrides ("Sort As")</span>
              <span class="text-[10px] text-brand-text-secondary/70 group-open:rotate-180 transition-transform">▼</span>
            </summary>
            <div class="p-3 pt-2 grid grid-cols-2 gap-3 border-t border-brand-border/60">
              <FormField label="Album Sort As" for="album-tag-albumsort">
                <Input id="album-tag-albumsort" bind:value={albumsort} disabled={isSaving} size="sm" class="w-full" />
              </FormField>

              <FormField label="Album Artist Sort As" for="album-tag-albumartistsort">
                <Input id="album-tag-albumartistsort" bind:value={albumArtistSort} disabled={isSaving} size="sm" class="w-full" />
              </FormField>

              <FormField label="Genre Sort As" for="album-tag-genresort" span2>
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
        <span>{i18n.t('albumTagEditor.tracksAffected', { count: songIds.length })}</span>
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
