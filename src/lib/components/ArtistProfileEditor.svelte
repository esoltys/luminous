<script lang="ts">
  import {
    XIcon as X,
    PlusIcon as Plus,
    TrashIcon as Trash2,
    GlobeIcon as Globe,
    TagIcon,
    LinkIcon,
    UserIcon as User,
    FloppyDiskIcon as Save,
    CircleNotchIcon as LoaderCircle
  } from "phosphor-svelte";
  import Button from "./Button.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { portal } from "../utils/portal";
  import SocialIcon from "./SocialIcon.svelte";
  import { SOCIAL_PLATFORMS, getPlatformInfo, type SocialPlatformInfo } from "../utils/artistSocials";
  import type { ArtistProfile, ArtistSocialLink } from "../types";

  let {
    artistName,
    isOpen = $bindable(false),
    onClose,
    onSaved,
  }: {
    artistName: string;
    isOpen?: boolean;
    onClose: () => void;
    onSaved?: (profile: ArtistProfile) => void;
  } = $props();

  let website = $state("");
  let bio = $state("");
  let tags = $state<string[]>([]);
  let newTagInput = $state("");
  let socialLinks = $state<ArtistSocialLink[]>([]);
  let isSaving = $state(false);

  // Sync state when opened or artistName changes
  $effect(() => {
    if (isOpen) {
      const existing = collectionStore.getArtistProfile(artistName);
      website = existing?.website ?? "";
      bio = existing?.bio ?? "";
      tags = existing?.tags ? [...existing.tags] : [];
      socialLinks = existing?.social_links ? existing.social_links.map((l) => ({ ...l })) : [];
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

  function handleAddSocialLink() {
    socialLinks = [
      ...socialLinks,
      { platform: "website", handle_or_url: "" },
    ];
  }

  function handleRemoveSocialLink(index: number) {
    socialLinks = socialLinks.filter((_, i) => i !== index);
  }

  async function handleSave() {
    if (!artistName.trim()) return;
    isSaving = true;
    try {
      const cleanTags = tags.map((t) => t.trim()).filter(Boolean);
      const cleanLinks = socialLinks
        .filter((l) => l.handle_or_url.trim() !== "")
        .map((l) => ({
          platform: l.platform,
          handle_or_url: l.handle_or_url.trim(),
        }));

      const profile: ArtistProfile = {
        artist_key: artistName,
        website: website.trim() || null,
        tags: cleanTags,
        social_links: cleanLinks,
        bio: bio.trim() || null,
      };

      const saved = await collectionStore.saveArtistProfile(profile);
      toastStore.show(i18n.t("artistProfileEditor.savedSuccess", {}, "Artist profile updated"));
      onSaved?.(saved);
      onClose();
    } catch (err) {
      console.error("Failed to save artist profile:", err);
      toastStore.show(i18n.t("artistProfileEditor.savedError", {}, "Failed to save artist profile"), "error");
    } finally {
      isSaving = false;
    }
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      onClose();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-3 sm:p-4 overflow-y-auto"
    onclick={handleBackdropClick}
  >
    <div
      class="w-full max-w-xl bg-brand-sidebar border border-brand-border rounded-xl shadow-2xl overflow-hidden flex flex-col max-h-[85vh] sm:max-h-[90vh] my-auto animate-in fade-in zoom-in-95 duration-150"
      role="dialog"
      aria-modal="true"
      aria-labelledby="artist-editor-title"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 sm:px-6 py-3 sm:py-4 border-b border-brand-border bg-brand-sidebar/80 shrink-0">
        <div class="flex items-center gap-2.5 min-w-0 mr-2">
          <User class="w-5 h-5 text-brand-accent shrink-0" />
          <h2 id="artist-editor-title" class="text-base sm:text-lg font-bold text-brand-text-primary truncate">
            {i18n.t("artistProfileEditor.title", {}, "Edit Artist Details")}: <span class="text-brand-accent font-semibold">{artistName}</span>
          </h2>
        </div>
        <button
          onclick={onClose}
          class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 rounded-md transition-colors shrink-0"
          aria-label="Close dialog"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Body Form -->
      <div class="p-4 sm:p-6 overflow-y-auto flex-1 min-h-0 flex flex-col gap-4 sm:gap-5 text-sm">
        <!-- Bio Field -->
        <div class="flex flex-col gap-1.5">
          <label for="artist-bio" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider">
            {i18n.t("artistProfileEditor.bio", {}, "About / Biography")}
          </label>
          <textarea
            id="artist-bio"
            bind:value={bio}
            rows="3"
            placeholder={i18n.t("artistProfileEditor.bioPlaceholder", {}, "Add a bio or background notes for this artist...")}
            class="w-full px-3 py-2 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent resize-none transition-colors"
          ></textarea>
        </div>

        <!-- Tags Manager -->
        <div class="flex flex-col gap-2">
          <label for="artist-tags-input" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
            <TagIcon class="w-3.5 h-3.5 text-brand-accent" />
            {i18n.t("artistProfileEditor.tags", {}, "Tags")}
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
                    class="hover:text-red-400 focus:outline-none"
                    title={i18n.t("artistProfileEditor.removeTagTooltip", { tag }, `Remove tag ${tag}`)}
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
              id="artist-tags-input"
              type="text"
              bind:value={newTagInput}
              onkeydown={handleTagKeydown}
              placeholder={i18n.t("artistProfileEditor.tagInputPlaceholder", {}, "Add a tag and press Enter...")}
              class="flex-1 px-3 py-1.5 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary placeholder:text-brand-text-secondary/50 text-xs focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent transition-colors"
            />
            <button
              type="button"
              onclick={handleAddTag}
              disabled={!newTagInput.trim()}
              class="px-3 py-1.5 bg-brand-accent/10 hover:bg-brand-accent/20 disabled:opacity-40 text-brand-accent text-xs font-medium rounded-lg border border-brand-accent/20 transition-colors flex items-center gap-1"
            >
              <Plus class="w-3.5 h-3.5" />
              {i18n.t("artistProfileEditor.addTagBtn", {}, "Add")}
            </button>
          </div>
        </div>

        <!-- Website Field -->
        <div class="flex flex-col gap-1.5">
          <label for="artist-website" class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
            <Globe class="w-3.5 h-3.5 text-brand-accent" />
            {i18n.t("artistProfileEditor.website", {}, "Website")}
          </label>
          <input
            id="artist-website"
            type="text"
            bind:value={website}
            placeholder={i18n.t("artistProfileEditor.websitePlaceholder", {}, "https://www.artist.com or www.artist.com")}
            class="w-full px-3 py-2 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent transition-colors"
          />
        </div>

        <!-- Social & External Links -->
        <div class="flex flex-col gap-2.5">
          <div class="flex items-center justify-between">
            <label class="font-medium text-xs text-brand-text-secondary uppercase tracking-wider flex items-center gap-1.5">
              <LinkIcon class="w-3.5 h-3.5 text-brand-accent" />
              {i18n.t("artistProfileEditor.socialLinks", {}, "Links")}
            </label>
            <button
              type="button"
              onclick={handleAddSocialLink}
              class="text-xs font-medium text-brand-accent hover:underline flex items-center gap-1"
            >
              <Plus class="w-3.5 h-3.5" />
              {i18n.t("artistProfileEditor.addLinkBtn", {}, "Add Link")}
            </button>
          </div>

          {#if socialLinks.length === 0}
            <div class="px-4 py-3 rounded-lg border border-dashed border-brand-border text-center text-xs text-brand-text-secondary/60">
              {i18n.t("artistProfileEditor.noLinks", {}, 'No links added yet. Click "Add Link" to attach social media or streaming profiles.')}
            </div>
          {:else}
            <div class="flex flex-col gap-2">
              {#each socialLinks as link, idx}
                {@const platformInfo = getPlatformInfo(link.platform)}
                <div class="flex flex-col sm:flex-row items-stretch sm:items-center gap-2">
                  <!-- Platform Select -->
                  <div class="relative sm:w-40 shrink-0">
                    <select
                      bind:value={link.platform}
                      class="w-full appearance-none pl-8 pr-6 py-1.5 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary text-xs focus:outline-none focus:border-brand-accent transition-colors"
                    >
                      {#each SOCIAL_PLATFORMS as p (p.id)}
                        <option value={p.id}>
                          {p.id === "website" ? i18n.t("artistProfileEditor.website", {}, "Website") : p.id === "custom" ? i18n.t("artistProfileEditor.customLink", {}, "Custom Link") : p.label}
                        </option>
                      {/each}
                    </select>
                    <div class="absolute left-2.5 top-1/2 -translate-y-1/2 pointer-events-none text-brand-text-secondary">
                      <SocialIcon platform={link.platform} size={14} />
                    </div>
                  </div>

                  <!-- Value / Handle Input and Delete Button -->
                  <div class="flex-1 flex items-center gap-2 min-w-0">
                    <input
                      type="text"
                      bind:value={link.handle_or_url}
                      placeholder={platformInfo.placeholder}
                      class="w-full min-w-0 px-3 py-1.5 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary text-xs placeholder:text-brand-text-secondary/40 focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent transition-colors"
                    />
                    <button
                      type="button"
                      onclick={() => handleRemoveSocialLink(idx)}
                      class="p-1.5 text-brand-text-secondary hover:text-red-400 hover:bg-red-500/10 rounded-md transition-colors shrink-0"
                      title={i18n.t("artistProfileEditor.removeLinkTooltip", {}, "Remove link")}
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
          {i18n.t("artistProfileEditor.cancel", {}, "Cancel")}
        </Button>
        <Button
          onclick={handleSave}
          disabled={isSaving}
          variant="primary"
          size="sm"
        >
          {#if isSaving}
            <LoaderCircle class="w-3.5 h-3.5 animate-spin" />
            <span>{i18n.t("artistProfileEditor.saving", {}, "Saving...")}</span>
          {:else}
            <Save class="w-3.5 h-3.5" />
            <span>{i18n.t("artistProfileEditor.save", {}, "Save")}</span>
          {/if}
        </Button>
      </div>
    </div>
  </div>
{/if}
