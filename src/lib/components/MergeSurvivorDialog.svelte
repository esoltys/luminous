<script lang="ts">
  import { GitMerge, X } from "lucide-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    /** Every tag name involved — 2 for a suggestion/simple merge, more for a
     * bulk "Merge Selected" across several distinct names. The mock merges
     * straight into whichever name it picks; we ask the user instead. */
    names: string[];
    onConfirm: (survivor: string) => void;
    onCancel: () => void;
  }

  let { names, onConfirm, onCancel }: Props = $props();
  // svelte-ignore state_referenced_locally
  let survivor = $state(names[0]);
</script>

<Modal onClose={onCancel} maxWidth="max-w-sm">
  <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
    <div class="flex items-center gap-2">
      <GitMerge class="w-4 h-4 text-brand-accent-text" />
      <h3 class="text-sm font-bold">{i18n.t("songTags.mergeDialogTitle", {}, "Merge tags")}</h3>
    </div>
    <button onclick={onCancel} class="text-brand-text-secondary hover:text-brand-text-primary transition-colors">
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="p-6 flex flex-col gap-3">
    <p class="text-sm text-brand-text-secondary">
      {i18n.t("songTags.mergeDialogPrompt", {}, "Which name should be kept?")}
    </p>
    <div class="flex flex-col gap-1.5">
      {#each names as name (name)}
        <label class="flex items-center gap-2 px-3 py-2 rounded-lg border cursor-pointer transition-colors {survivor === name ? 'border-brand-accent bg-brand-accent/10' : 'border-brand-border hover:border-brand-accent/40'}">
          <input type="radio" name="merge-survivor" value={name} checked={survivor === name} onchange={() => { survivor = name; }} />
          <span class="text-sm font-medium text-brand-text-primary">{name}</span>
        </label>
      {/each}
    </div>
  </div>

  <div class="flex items-center justify-end gap-3 px-6 py-4 border-t border-brand-border bg-brand-main">
    <Button onclick={onCancel} variant="secondary" size="sm">
      {i18n.t("songTags.cancelBtn", {}, "Cancel")}
    </Button>
    <Button onclick={() => onConfirm(survivor)} variant="primary" size="sm">
      {i18n.t("songTags.mergeConfirm", {}, "Merge")}
    </Button>
  </div>
</Modal>
