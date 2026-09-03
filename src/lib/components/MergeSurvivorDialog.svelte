<script lang="ts">
  import { GitMergeIcon as GitMerge, XIcon as X } from "phosphor-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { tagsStore } from "../stores/tags.svelte";

  interface Props {
    /** Every tag name involved — 2 for a suggestion/simple merge, more for a
     * bulk "Merge Selected" across several distinct names. The mock merges
     * straight into whichever name it picks; we ask the user instead. */
    names: string[];
    onConfirm: (survivor: string) => void;
    onCancel: () => void;
  }

  let { names, onConfirm, onCancel }: Props = $props();

  function songCount(name: string): number {
    return tagsStore.allTags.find((t) => t.name === name)?.song_count ?? 0;
  }

  // The name involved in the most songs is the least disruptive pick and
  // the one users expect to see first, so it leads both the ordering and
  // the default selection.
  let sortedNames = $derived([...names].sort((a, b) => songCount(b) - songCount(a)));
  // svelte-ignore state_referenced_locally
  let survivor = $state(sortedNames[0]);

  /** "Parent: Name" when curated as a sub-genre somewhere — same context
   * the merge-suggestion banner shows, so the choice isn't ambiguous when
   * two names share a label but live under different parents. */
  function nameLabel(name: string): string {
    const parent = tagsStore.hierarchy.find((g) => g.children.some((c) => c.name === name));
    return parent ? `${parent.name}: ${name}` : name;
  }
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
      {#each sortedNames as name (name)}
        <label class="flex items-center justify-between gap-2 px-3 py-2 rounded-lg border cursor-pointer transition-colors {survivor === name ? 'border-brand-accent bg-brand-accent/10' : 'border-brand-border hover:border-brand-accent/40'}">
          <span class="flex items-center gap-2 min-w-0">
            <input type="radio" name="merge-survivor" value={name} checked={survivor === name} onchange={() => { survivor = name; }} />
            <span class="text-sm font-medium text-brand-text-primary truncate">{nameLabel(name)}</span>
          </span>
          <span class="text-xs text-brand-text-secondary tabular-nums shrink-0">
            {i18n.t("songTags.songCount", { count: songCount(name) }, `${songCount(name)} songs`)}
          </span>
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
