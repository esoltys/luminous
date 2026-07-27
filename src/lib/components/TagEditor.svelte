<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { Sliders, Save, X, Sparkles, LoaderCircle, AlertTriangle, Check } from "lucide-svelte";
  import { fade } from "svelte/transition";
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import SongRating from "./SongRating.svelte";
  import FormField from "./FormField.svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";

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
  /** Fields last changed by an AcoustID lookup, so they can be highlighted until edited or re-looked-up. */
  let changedFields = $state(new Set<string>());

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
    changedFields = new Set();
    try {
      const suggestions = await invoke<{
        title: string | null;
        artist: string | null;
        album: string | null;
        year: number | null;
      }>("lookup_acoustid_tags", { songId });

      const next = new Set<string>();
      if (suggestions.title && suggestions.title !== title) {
        title = suggestions.title;
        next.add("title");
      }
      if (suggestions.artist && suggestions.artist !== artist) {
        artist = suggestions.artist;
        next.add("artist");
      }
      if (suggestions.album && suggestions.album !== album) {
        album = suggestions.album;
        next.add("album");
      }
      if (suggestions.year && suggestions.year !== year) {
        year = suggestions.year;
        next.add("year");
      }
      changedFields = next;
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
      toastStore.show(i18n.t('tagEditor.saveFailedPrefix') + e.toString(), "error");
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

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      const target = e.target as HTMLElement;
      if (target.tagName === "BUTTON" || target.tagName === "TEXTAREA") return;
      if (isSaving || isLoading) return;
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
          <!-- File Path (read-only) -->
          <div class="flex flex-col gap-1 bg-brand-main border border-brand-border rounded-lg p-2.5">
            <span class="text-[9px] font-bold text-brand-text-secondary/60 uppercase font-mono">{i18n.t('tagEditor.locationField')}</span>
            <span class="text-[10px] text-brand-text-secondary break-all select-text font-mono">{path}</span>
          </div>

          <!-- AcoustID lookup error info -->
          {#if lookupErrorMsg}
            <div class="flex items-start gap-2.5 bg-brand-main border border-red-500/40 rounded-xl p-3 text-red-500 text-xs">
              <AlertTriangle class="w-4 h-4 shrink-0 mt-0.5" />
              <span>{lookupErrorMsg}</span>
            </div>
          {/if}

          <!-- Grid form -->
          <div class="grid grid-cols-2 gap-4">
            <!-- Title -->
            <FormField label={i18n.t('tagEditor.titleField')} for="tag-title" span2>
              <Input
                id="tag-title"
                bind:value={title}
                oninput={() => changedFields.delete("title")}
                disabled={isSaving}
                size="sm"
                highlighted={changedFields.has('title')}
                class="w-full"
              />
            </FormField>

            <!-- Artist -->
            <FormField label={i18n.t('tagEditor.artistField')} for="tag-artist">
              <Input
                id="tag-artist"
                bind:value={artist}
                oninput={() => changedFields.delete("artist")}
                disabled={isSaving}
                size="sm"
                highlighted={changedFields.has('artist')}
                class="w-full"
              />
            </FormField>

            <!-- Album -->
            <FormField label={i18n.t('tagEditor.albumField')} for="tag-album">
              <Input
                id="tag-album"
                bind:value={album}
                oninput={() => changedFields.delete("album")}
                disabled={isSaving}
                size="sm"
                highlighted={changedFields.has('album')}
                class="w-full"
              />
            </FormField>

            <!-- Album Artist -->
            <FormField label={i18n.t('tagEditor.albumArtistField')} for="tag-albumartist">
              <Input id="tag-albumartist" bind:value={albumArtist} disabled={isSaving} size="sm" class="w-full" />
            </FormField>

            <!-- Composer -->
            <FormField label={i18n.t('tagEditor.composerField')} for="tag-composer">
              <Input id="tag-composer" bind:value={composer} disabled={isSaving} size="sm" class="w-full" />
            </FormField>

            <!-- Genre -->
            <FormField label={i18n.t('tagEditor.genreField')} for="tag-genre" span2>
              <Input id="tag-genre" bind:value={genre} disabled={isSaving} size="sm" class="w-full" />
            </FormField>

            <!-- Rating (library-only, saves immediately) -->
            <div class="flex flex-col gap-1.5 col-span-2">
              <span class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('rating.label')}</span>
              <SongRating {rating} onRate={handleRate} size="md" />
            </div>

            <!-- Year -->
            <FormField label={i18n.t('tagEditor.yearField')} for="tag-year">
              <Input
                id="tag-year"
                type="number"
                value={year ?? ""}
                disabled={isSaving}
                oninput={(e) => {
                  const val = parseInt(e.currentTarget.value, 10);
                  year = isNaN(val) ? null : val;
                  changedFields.delete("year");
                }}
                size="sm"
                highlighted={changedFields.has('year')}
                class="w-full"
              />
            </FormField>

            <!-- Track Number -->
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

            <!-- Disc Number -->
            <FormField label={i18n.t('tagEditor.discField')} for="tag-disc" span2>
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
          </div>
        </div>
      {/if}
    </div>

    <!-- Footer -->
    <div class="h-16 flex items-center justify-between px-6 border-t border-brand-border shrink-0 bg-brand-main">
      {#if !isLoading && !errorMsg}
        <div class="flex items-center gap-3">
          <Button onclick={handleLookup} disabled={isLookingUp || isSaving} variant="secondary" size="sm">
            {#if isLookingUp}
              <LoaderCircle class="w-3.5 h-3.5 animate-spin text-brand-accent-text" />
              {i18n.t('tagEditor.lookingUp')}
            {:else}
              <Sparkles class="w-3.5 h-3.5 text-brand-accent-text" />
              {i18n.t('tagEditor.lookupAcoustID')}
            {/if}
          </Button>
          {#if lookupSucceeded}
            <div in:fade class="flex items-center gap-1.5 text-brand-accent-text text-xs font-semibold">
              <Check class="w-3.5 h-3.5 font-bold {changedFields.size > 0 ? 'animate-bounce' : ''}" />
              <span>{changedFields.size > 0 ? i18n.t('tagEditor.matched') : i18n.t('tagEditor.noChange')}</span>
            </div>
          {/if}
        </div>
      {:else}
        <div></div>
      {/if}

      <div class="flex items-center gap-2">
        <Button onclick={onClose} disabled={isSaving} variant="secondary" size="sm">
          {i18n.t('tagEditor.cancelBtn')}
        </Button>
        <Button onclick={handleSave} disabled={isLoading || !!errorMsg || isSaving} variant="primary" size="sm">
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
