<script lang="ts">
  import { GitMergeIcon as GitMerge, XIcon as X } from "phosphor-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import type { TagDuplicateGroup } from "../types";

  interface Props {
    /** Groups of spellings that differ only in case, accents, spacing or punctuation. */
    groups: TagDuplicateGroup[];
    /** Merges each of `others` (the spellings of one group except `into`) into `into`. */
    onMerge: (into: string, others: string[]) => void | Promise<void>;
    onClose: () => void;
  }

  let { groups, onMerge, onClose }: Props = $props();

  /** The spelling picked to keep, by group. A group with no pick keeps its suggestion. */
  let picked = $state<Record<string, string>>({});
  /** The group being merged right now (its `suggested`), so only one merge runs at a time. */
  let mergingGroup = $state<string | null>(null);

  function target(group: TagDuplicateGroup): string {
    const choice = picked[group.suggested];
    return group.spellings.some((s) => s.name === choice) ? choice : group.suggested;
  }

  async function merge(group: TagDuplicateGroup) {
    const into = target(group);
    mergingGroup = group.suggested;
    try {
      await onMerge(
        into,
        group.spellings.map((s) => s.name).filter((name) => name !== into),
      );
    } finally {
      mergingGroup = null;
    }
  }

  // Merging the last group leaves nothing to review.
  $effect(() => {
    if (groups.length === 0) onClose();
  });
</script>

<Modal {onClose} maxWidth="max-w-lg">
  <div
    class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main"
  >
    <div class="flex items-center gap-2">
      <GitMerge class="w-4 h-4 text-brand-accent-text" />
      <h3 class="text-sm font-bold">
        {i18n.t("songTags.duplicatesTitle", {}, "Merge Duplicate Tags")}
      </h3>
    </div>
    <button
      type="button"
      onclick={onClose}
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors"
      aria-label={i18n.t("songTags.duplicatesDone", {}, "Done")}
    >
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="p-6 flex flex-col gap-4 max-h-[70vh] overflow-y-auto">
    <p class="text-xs text-brand-text-secondary">
      {i18n.t(
        "songTags.duplicatesIntro",
        {},
        "These artist tags differ only in spelling. Choose the spelling to keep for each group, then merge.",
      )}
    </p>

    {#each groups as group (group.suggested)}
      <fieldset
        class="rounded-lg bg-brand-sidebar border border-brand-border p-3 flex flex-col gap-2"
        data-testid="tag-duplicate-group"
      >
        {#each group.spellings as spelling (spelling.name)}
          <label class="flex items-center gap-2 text-sm text-brand-text-primary cursor-pointer">
            <input
              type="radio"
              name={`keep-${group.suggested}`}
              value={spelling.name}
              checked={target(group) === spelling.name}
              onchange={() => (picked[group.suggested] = spelling.name)}
              class="accent-brand-accent"
            />
            <span class="flex-1 min-w-0 truncate">{spelling.name}</span>
            <span class="text-xs text-brand-text-secondary tabular-nums shrink-0">
              {i18n.plural("songTags.artistCount", spelling.artist_count)}
            </span>
          </label>
        {/each}
        <div class="flex justify-end pt-1">
          <Button
            variant="primary"
            size="sm"
            disabled={mergingGroup !== null}
            onclick={() => merge(group)}
          >
            {i18n.t(
              "songTags.duplicatesMergeBtn",
              { name: target(group) },
              `Merge into "${target(group)}"`,
            )}
          </Button>
        </div>
      </fieldset>
    {/each}

    <div class="flex items-center justify-end pt-2">
      <Button variant="secondary" size="sm" onclick={onClose}>
        {i18n.t("songTags.duplicatesDone", {}, "Done")}
      </Button>
    </div>
  </div>
</Modal>
