<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Sliders, Save, X, LoaderCircle, Layers } from "lucide-svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import FormField from "./FormField.svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";

  interface Props {
    songIds: number[];
    initialAlbum?: string;
    initialAlbumArtist?: string;
    initialGenre?: string;
    initialYear?: number | null;
    onClose: () => void;
    onSave?: () => void;
  }

  let {
    songIds,
    initialAlbum = "",
    initialAlbumArtist = "",
    initialGenre = "",
    initialYear = null,
    onClose,
    onSave
  }: Props = $props();

  // svelte-ignore state_referenced_locally
  let album = $state(initialAlbum);
  // svelte-ignore state_referenced_locally
  let albumArtist = $state(initialAlbumArtist);
  // svelte-ignore state_referenced_locally
  let genre = $state(initialGenre);
  // svelte-ignore state_referenced_locally
  let year = $state<number | null>(initialYear);

  let isSaving = $state(false);

  async function handleSave() {
    isSaving = true;
    try {
      await invoke("save_album_tags", {
        songIds,
        album,
        albumArtist,
        genre,
        year,
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
    <!-- Header -->
    <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
      <div class="flex items-center gap-2">
        <Sliders class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold">{i18n.t('albumTagEditor.title')}</h3>
      </div>
      <button onclick={onClose} disabled={isSaving} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50">
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-200px)]">
      <div class="flex flex-col gap-4">
        <!-- Form fields -->
        <div class="grid grid-cols-2 gap-4">
          <!-- Album Title -->
          <FormField label={i18n.t('albumTagEditor.albumField')} for="album-tag-album" span2>
            <Input id="album-tag-album" bind:value={album} disabled={isSaving} size="sm" class="w-full" />
          </FormField>

          <!-- Album Artist -->
          <FormField label={i18n.t('albumTagEditor.albumArtistField')} for="album-tag-albumartist" span2>
            <Input id="album-tag-albumartist" bind:value={albumArtist} disabled={isSaving} size="sm" class="w-full" />
          </FormField>

          <!-- Genre -->
          <FormField label={i18n.t('albumTagEditor.genreField')} for="album-tag-genre">
            <Input id="album-tag-genre" bind:value={genre} disabled={isSaving} size="sm" class="w-full" />
          </FormField>

          <!-- Year -->
          <FormField label={i18n.t('albumTagEditor.yearField')} for="album-tag-year">
            <Input id="album-tag-year" type="number" bind:value={year} disabled={isSaving} size="sm" class="w-full" />
          </FormField>
        </div>
      </div>
    </div>

    <!-- Footer -->
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
