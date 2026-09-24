<script lang="ts">
  import { isRemotePath } from "../utils/remoteSource";
  import { invoke } from "@tauri-apps/api/core";
  import {
    XIcon as X,
    PlusIcon as Plus,
    TrashIcon as Trash2,
    GlobeIcon as Globe,
    TagIcon,
    LinkIcon,
    DiscIcon as Disc,
    FloppyDiskIcon as Save,
    CircleNotchIcon as LoaderCircle,
    StackIcon as Layers,
    LockIcon as Lock,
    ImageBrokenIcon as ImageOff,
    CloudIcon,
    FolderOpenIcon as FolderOpen
  } from "phosphor-svelte";
  import Button from "./Button.svelte";
  import FormField from "./FormField.svelte";
  import Input from "./Input.svelte";
  import ChipInput from "./ChipInput.svelte";
  import PlainChipInput from "./PlainChipInput.svelte";
  import CoverArt from "./CoverArt.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { tasksStore } from "../stores/tasks.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { portal } from "../utils/portal";
  import SocialIcon from "./SocialIcon.svelte";
  import { ALBUM_LINK_PLATFORMS, getPlatformInfo } from "../utils/artistSocials";
  import { getAlbumFolderPath } from "../utils/pathUtils";
  import type { AlbumProfile, AlbumLink } from "../types";

  let {
    albumName,
    artistName,
    songIds = [],
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
    isOpen = $bindable(false),
    onClose,
    onSaved,
  }: {
    albumName: string;
    artistName?: string | null;
    /** Song ids this album's embedded-tag fields (below) apply to — same
        shape as {@link import("./AlbumTagEditor.svelte")}, which this editor
        subsumes for the album detail view (#962: having two separate "Edit
        Album..." entries there, each writing a different disagreeing tag
        list, was the actual bug). */
    songIds?: number[];
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
    initialArtAutomatic?: string | null;
    initialArtManual?: string | null;
    isOpen?: boolean;
    onClose: () => void;
    onSaved?: (profile: AlbumProfile) => void;
  } = $props();

  // -- Curated profile fields (description/website/links; album_profiles table) --
  let website = $state("");
  let description = $state("");
  let links = $state<AlbumLink[]>([]);

  // -- Embedded ID3 tag fields (songs.genre and friends; save_album_tags) --
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

  let isSaving = $state(false);

  // Remote songs (WebDAV #682, OpenSubsonic #916) have no local file Luminous can write lofty tags to,
  // and there's no write-back to the remote server -- edits here only ever
  // reach Luminous's own DB. A representative track's path is enough since
  // an album's tracks all share one source.
  let samplePath = $derived.by(() => collectionStore.songs.find((s) => s.id === songIds[0])?.path ?? "");
  let isRemoteSource = $derived(isRemotePath(samplePath));
  let albumFolderPath = $derived.by(() => {
    const paths = songIds
      .map((id) => collectionStore.songs.find((s) => s.id === id)?.path)
      .filter((p): p is string => Boolean(p));
    return getAlbumFolderPath(paths);
  });

  // Sync state when opened or the underlying album changes
  $effect(() => {
    if (isOpen) {
      const existing = collectionStore.getAlbumProfile(albumName);
      website = existing?.website ?? "";
      description = existing?.description ?? "";
      links = existing?.links ? existing.links.map((l) => ({ ...l })) : [];

      album = initialAlbum ?? "";
      albumArtist = initialAlbumArtist ?? "";
      genre = initialGenre ?? "";
      year = initialYear;
      disc = initialDisc;
      compilation = initialCompilation ?? false;
      albumsort = initialAlbumSort ?? "";
      albumArtistSort = initialAlbumArtistSort ?? "";
      genresort = initialGenreSort ?? "";
      previousAlbumArtist = initialAlbumArtist ?? "";
    }
  });

  function handleAddLink() {
    links = [
      ...links,
      { platform: "bandcamp", handle_or_url: "" },
    ];
  }

  function handleRemoveLink(index: number) {
    links = links.filter((_, i) => i !== index);
  }

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

  let isOpeningFolder = $state(false);

  async function handleOpenFolder() {
    isOpeningFolder = true;
    try {
      await invoke("open_song_folder", { songIds });
    } catch (e: any) {
      console.error("Failed to open containing folder:", e);
      toastStore.show(i18n.t('albumTagEditor.openFolderFailedPrefix', {}, 'Failed to open folder: ') + e.toString(), "error");
    } finally {
      isOpeningFolder = false;
    }
  }

  let isClearingArt = $state(false);
  let showClearArtConfirm = $state(false);

  async function handleClearArt() {
    isClearingArt = true;
    try {
      const count = await invoke<number>("clear_album_cover_art", { songIds });

      await collectionStore.refreshStats();
      await collectionStore.refreshLibrary();

      toastStore.show(i18n.t("albumTagEditor.clearArtSuccess", { count }), "success");
    } catch (e: any) {
      console.error("Failed to clear album artwork:", e);
      toastStore.show(i18n.t("albumTagEditor.clearArtFailedPrefix") + e.toString(), "error");
    } finally {
      isClearingArt = false;
      showClearArtConfirm = false;
    }
  }

  async function handleSave() {
    if (!albumName.trim()) return;
    isSaving = true;
    try {
      const cleanLinks = links
        .filter((l) => l.handle_or_url.trim() !== "")
        .map((l) => ({
          platform: l.platform,
          handle_or_url: l.handle_or_url.trim(),
        }));

      const profile: AlbumProfile = {
        album_key: albumName,
        artist_key: artistName || null,
        website: website.trim() || null,
        links: cleanLinks,
        description: description.trim() || null,
      };

      const taskId = "album-tag-save";
      const targetAlbumName = album || albumName || i18n.t("collection.unknownAlbum");
      if (songIds.length > 0) {
        tasksStore.startTask({
          id: taskId,
          label: i18n.t("tasks.savingAlbumTags", { album: targetAlbumName }, `Saving tags for ${targetAlbumName}...`),
          total: songIds.length,
          contextName: targetAlbumName,
        });
      }

      const savePromises: Promise<unknown>[] = [collectionStore.saveAlbumProfile(profile)];
      if (songIds.length > 0) {
        savePromises.push(
          invoke("save_album_tags", {
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
          })
        );
      }

      const [saved] = await Promise.all(savePromises);

      if (songIds.length > 0) {
        tasksStore.completeTask(
          taskId,
          i18n.t("tasks.albumTagsSaved", { album: targetAlbumName }, `Saved tags for ${targetAlbumName}`)
        );
        await collectionStore.refreshStats();
      } else {
        toastStore.show(i18n.t("albumProfileEditor.savedSuccess", {}, "Album details updated"), "success");
      }
      await collectionStore.refreshLibrary();

      onSaved?.(saved as AlbumProfile);
      onClose();
    } catch (e) {
      console.error("Failed to save album details:", e);
      if (songIds.length > 0) {
        tasksStore.failTask("album-tag-save", String(e));
      }
      toastStore.show(i18n.t("albumProfileEditor.savedError", {}, "Failed to save album details"), "error");
    } finally {
      isSaving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && isOpen && !isSaving) {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    use:portal
    class="fixed inset-0 z-50 flex items-center justify-center p-4 sm:p-6 bg-black/60 backdrop-blur-xs animate-in fade-in duration-200"
    role="dialog"
    aria-modal="true"
    aria-labelledby="album-editor-title"
    tabindex="-1"
    onclick={(e) => { if (e.target === e.currentTarget && !isSaving) onClose(); }}
  >
    <div
      class="bg-brand-sidebar border border-brand-border rounded-xl shadow-2xl w-full max-w-xl max-h-[90vh] flex flex-col overflow-hidden text-brand-text-primary"
      onclick={(e) => e.stopPropagation()}
      role="document"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 sm:px-6 py-3.5 sm:py-4 border-b border-brand-border bg-brand-sidebar/80 shrink-0">
        <div class="flex items-center gap-2.5 min-w-0">
          <Disc class="w-5 h-5 text-brand-accent shrink-0" />
          <h2 id="album-editor-title" class="text-base sm:text-lg font-bold text-brand-text-primary truncate">
            {i18n.t("albumProfileEditor.title", {}, "Edit Album Details")}: <span class="text-brand-text-primary font-semibold">{albumName}</span>
          </h2>
        </div>
        <button
          onclick={onClose}
          class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 rounded-md transition-colors shrink-0 cursor-pointer"
          aria-label="Close dialog"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Body Form -->
      <div class="p-4 sm:p-6 overflow-y-auto flex-1 min-h-0 flex flex-col gap-4 sm:gap-5 text-sm">
        {#if isRemoteSource}
          <div class="flex items-start gap-2.5 bg-brand-main border border-brand-border rounded-lg p-2.5 text-brand-text-secondary text-xs">
            <CloudIcon class="w-4 h-4 shrink-0 mt-0.5" />
            <span>{i18n.t('albumTagEditor.remoteSourceNote')}</span>
          </div>
        {/if}

        {#if songIds.length > 0}
          <!-- Location -->
          <div class="flex items-center gap-3">
            <div class="flex-1 flex flex-col gap-1 min-w-0">
              <span class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">{i18n.t('albumTagEditor.locationField')}</span>
              <span class="text-xs text-brand-text-secondary break-all select-text">{albumFolderPath}</span>
            </div>
            {#if !isRemoteSource}
              <Button
                onclick={handleOpenFolder}
                disabled={isSaving || isOpeningFolder}
                variant="secondary"
                size="sm"
              >
                <FolderOpen class="w-3.5 h-3.5" />
                {i18n.t('albumTagEditor.openFolderBtn', {}, 'Open Folder')}
              </Button>
            {/if}
          </div>

          <!-- Artwork -->
          <div class="flex items-center gap-3">
            <CoverArt songId={songIds[0]} artEmbedded={hasEmbeddedArt} artAutomatic={initialArtAutomatic} artManual={initialArtManual} sizeClass="w-12 h-12 rounded" />
            <div class="flex-1 flex flex-col gap-1 min-w-0">
              <span class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">{i18n.t('albumTagEditor.artworkField')}</span>
              <span class="text-xs text-brand-text-secondary">
                {hasEmbeddedArt ? i18n.t('albumTagEditor.artworkEmbedded') : i18n.t('albumTagEditor.artworkNotEmbedded')}
              </span>
            </div>
            {#if !isRemoteSource}
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
            {/if}
          </div>

          <!-- Album / Artist / Genre / Year / Disc (embedded file tags) -->
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

            <div class="col-span-2 flex flex-col gap-2">
              <label for="album-tag-genre-input" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
                <TagIcon class="w-3.5 h-3.5 text-brand-accent" />
                {i18n.t('albumTagEditor.genreField')}
              </label>

              <PlainChipInput
                id="album-tag-genre-input"
                bind:value={genre}
                disabled={isSaving}
                placeholder={i18n.t('albumTagEditor.genrePlaceholder')}
              />
            </div>

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
        {/if}

        <!-- Description / Liner Notes Field -->
        <div class="flex flex-col gap-1.5">
          <label for="album-description" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
            {i18n.t("albumProfileEditor.description", {}, "Description & Liner Notes")}
          </label>
          <textarea
            id="album-description"
            bind:value={description}
            rows="4"
            placeholder={i18n.t("albumProfileEditor.descriptionPlaceholder", {}, "Add album background, liner notes, recording details, or reviews (Markdown links supported e.g. [Review](https://...))...")}
            class="w-full px-3 py-2 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent resize-none transition-colors"
          ></textarea>
        </div>

        <!-- Release Website / Official Page Field -->
        <div class="flex flex-col gap-1.5">
          <label for="album-website" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
            <Globe class="w-3.5 h-3.5 text-brand-accent" />
            {i18n.t("albumProfileEditor.website", {}, "Official Page / Store")}
          </label>
          <input
            id="album-website"
            type="text"
            bind:value={website}
            placeholder={i18n.t("albumProfileEditor.websitePlaceholder", {}, "https://artist.com/album or www.artist.com/album")}
            class="w-full px-3 py-2 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent transition-colors"
          />
        </div>

        <!-- Release Links (Bandcamp, Discogs, Spotify, etc.) -->
        <div class="flex flex-col gap-2.5">
          <div class="flex items-center justify-between">
            <label class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
              <LinkIcon class="w-3.5 h-3.5 text-brand-accent" />
              {i18n.t("albumProfileEditor.links", {}, "Release & Source Links")}
            </label>
            <button
              type="button"
              onclick={handleAddLink}
              class="text-xs font-medium text-brand-text-primary hover:underline flex items-center gap-1 cursor-pointer"
            >
              <Plus class="w-3.5 h-3.5" />
              {i18n.t("albumProfileEditor.addLinkBtn", {}, "Add Link")}
            </button>
          </div>

          {#if links.length === 0}
            <div class="px-4 py-3 rounded-lg border border-dashed border-brand-border text-center text-xs text-brand-text-secondary/60">
              {i18n.t("albumProfileEditor.noLinks", {}, 'No release links added yet. Click "Add Link" to attach Bandcamp, Discogs, reviews, or streaming links.')}
            </div>
          {:else}
            <div class="flex flex-col gap-2">
              {#each links as link, idx}
                {@const platformInfo = getPlatformInfo(link.platform)}
                <div class="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
                  <!-- Platform Select -->
                  <div class="relative sm:w-40 shrink-0">
                    <select
                      bind:value={link.platform}
                      class="w-full appearance-none pl-8 pr-6 py-1.5 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary text-xs focus:outline-none focus:border-brand-accent transition-colors"
                    >
                      {#each ALBUM_LINK_PLATFORMS as p (p.id)}
                        <option value={p.id}>
                          {p.id === "website" ? i18n.t("albumProfileEditor.website", {}, "Official Page") : p.id === "custom" ? i18n.t("albumProfileEditor.customLink", {}, "Custom Link") : p.label}
                        </option>
                      {/each}
                    </select>
                    <div class="absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none text-brand-text-secondary">
                      <SocialIcon platform={link.platform} size={14} />
                    </div>
                  </div>

                  <!-- Value / URL Input and Delete Button -->
                  <div class="flex-1 flex items-center gap-2 min-w-0">
                    <input
                      type="text"
                      bind:value={link.handle_or_url}
                      placeholder={platformInfo.placeholder}
                      class="w-full min-w-0 px-3 py-1.5 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary text-xs placeholder:text-brand-text-secondary/40 focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent transition-colors"
                    />
                    <button
                      type="button"
                      onclick={() => handleRemoveLink(idx)}
                      class="p-1.5 text-brand-text-secondary hover:text-red-400 hover:bg-red-500/10 rounded-md transition-colors shrink-0 cursor-pointer"
                      title={i18n.t("albumProfileEditor.removeLinkTooltip", {}, "Remove link")}
                    >
                      <Trash2 class="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <!-- Footer Buttons -->
      <div class="flex items-center justify-between gap-3 px-4 sm:px-6 py-3 sm:py-4 border-t border-brand-border bg-brand-sidebar/80 shrink-0">
        {#if songIds.length > 0}
          <div class="flex items-center gap-2 text-xs font-medium text-brand-text-secondary">
            <Layers class="w-3.5 h-3.5 text-brand-accent shrink-0" />
            <span>{i18n.t('albumTagEditor.tracksAffected', { count: songIds.length })}</span>
          </div>
        {:else}
          <span></span>
        {/if}
        <div class="flex items-center gap-3">
          <Button
            onclick={onClose}
            disabled={isSaving}
            variant="secondary"
            size="sm"
          >
            {i18n.t("albumProfileEditor.cancel", {}, "Cancel")}
          </Button>
          <Button
            onclick={handleSave}
            disabled={isSaving}
            variant="primary"
            size="sm"
          >
            {#if isSaving}
              <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
              <span>{i18n.t("albumProfileEditor.saving", {}, "Saving...")}</span>
            {:else}
              <Save class="w-3.5 h-3.5" />
              <span>{i18n.t("albumProfileEditor.save", {}, "Save")}</span>
            {/if}
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}

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
