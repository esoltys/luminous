<script lang="ts">
  import { FolderPlusIcon as FolderPlus, XIcon as X } from "phosphor-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    initialTags?: string[];
    onConfirm: (groupName: string) => void;
    onCancel: () => void;
  }

  let { initialTags = [], onConfirm, onCancel }: Props = $props();

  let groupName = $state("");

  function handleSubmit() {
    const trimmed = groupName.trim();
    if (!trimmed) return;
    onConfirm(trimmed);
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      handleSubmit();
    } else if (e.key === "Escape") {
      onCancel();
    }
  }

  function focusInput(node: HTMLInputElement) {
    node.focus();
  }
</script>

<Modal onClose={onCancel} maxWidth="max-w-sm">
  <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
    <div class="flex items-center gap-2">
      <FolderPlus class="w-4 h-4 text-brand-accent-text" />
      <h3 class="text-sm font-bold">
        {initialTags.length > 0
          ? i18n.t("songTags.groupSelectedTitle", {}, "Group Selected Tags")
          : i18n.t("songTags.createGroupTitle", {}, "Create Tag Group")}
      </h3>
    </div>
    <button onclick={onCancel} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="p-6 flex flex-col gap-4">
    {#if initialTags.length > 0}
      <div class="flex flex-col gap-1.5">
        <span class="text-xs text-brand-text-secondary font-medium">
          {i18n.t("songTags.selectedCount", { count: initialTags.length }, `${initialTags.length} selected`)}:
        </span>
        <div class="flex flex-wrap gap-1 max-h-24 overflow-y-auto">
          {#each initialTags as tag (tag)}
            <span class="px-2 py-0.5 rounded-full text-xs bg-brand-sidebar border border-brand-border text-brand-text-primary">
              {tag}
            </span>
          {/each}
        </div>
      </div>
    {/if}

    <div class="flex flex-col gap-1.5">
      <label for="artist-group-name" class="text-xs font-medium text-brand-text-secondary">
        {i18n.t("songTags.createGroupPrompt", {}, "Group name")}
      </label>
      <input
        id="artist-group-name"
        use:focusInput
        bind:value={groupName}
        onkeydown={handleKeyDown}
        placeholder="e.g. Award-Winning"
        class="w-full bg-brand-sidebar border border-brand-border focus:border-brand-accent rounded-lg px-3 py-2 text-sm text-brand-text-primary focus:outline-none transition-colors"
      />
    </div>

    <div class="flex items-center justify-end gap-2 pt-2">
      <Button variant="secondary" size="sm" onclick={onCancel}>
        {i18n.t("songTags.cancelBtn", {}, "Cancel")}
      </Button>
      <Button
        variant="primary"
        size="sm"
        disabled={!groupName.trim()}
        onclick={handleSubmit}
      >
        {initialTags.length > 0
          ? i18n.t("songTags.groupSelected", {}, "Group Selected")
          : i18n.t("songTags.createGroupBtn", {}, "Create")}
      </Button>
    </div>
  </div>
</Modal>
