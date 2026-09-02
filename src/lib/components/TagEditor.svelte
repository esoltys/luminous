<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import {
    SlidersIcon as Sliders,
    FloppyDiskIcon as Save,
    XIcon as X,
    SparkleIcon as Sparkles,
    CircleNotchIcon as LoaderCircle,
    WarningIcon as AlertTriangle,
    CheckIcon as Check,
    MagnifyingGlassMinusIcon as SearchX,
    LockIcon as Lock,
    ImageBrokenIcon as ImageOff
  } from "phosphor-svelte";
  import { fade } from "svelte/transition";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import SongRating from "./SongRating.svelte";
  import FormField from "./FormField.svelte";
  import LoadingSpinner from "./LoadingSpinner.svelte";
  import Modal from "./Modal.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import CoverArt from "./CoverArt.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import ChipInput from "./ChipInput.svelte";

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
  let grouping = $state("");
  let bpm = $state<number | null>(null);
  let initialKey = $state("");
  let path = $state("");
  let rating = $state(-1);
  // Compilation is an album-level property edited via AlbumTagEditor, not
  // here — this is read-only, just so a compilation's Album Artist shows
  // the same "Various Artists" pill here as it does there instead of an
  // editable field that would suggest a per-track override is meaningful.
  let compilation = $state(false);
  let artEmbedded = $state(false);
  let coverArtVersion = $state(0);
  let showClearArtConfirm = $state(false);
  let isClearingArt = $state(false);

  let titlesort = $state("");
  let artistsort = $state("");
  let albumsort = $state("");
  let albumArtistSort = $state("");
  let composersort = $state("");
  let genresort = $state("");

  let isLoading = $state(false);
  let isSaving = $state(false);
  let isLookingUp = $state(false);
  let lookupSucceeded = $state(false);
  let errorMsg = $state("");
  let lookupErrorMsg = $state("");
  let lookupNotFound = $state(false);
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
        year: number | null;
        grouping: string;
        bpm: number | null;
        initial_key: string;
        rating: number;
        compilation: boolean;
        art_embedded: boolean;
      }>("get_song_details", { songId });

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
      year = details.year;
      grouping = details.grouping;
      bpm = details.bpm;
      initialKey = details.initial_key;
      path = details.path;
      rating = details.rating;
      compilation = details.compilation;
      artEmbedded = details.art_embedded;
    } catch (e: any) {
      console.error("Failed to load metadata:", e);
      errorMsg = e.toString();
    } finally {
      isLoading = false;
    }
  }

  async function handleLookup() {
    if (!songId) return;
    isLookingUp = true;
    lookupErrorMsg = "";
    lookupSucceeded = false;
    lookupNotFound = false;
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
      if (str.includes("NO_API_KEY")) {
        navigationStore.activeTab = "settings";
        invoke("set_app_setting", { key: "active_settings_tab", value: "tools" });
        onClose();
        return;
      } else if (str.includes("fpcalc") || str.includes("chromaprint")) {
        lookupErrorMsg = i18n.t('tagEditor.acoustidFpcalcError');
      } else if (str.includes("invalid API key") || str.includes("API key")) {
        lookupErrorMsg = i18n.t('tagEditor.acoustidApiKeyError');
      } else if (str.includes("No matching")) {
        lookupNotFound = true;
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
        year,
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

  async function handleClearArt() {
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
  onMount(() => {
    // Best-effort preload for the genre field's autocomplete — a failure
    // here shouldn't block or break the editor itself.
    if (!tagsStore.loaded) tagsStore.load().catch(() => {});
  });

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
          <div class="flex flex-col gap-1 bg-brand-main border border-brand-border rounded-lg p-2.5">
            <span class="text-[9px] font-bold text-brand-text-secondary/60 uppercase font-mono">{i18n.t('tagEditor.locationField')}</span>
            <span class="text-[10px] text-brand-text-secondary break-all select-text font-mono">{path}</span>
          </div>

          <div class="flex items-center gap-3 bg-brand-main border border-brand-border rounded-lg p-2.5">
            {#key coverArtVersion}
              <CoverArt {songId} sizeClass="w-12 h-12 rounded" />
            {/key}
            <div class="flex-1 flex flex-col gap-0.5 min-w-0">
              <span class="text-[9px] font-bold text-brand-text-secondary/60 uppercase font-mono">{i18n.t('tagEditor.artworkField')}</span>
              <span class="text-[10px] text-brand-text-secondary font-mono">
                {artEmbedded ? i18n.t('tagEditor.artworkEmbedded') : i18n.t('tagEditor.artworkNotEmbedded')}
              </span>
            </div>
            <Button
              onclick={() => { showClearArtConfirm = true; }}
              disabled={!artEmbedded || isSaving || isClearingArt}
              variant="secondary"
              size="sm"
            >
              <ImageOff class="w-3.5 h-3.5" />
              {i18n.t('tagEditor.clearArtBtn')}
            </Button>
          </div>

          {#if lookupErrorMsg}
            <div class="flex items-start gap-2.5 bg-brand-main border border-red-500/40 rounded-xl p-3 text-red-500 text-xs">
              <AlertTriangle class="w-4 h-4 shrink-0 mt-0.5" />
              <span>{lookupErrorMsg}</span>
            </div>
          {/if}

          <!-- Grid form: field order mirrors the collection table's column order (track, title, artist,
               album, composer, album artist, year, genre, grouping, bpm, initial key), with Disc paired
               alongside Track (Disc first) since it has no column of its own in the table. -->
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

            <FormField label={i18n.t('tagEditor.artistField')} for="tag-artist">
              <ChipInput
                id="tag-artist"
                bind:value={artist}
                oninput={() => changedFields.delete("artist")}
                disabled={isSaving}
                placeholder={i18n.t('tagEditor.artistPlaceholder')}
                highlighted={changedFields.has('artist')}
                class="w-full"
              />
            </FormField>

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
                  changedFields.delete("year");
                }}
                size="sm"
                highlighted={changedFields.has('year')}
                class="w-full"
              />
            </FormField>

            <FormField label={i18n.t('tagEditor.genreField')} for="tag-genre" span2 tooltip={i18n.t('tagEditor.genreTooltip', {}, 'Drag chips to reorder — the first value is treated as the main genre in the Genres tab, the rest as subgenres of it.')}>
              <ChipInput
                id="tag-genre"
                bind:value={genre}
                disabled={isSaving}
                placeholder={i18n.t('tagEditor.genrePlaceholder')}
                suggestions={tagsStore.allTags.map((t) => t.name)}
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
            <div class="flex flex-col gap-1.5">
              <span class="text-[10px] font-bold text-brand-text-secondary/80 uppercase tracking-wide">{i18n.t('rating.label')}</span>
              <SongRating {rating} onRate={handleRate} size="md" />
            </div>

            <!-- Sort Overrides ("Sort As") -->
            <details class="col-span-2 group border border-brand-border rounded-lg bg-brand-sidebar/40 overflow-hidden mt-1">
              <summary class="flex items-center justify-between px-3 py-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors">
                <span>Sort Overrides ("Sort As")</span>
                <span class="text-[10px] text-brand-text-secondary/70 group-open:rotate-180 transition-transform">▼</span>
              </summary>
              <div class="p-3 pt-2 grid grid-cols-2 gap-3 border-t border-brand-border/60">
                <FormField label="Title Sort As" for="tag-titlesort">
                  <Input id="tag-titlesort" bind:value={titlesort} disabled={isSaving} size="sm" placeholder="e.g. Beatles, The" class="w-full" />
                </FormField>

                <FormField label="Artist Sort As" for="tag-artistsort">
                  <Input id="tag-artistsort" bind:value={artistsort} disabled={isSaving} size="sm" placeholder="e.g. Beatles, The" class="w-full" />
                </FormField>

                <FormField label="Album Sort As" for="tag-albumsort">
                  <Input id="tag-albumsort" bind:value={albumsort} disabled={isSaving} size="sm" class="w-full" />
                </FormField>

                <FormField label="Album Artist Sort As" for="tag-albumartistsort">
                  <Input id="tag-albumartistsort" bind:value={albumArtistSort} disabled={isSaving} size="sm" class="w-full" />
                </FormField>

                <FormField label="Composer Sort As" for="tag-composersort">
                  <Input id="tag-composersort" bind:value={composersort} disabled={isSaving} size="sm" class="w-full" />
                </FormField>

                <FormField label="Genre Sort As" for="tag-genresort">
                  <Input id="tag-genresort" bind:value={genresort} disabled={isSaving} size="sm" class="w-full" />
                </FormField>
              </div>
            </details>
          </div>
        </div>
      {/if}
    </div>

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
          {:else if lookupNotFound}
            <div in:fade class="flex items-center gap-1.5 text-brand-text-secondary text-xs font-semibold">
              <SearchX class="w-3.5 h-3.5" />
              <span>{i18n.t('tagEditor.notFound')}</span>
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
