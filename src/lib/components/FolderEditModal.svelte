<script lang="ts">
  import { untrack } from "svelte";
  import type { MusicDirectory } from "../types";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { PLAYER_DOCK_CLEARANCE_PX } from "../constants";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import LibraryBadge from "./LibraryBadge.svelte";
  import ColorPicker from "./ColorPicker.svelte";
  import { BADGE_ICON_CHOICES, BADGE_COLOR_CHOICES } from "../badgeChoices";
  import {
    XIcon as X,
    FolderSimpleIcon as FolderSimple,
  } from "phosphor-svelte";

  interface Props {
    directory: MusicDirectory;
    onClose: () => void;
  }

  let { directory, onClose }: Props = $props();

  let nickname = $state(untrack(() => directory.nickname ?? ""));
  let selectedIcon = $state(untrack(() => directory.icon ?? "folder"));
  let selectedColor = $state<string | null>(untrack(() => directory.color ?? null));
  let saving = $state(false);

  const ICON_CHOICES = BADGE_ICON_CHOICES;
  const COLOR_CHOICES = BADGE_COLOR_CHOICES;

  let previewDirectory = $derived<MusicDirectory>({
    ...directory,
    nickname: nickname.trim() || null,
    icon: selectedIcon,
    color: selectedColor,
  });

  let dockClearance = $derived(playerStore.currentSong ? PLAYER_DOCK_CLEARANCE_PX : 0);

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onClose();
    }
  }

  async function handleSubmit(e: SubmitEvent) {
    e.preventDefault();
    if (saving) return;
    saving = true;
    try {
      await collectionStore.updateDirectoryMetadata(directory.id, {
        nickname: nickname.trim() || null,
        icon: selectedIcon,
        color: selectedColor,
      });
      onClose();
    } catch (err) {
      console.error("Failed to update directory metadata:", err);
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm p-4"
  style="padding-bottom: {dockClearance + 16}px"
  onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
>
  <div
    class="bg-brand-sidebar border border-brand-border rounded-2xl w-full max-w-lg shadow-2xl overflow-hidden flex flex-col animate-in fade-in zoom-in-95 duration-150"
    style="max-height: min(90vh, calc(100vh - {dockClearance + 32}px))"
  >
    <div class="flex items-center justify-between px-6 py-4 border-b border-brand-border/60 bg-brand-main/50">
      <div class="flex items-center gap-2.5 min-w-0">
        <div class="p-2 rounded-xl bg-brand-accent/20 text-brand-accent-text shrink-0">
          <FolderSimple class="w-5 h-5" />
        </div>
        <div class="min-w-0">
          <h2 class="text-base font-bold text-brand-text-primary truncate">{i18n.t("settings.editFolderTitle")}</h2>
          <p class="text-xs text-brand-text-secondary/70 truncate" title={directory.path}>{directory.path}</p>
        </div>
      </div>
      <button
        type="button"
        onclick={onClose}
        class="text-brand-text-secondary hover:text-brand-text-primary p-1.5 rounded-lg hover:bg-brand-main/80 transition-colors"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <form onsubmit={handleSubmit} class="p-6 flex-1 overflow-y-auto flex flex-col gap-5">
      <!-- Live Preview -->
      <div class="bg-brand-main/40 border border-brand-border/50 rounded-xl p-4 flex items-center justify-between gap-4">
        <span class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider">
          {i18n.t("settings.folderPreview")}
        </span>
        <div>
          <LibraryBadge directory={previewDirectory} size="md" />
        </div>
      </div>

      <!-- Nickname -->
      <div>
        <label for="folder-nickname-input" class="block text-xs font-semibold text-brand-text-secondary uppercase tracking-wider mb-1.5">
          {i18n.t("settings.folderNickname")}
        </label>
        <Input
          id="folder-nickname-input"
          type="text"
          bind:value={nickname}
          placeholder={i18n.t("settings.folderNicknamePlaceholder")}
          class="w-full"
        />
      </div>

      <!-- Icon Selection -->
      <div>
        <span class="block text-xs font-semibold text-brand-text-secondary uppercase tracking-wider mb-2">
          {i18n.t("settings.folderIcon")}
        </span>
        <div class="grid grid-cols-5 gap-2">
          {#each ICON_CHOICES as choice}
            {@const Icon = choice.icon}
            {@const isSelected = selectedIcon === choice.id}
            <button
              type="button"
              onclick={() => { selectedIcon = choice.id; }}
              class="flex flex-col items-center justify-center p-2.5 rounded-xl border transition-all duration-150 gap-1
                {isSelected
                  ? 'bg-brand-accent/20 border-brand-accent text-brand-accent-text ring-1 ring-brand-accent'
                  : 'bg-brand-main/40 border-brand-border/60 text-brand-text-secondary hover:text-brand-text-primary hover:border-brand-border'}"
              title={choice.label}
            >
              <Icon class="w-5 h-5" />
              <span class="text-[10px] font-medium truncate max-w-full">{choice.label}</span>
            </button>
          {/each}
        </div>
      </div>

      <!-- Color Selection -->
      <div>
        <span class="block text-xs font-semibold text-brand-text-secondary uppercase tracking-wider mb-2">
          {i18n.t("settings.folderColor")}
        </span>
        <ColorPicker choices={COLOR_CHOICES} value={selectedColor} onChange={(v) => { selectedColor = v; }} />
      </div>

      <!-- Footer Buttons -->
      <div class="flex items-center justify-end gap-3 pt-4 border-t border-brand-border/60 mt-2">
        <Button type="button" variant="secondary" onclick={onClose} disabled={saving}>
          {i18n.t("settings.cancel")}
        </Button>
        <Button type="submit" variant="primary" disabled={saving}>
          {saving ? i18n.t("settings.saving", {}, "Saving...") : i18n.t("settings.saveChanges")}
        </Button>
      </div>
    </form>
  </div>
</div>
