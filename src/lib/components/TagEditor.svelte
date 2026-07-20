<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Sliders, Save, X, Sparkles, LoaderCircle, AlertTriangle, Check } from "lucide-svelte";
  import { fade } from "svelte/transition";
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import SongRating from "./SongRating.svelte";
  import { portal } from "../utils/portal";

  interface Props {
    songId: number;
    onClose: () => void;
    onSave?: () => void;
  }

  let { songId, onClose, onSave }: Props = $props();

  let title = $state("");
  let artist = $state("");
  let album = $state("");
  let albumArtist = $state("");
  let composer = $state("");
  let genre = $state("");
  let track = $state<number | null>(null);
  let disc = $state<number | null>(null);
  let year = $state<number | null>(null);
  let path = $state("");
  let rating = $state(-1);

  let isLoading = $state(false);
  let isSaving = $state(false);
  let isLookingUp = $state(false);
  let lookupSucceeded = $state(false);
  let errorMsg = $state("");
  let lookupErrorMsg = $state("");

  async function loadMetadata() {
    isLoading = true;
    errorMsg = "";
    try {
      const details = await invoke<{
        id: number;
        path: string;
        title: string;
        artist: string;
        album: string;
        album_artist: string;
        composer: string;
        genre: string;
        track: number | null;
        disc: number | null;
        year: number | null;
        rating: number;
      }>("get_song_details", { songId });

      title = details.title;
      artist = details.artist;
      album = details.album;
      albumArtist = details.album_artist;
      composer = details.composer;
      genre = details.genre;
      track = details.track;
      disc = details.disc;
      year = details.year;
      path = details.path;
      rating = details.rating;
    } catch (e: any) {
      console.error("Failed to load metadata:", e);
      errorMsg = e.toString();
    } finally {
      isLoading = false;
    }
  }

  async function handleLookup() {
    isLookingUp = true;
    lookupErrorMsg = "";
    lookupSucceeded = false;
    try {
      const suggestions = await invoke<{
        title: string | null;
        artist: string | null;
        album: string | null;
        year: number | null;
      }>("lookup_acoustid_tags", { songId });

      if (suggestions.title) title = suggestions.title;
      if (suggestions.artist) artist = suggestions.artist;
      if (suggestions.album) album = suggestions.album;
      if (suggestions.year) year = suggestions.year;
      lookupSucceeded = true;
    } catch (e: any) {
      console.error("AcoustID lookup failed:", e);
      const str = e.toString();
      if (str.includes("fpcalc") || str.includes("chromaprint")) {
        lookupErrorMsg = i18n.t('tagEditor.acoustidFpcalcError');
      } else if (str.includes("invalid API key") || str.includes("API key")) {
        lookupErrorMsg = i18n.t('tagEditor.acoustidApiKeyError');
      } else {
        lookupErrorMsg = str;
      }
    } finally {
      isLookingUp = false;
    }
  }

  async function handleSave() {
    isSaving = true;
    try {
      await invoke("save_song_tags", {
        songId,
        title,
        artist,
        album,
        albumArtist,
        composer,
        genre,
        track,
        disc,
        year,
      });

      // Refresh the database views and collection store stats
      await collectionStore.refreshStats();
      await collectionStore.refreshLibrary();

      if (onSave) onSave();
      onClose();
    } catch (e: any) {
      console.error("Failed to save tags:", e);
      alert(i18n.t('tagEditor.saveFailedPrefix') + e.toString());
    } finally {
      isSaving = false;
    }
  }

  // Rating lives in the library database only (never written to the file),
  // so it saves immediately rather than waiting for the Save button.
  async function handleRate(value: number) {
    try {
      rating = await invoke<number>("set_song_rating", { songId, rating: value });
    } catch (e) {
      console.error("Failed to save rating:", e);
    }
  }

  onMount(loadMetadata);
</script>

