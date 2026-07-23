<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Sliders, Save, X, LoaderCircle, Layers } from "lucide-svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { portal } from "../utils/portal";

  interface Props {
    songIds: number[];
    initialAlbum?: string;
    initialAlbumArtist?: string;
    initialGenre?: string;
    initialYear?: number | null;
    onClose: () => void;
    onSave?: () => void;
  }

  let props: Props = $props();

  let album = $state(props.initialAlbum ?? "");
  let albumArtist = $state(props.initialAlbumArtist ?? "");
  let genre = $state(props.initialGenre ?? "");
  let year = $state<number | null>(props.initialYear ?? null);

  let isSaving = $state(false);

  async function handleSave() {
    isSaving = true;
    try {
      await invoke("save_album_tags", {
        songIds: props.songIds,
        album,
        albumArtist,
        genre,
        year,
      });

      await collectionStore.refreshStats();
      await collectionStore.refreshLibrary();

      if (props.onSave) props.onSave();
      props.onClose();
    } catch (e: any) {
      console.error("Failed to save album tags:", e);
      alert(i18n.t("albumTagEditor.saveFailedPrefix") + e.toString());
    } finally {
      isSaving = false;
    }
  }
</script>

<div use:portal class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-xs select-none">
  <div class="bg-brand-sidebar border border-brand-border rounded-2xl w-full max-w-lg overflow-hidden shadow-2xl flex flex-col text-brand-text-primary">
    <!-- Header -->
    <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
      <div class="flex items-center gap-2">
        <Sliders class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold">{i18n.t('albumTagEditor.title')}</h3>
      </div>
      <button onclick={props.onClose} disabled={isSaving} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50">
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-200px)]">
      <div class="flex flex-col gap-4">
        <!-- Tracks badge -->
        <div class="flex items-center gap-2 bg-brand-main border border-brand-border rounded-lg px-3 py-2.5 text-xs font-medium text-brand-text-secondary">
          <Layers class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
          <span>{i18n.t('albumTagEditor.tracksAffected', { count: props.songIds.length })}</span>
        </div>

        <!-- Form fields -->
        <div class="grid grid-cols-2 gap-4">
          <!-- Album Title -->
          <div class="flex flex-col gap-1 col-span-2">
            <label for="album-tag-album" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">
              {i18n.t('albumTagEditor.albumField')}
            </label>
            <input
              id="album-tag-album"
              bind:value={album}
              disabled={isSaving}
              class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
            />
          </div>

          <!-- Album Artist -->
          <div class="flex flex-col gap-1 col-span-2">
            <label for="album-tag-albumartist" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">
              {i18n.t('albumTagEditor.albumArtistField')}
            </label>
            <input
              id="album-tag-albumartist"
              bind:value={albumArtist}
              disabled={isSaving}
              class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
            />
          </div>

          <!-- Genre -->
          <div class="flex flex-col gap-1">
            <label for="album-tag-genre" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">
              {i18n.t('albumTagEditor.genreField')}
            </label>
            <input
              id="album-tag-genre"
              bind:value={genre}
              disabled={isSaving}
              class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
            />
          </div>

          <!-- Year -->
          <div class="flex flex-col gap-1">
            <label for="album-tag-year" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">
              {i18n.t('albumTagEditor.yearField')}
            </label>
            <input
              id="album-tag-year"
              type="number"
              bind:value={year}
              disabled={isSaving}
              class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- Footer -->
    <div class="h-16 flex items-center justify-end px-6 border-t border-brand-border gap-3 bg-brand-main shrink-0">
      <button
        onclick={props.onClose}
        disabled={isSaving}
        class="px-4 py-2 text-xs font-semibold text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50"
      >
        {i18n.t('albumTagEditor.cancelBtn')}
      </button>
      <button
        onclick={handleSave}
        disabled={isSaving}
        class="flex items-center gap-2 px-5 py-2 rounded-xl bg-brand-accent hover:bg-brand-accent-hover text-white text-xs font-bold transition-all shadow-md shadow-brand-accent/20 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {#if isSaving}
          <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
          <span>{i18n.t('albumTagEditor.saving')}</span>
        {:else}
          <Save class="w-3.5 h-3.5" />
          <span>{i18n.t('albumTagEditor.saveBtn')}</span>
        {/if}
      </button>
    </div>
  </div>
</div>
