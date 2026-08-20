<script lang="ts">
  import { Tag as TagIcon, X } from "lucide-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import ChipInput from "./ChipInput.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { parseMultiValue, joinMultiValue } from "../utils/multiValue";

  let { songId, songTitle, onClose }: { songId: number; songTitle?: string; onClose: () => void } = $props();

  let value = $state("");
  let loading = $state(true);
  let saving = $state(false);

  $effect(() => {
    (async () => {
      const tags = await tagsStore.getSongTags(songId);
      value = joinMultiValue(tags);
      loading = false;
    })();
  });

  async function handleSave() {
    saving = true;
    try {
      await tagsStore.setSongTags(songId, parseMultiValue(value));
      onClose();
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      const target = e.target as HTMLElement;
      if (target.tagName === "BUTTON") return;
      e.preventDefault();
      handleSave();
    }
  }
</script>

<Modal onClose={saving ? () => {} : onClose} onKeydown={handleKeydown} maxWidth="max-w-md">
  <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
    <div class="flex items-center gap-2 min-w-0">
      <TagIcon class="w-4 h-4 text-brand-accent-text" />
      <h3 class="text-sm font-bold truncate">
        {i18n.t("songTags.title", {}, "Edit Tags")}{songTitle ? ` — ${songTitle}` : ""}
      </h3>
    </div>
    <button onclick={onClose} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="p-6 space-y-2">
    {#if loading}
      <div class="h-9 rounded-lg bg-brand-main animate-pulse"></div>
    {:else}
      <ChipInput bind:value placeholder={i18n.t("songTags.placeholder", {}, "Add a tag and press Enter")} />
    {/if}
    <p class="text-xs text-brand-text-secondary">
      {i18n.t("songTags.hint", {}, "First tag is the main category; the rest are subgenres of it.")}
    </p>
  </div>

  <div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-brand-border bg-brand-main">
    <Button onclick={onClose} variant="secondary" size="sm" disabled={saving}>
      {i18n.t("common.cancel", {}, "Cancel")}
    </Button>
    <Button onclick={handleSave} variant="primary" size="sm" disabled={loading || saving}>
      {i18n.t("common.save", {}, "Save")}
    </Button>
  </div>
</Modal>
