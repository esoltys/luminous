<script lang="ts">
  import {
    XIcon as X,
    PlusIcon as Plus,
    TrashIcon as Trash2,
    GlobeIcon as Globe,
    TagIcon,
    LinkIcon,
    DiscIcon as Disc,
    FloppyDiskIcon as Save,
    CircleNotchIcon as LoaderCircle
  } from "phosphor-svelte";
  import Button from "./Button.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { portal } from "../utils/portal";
  import SocialIcon from "./SocialIcon.svelte";
  import { ALBUM_LINK_PLATFORMS, getPlatformInfo } from "../utils/artistSocials";
  import type { AlbumProfile, AlbumLink } from "../types";

  let {
    albumName,
    artistName,
    isOpen = $bindable(false),
    onClose,
    onSaved,
  }: {
    albumName: string;
    artistName?: string | null;
    isOpen?: boolean;
    onClose: () => void;
    onSaved?: (profile: AlbumProfile) => void;
  } = $props();

  let website = $state("");
  let description = $state("");
  let tags = $state<string[]>([]);
  let newTagInput = $state("");
  let links = $state<AlbumLink[]>([]);
  let isSaving = $state(false);

  // Sync state when opened or albumName changes
  $effect(() => {
    if (isOpen) {
      const existing = collectionStore.getAlbumProfile(albumName);
      website = existing?.website ?? "";
      description = existing?.description ?? "";
      tags = existing?.tags ? [...existing.tags] : [];
      links = existing?.links ? existing.links.map((l) => ({ ...l })) : [];
      newTagInput = "";
    }
  });

  function handleAddTag() {
    const raw = newTagInput.trim().replace(/^,+|,+$/g, "");
    if (!raw) return;
    const parts = raw.split(",").map((s) => s.trim().toLowerCase()).filter(Boolean);
    for (const part of parts) {
      if (!tags.some((t) => t.toLowerCase() === part)) {
        tags = [...tags, part];
      }
    }
    newTagInput = "";
  }

  function handleTagKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      handleAddTag();
    }
  }

  function handleRemoveTag(index: number) {
    tags = tags.filter((_, i) => i !== index);
  }

  function handleAddLink() {
    links = [
      ...links,
      { platform: "bandcamp", handle_or_url: "" },
    ];
  }

  function handleRemoveLink(index: number) {
    links = links.filter((_, i) => i !== index);
  }

  async function handleSave() {
    if (!albumName.trim()) return;
    isSaving = true;
    try {
      const cleanTags = tags.map((t) => t.trim()).filter(Boolean);
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
        tags: cleanTags,
        links: cleanLinks,
        description: description.trim() || null,
      };

      const saved = await collectionStore.saveAlbumProfile(profile);
      toastStore.show(i18n.t("albumProfileEditor.savedSuccess", {}, "Album profile updated"), "success");
      onSaved?.(saved);
      onClose();
    } catch (e) {
      console.error("Failed to save album profile:", e);
      toastStore.show(i18n.t("albumProfileEditor.savedError", {}, "Failed to save album profile"), "error");
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
            {i18n.t("albumProfileEditor.title", {}, "Edit Album Details")}: <span class="text-brand-accent font-semibold">{albumName}</span>
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

        <!-- Tags Manager -->
        <div class="flex flex-col gap-2">
          <label for="album-tags-input" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
            <TagIcon class="w-3.5 h-3.5 text-brand-accent" />
            {i18n.t("albumProfileEditor.tags", {}, "Album Tags")}
          </label>

          <!-- Current Tags Pills -->
          {#if tags.length > 0}
            <div class="flex flex-wrap gap-1.5">
              {#each tags as tag, idx (tag)}
                <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25">
                  <span>{tag}</span>
                  <button
                    type="button"
                    onclick={() => handleRemoveTag(idx)}
                    class="hover:text-red-400 focus:outline-none cursor-pointer"
                    title={i18n.t("albumProfileEditor.removeTagTooltip", { tag }, `Remove tag ${tag}`)}
                  >
                    <X class="w-3 h-3" />
                  </button>
                </span>
              {/each}
            </div>
          {/if}

          <!-- Add Tag Input -->
          <div class="flex items-center gap-2">
            <input
              id="album-tags-input"
              type="text"
              bind:value={newTagInput}
              onkeydown={handleTagKeydown}
              placeholder={i18n.t("albumProfileEditor.tagInputPlaceholder", {}, "Add an album tag and press Enter...")}
              class="flex-1 px-3 py-1.5 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary placeholder:text-brand-text-secondary/50 text-xs focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent transition-colors"
            />
            <button
              type="button"
              onclick={handleAddTag}
              disabled={!newTagInput.trim()}
              class="px-3 py-1.5 bg-brand-accent/10 hover:bg-brand-accent/20 disabled:opacity-40 text-brand-accent text-xs font-medium rounded-lg border border-brand-accent/20 transition-colors flex items-center gap-1 cursor-pointer"
            >
              <Plus class="w-3.5 h-3.5" />
              {i18n.t("albumProfileEditor.addTagBtn", {}, "Add")}
            </button>
          </div>
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
              class="text-xs font-medium text-brand-accent hover:underline flex items-center gap-1 cursor-pointer"
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
      <div class="flex items-center justify-end gap-3 px-4 sm:px-6 py-3 sm:py-4 border-t border-brand-border bg-brand-sidebar/80 shrink-0">
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
{/if}