<div use:portal class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-xs select-none">
  <div class="bg-brand-sidebar border border-brand-border rounded-2xl w-full max-w-lg overflow-hidden shadow-2xl flex flex-col text-brand-text-primary">
    <!-- Header -->
    <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
      <div class="flex items-center gap-2">
        <Sliders class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold">{i18n.t('tagEditor.title')}</h3>
      </div>
      <button onclick={onClose} disabled={isSaving} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors disabled:opacity-50">
        <X class="w-4 h-4" />
      </button>
    </div>

    <!-- Body -->
    <div class="flex-1 overflow-y-auto p-6 max-h-[calc(100vh-200px)]">
      {#if isLoading}
        <div class="w-full py-16 flex flex-col items-center justify-center gap-3">
          <LoaderCircle class="w-6 h-6 animate-spin text-brand-accent-text" />
          <span class="text-xs text-brand-text-secondary/60 font-medium">{i18n.t('tagEditor.readingTags')}</span>
        </div>
      {:else if errorMsg}
        <div class="w-full py-12 flex flex-col items-center justify-center gap-3 text-center">
          <AlertTriangle class="w-8 h-8 text-red-500" />
          <p class="text-sm font-semibold text-red-400">{i18n.t('tagEditor.readFailed')}</p>
          <p class="text-xs text-brand-text-secondary/65 max-w-xs">{errorMsg}</p>
        </div>
      {:else}
        <div class="flex flex-col gap-4">
          <!-- File Path (read-only) -->
          <div class="flex flex-col gap-1 bg-brand-main border border-brand-border rounded-lg p-2.5">
            <span class="text-[9px] font-bold text-brand-text-secondary/60 uppercase font-mono">{i18n.t('tagEditor.locationField')}</span>
            <span class="text-[10px] text-brand-text-secondary break-all select-text font-mono">{path}</span>
          </div>

          <!-- AcoustID lookup error info -->
          {#if lookupErrorMsg}
            <div class="flex items-start gap-2.5 bg-red-950/20 border border-red-900/60 rounded-xl p-3 text-red-300 text-xs">
              <AlertTriangle class="w-4 h-4 shrink-0 mt-0.5" />
              <span>{lookupErrorMsg}</span>
            </div>
          {/if}

          <!-- Grid form -->
          <div class="grid grid-cols-2 gap-4">
            <!-- Title -->
            <div class="flex flex-col gap-1 col-span-2">
              <label for="tag-title" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.titleField')}</label>
              <input
                id="tag-title"
                bind:value={title}
                disabled={isSaving}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>

            <!-- Artist -->
            <div class="flex flex-col gap-1">
              <label for="tag-artist" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.artistField')}</label>
              <input
                id="tag-artist"
                bind:value={artist}
                disabled={isSaving}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>

            <!-- Album -->
            <div class="flex flex-col gap-1">
              <label for="tag-album" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.albumField')}</label>
              <input
                id="tag-album"
                bind:value={album}
                disabled={isSaving}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>

            <!-- Album Artist -->
            <div class="flex flex-col gap-1">
              <label for="tag-albumartist" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.albumArtistField')}</label>
              <input
                id="tag-albumartist"
                bind:value={albumArtist}
                disabled={isSaving}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>

            <!-- Composer -->
            <div class="flex flex-col gap-1">
              <label for="tag-composer" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.composerField')}</label>
              <input
                id="tag-composer"
                bind:value={composer}
                disabled={isSaving}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>

            <!-- Genre -->
            <div class="flex flex-col gap-1 col-span-2">
              <label for="tag-genre" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.genreField')}</label>
              <input
                id="tag-genre"
                bind:value={genre}
                disabled={isSaving}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>

            <!-- Rating (library-only, saves immediately) -->
            <div class="flex flex-col gap-1.5 col-span-2">
              <span class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('rating.label')}</span>
              <SongRating {rating} onRate={handleRate} size="md" />
            </div>

            <!-- Year -->
            <div class="flex flex-col gap-1">
              <label for="tag-year" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.yearField')}</label>
              <input
                id="tag-year"
                type="number"
                value={year ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseInt((e.target as HTMLInputElement).value, 10);
                  year = isNaN(val) ? null : val;
                }}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>

            <!-- Track Number -->
            <div class="flex flex-col gap-1">
              <label for="tag-track" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.trackField')}</label>
              <input
                id="tag-track"
                type="number"
                value={track ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseInt((e.target as HTMLInputElement).value, 10);
                  track = isNaN(val) ? null : val;
                }}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>

            <!-- Disc Number -->
            <div class="flex flex-col gap-1 col-span-2">
              <label for="tag-disc" class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('tagEditor.discField')}</label>
              <input
                id="tag-disc"
                type="number"
                value={disc ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseInt((e.target as HTMLInputElement).value, 10);
                  disc = isNaN(val) ? null : val;
                }}
                class="bg-brand-main border border-brand-border rounded-lg px-3 py-2 text-xs text-brand-text-primary outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent disabled:opacity-50"
              />
            </div>
          </div>
        </div>
      {/if}
    </div>

    <!-- Footer -->
    <div class="h-16 flex items-center justify-between px-6 border-t border-brand-border shrink-0 bg-brand-main">
      {#if !isLoading && !errorMsg}
        <div class="flex items-center gap-3">
          <button
            onclick={handleLookup}
            disabled={isLookingUp || isSaving}
            class="flex items-center gap-1.5 bg-brand-sidebar border border-brand-border hover:bg-brand-main text-brand-text-secondary hover:text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold transition-all disabled:opacity-50"
          >
            {#if isLookingUp}
              <LoaderCircle class="w-3.5 h-3.5 animate-spin text-brand-accent-text" />
              {i18n.t('tagEditor.lookingUp')}
            {:else}
              <Sparkles class="w-3.5 h-3.5 text-brand-accent-text" />
              {i18n.t('tagEditor.lookupAcoustID')}
            {/if}
          </button>
          {#if lookupSucceeded}
            <div in:fade class="flex items-center gap-1.5 text-emerald-500 text-xs font-semibold">
              <Check class="w-3.5 h-3.5 font-bold animate-bounce" />
              <span>{i18n.t('tagEditor.matched')}</span>
            </div>
          {/if}
        </div>
      {:else}
        <div></div>
      {/if}

      <div class="flex items-center gap-2">
        <button
          onclick={onClose}
          disabled={isSaving}
          class="bg-brand-sidebar border border-brand-border hover:bg-brand-main text-brand-text-secondary hover:text-brand-text-primary px-4 py-2 rounded-lg text-xs font-semibold transition-all"
        >
          {i18n.t('tagEditor.cancelBtn')}
        </button>
        <button
          onclick={handleSave}
          disabled={isLoading || !!errorMsg || isSaving}
          class="flex items-center gap-1.5 bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast px-4 py-2 rounded-lg text-xs font-semibold transition-all shadow-lg shadow-brand-accent/20 disabled:opacity-50"
        >
          {#if isSaving}
            <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
            {i18n.t('tagEditor.updatingTags')}
          {:else}
            <Save class="w-3.5 h-3.5" />
            {i18n.t('tagEditor.saveBtn')}
          {/if}
        </button>
      </div>
    </div>
  </div>
</div>
