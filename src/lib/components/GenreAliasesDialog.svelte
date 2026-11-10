<script lang="ts">
  import { ArrowRightIcon as ArrowRight, TrashIcon as Trash2, XIcon as X, GitMergeIcon as GitMerge } from "phosphor-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { toastStore } from "../stores/toast.svelte";

  interface Props {
    /** Alias spelling to prefill — "" when opened from the toolbar. */
    initialAlias?: string;
    onClose: () => void;
  }

  let { initialAlias = "", onClose }: Props = $props();

  // Prefill only; the dialog is remounted for each open.
  // svelte-ignore state_referenced_locally
  let alias = $state(initialAlias);
  let canonical = $state("");
  let busy = $state(false);

  // Load on open so the list is current even if the Genres tab's
  // "tags-changed" listener isn't mounted.
  $effect(() => {
    tagsStore.loadGenreAliases().catch((e) => console.error("Failed to load genre aliases:", e));
  });

  /** Genre suggestions for one field, leaving out what's typed in the other:
   * a genre can't be an alias of itself. */
  function suggestionsExcluding(other: string) {
    const skip = other.trim().toLowerCase();
    return tagsStore.allTags.filter((t) => t.name.toLowerCase() !== skip);
  }
  let aliasOptions = $derived(suggestionsExcluding(canonical));
  let canonicalOptions = $derived(suggestionsExcluding(alias));

  let canSubmit = $derived(alias.trim() !== "" && canonical.trim() !== "" && !busy);

  async function handleSubmit() {
    if (!canSubmit) return;
    busy = true;
    try {
      await tagsStore.setGenreAlias(alias.trim(), canonical.trim());
      alias = "";
      canonical = "";
    } catch (e) {
      toastStore.show(String(e), "error");
    } finally {
      busy = false;
    }
  }

  async function handleRemove(name: string) {
    try {
      await tagsStore.removeGenreAlias(name);
    } catch (e) {
      toastStore.show(String(e), "error");
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") handleSubmit();
    else if (e.key === "Escape") onClose();
  }

  function focusInput(node: HTMLInputElement) {
    node.focus();
  }
</script>

<Modal {onClose} maxWidth="max-w-lg">
  <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
    <div class="flex items-center gap-2">
      <GitMerge class="w-4 h-4 text-brand-accent-text" />
      <h3 class="text-sm font-bold">{i18n.t("songTags.aliasesTitle", {}, "Genre Aliases")}</h3>
    </div>
    <button
      onclick={onClose}
      aria-label={i18n.t("songTags.cancelBtn", {}, "Cancel")}
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors"
    >
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="p-6 flex flex-col gap-4">
    <p class="text-xs text-brand-text-secondary">
      {i18n.t("songTags.aliasesHint", {}, "Treat one genre spelling as another. Your files' tags stay unchanged.")}
    </p>

    <div class="flex items-end gap-2">
      <div class="flex flex-col gap-1.5 flex-1 min-w-0">
        <label for="genre-alias-name" class="text-xs font-medium text-brand-text-secondary">
          {i18n.t("songTags.aliasLabel", {}, "Alias")}
        </label>
        <input
          id="genre-alias-name"
          use:focusInput
          bind:value={alias}
          onkeydown={handleKeyDown}
          list="genre-alias-options"
          class="w-full bg-brand-sidebar border border-brand-border focus:border-brand-accent rounded-lg px-3 py-2 text-sm text-brand-text-primary focus:outline-none transition-colors"
        />
      </div>
      <ArrowRight class="w-4 h-4 mb-2.5 shrink-0 text-brand-text-secondary" />
      <div class="flex flex-col gap-1.5 flex-1 min-w-0">
        <label for="genre-alias-canonical" class="text-xs font-medium text-brand-text-secondary">
          {i18n.t("songTags.canonicalLabel", {}, "Treat as")}
        </label>
        <input
          id="genre-alias-canonical"
          bind:value={canonical}
          onkeydown={handleKeyDown}
          list="genre-canonical-options"
          class="w-full bg-brand-sidebar border border-brand-border focus:border-brand-accent rounded-lg px-3 py-2 text-sm text-brand-text-primary focus:outline-none transition-colors"
        />
      </div>
      <Button variant="primary" size="sm" disabled={!canSubmit} onclick={handleSubmit}>
        {i18n.t("songTags.addAliasBtn", {}, "Add")}
      </Button>
    </div>
    <datalist id="genre-alias-options">
      {#each aliasOptions as tag (tag.name)}
        <option value={tag.name}></option>
      {/each}
    </datalist>
    <datalist id="genre-canonical-options">
      {#each canonicalOptions as tag (tag.name)}
        <option value={tag.name}></option>
      {/each}
    </datalist>

    {#if tagsStore.genreAliases.length === 0}
      <p class="text-xs text-brand-text-secondary py-2">
        {i18n.t("songTags.aliasesEmpty", {}, "No aliases yet.")}
      </p>
    {:else}
      <ul class="flex flex-col divide-y divide-brand-border/40 max-h-64 overflow-y-auto pr-3">
        {#each tagsStore.genreAliases as entry (entry.alias)}
          <li class="flex items-center gap-2 py-2 text-sm">
            <span class="truncate text-brand-text-primary">{entry.alias}</span>
            <ArrowRight class="w-3.5 h-3.5 shrink-0 text-brand-text-secondary" />
            <span class="truncate text-brand-text-primary flex-1">{entry.canonical}</span>
            <span class="text-xs text-brand-text-secondary shrink-0">
              {i18n.plural("songTags.aliasSongCount", entry.song_count)}
            </span>
            <button
              onclick={() => handleRemove(entry.alias)}
              aria-label={i18n.t("songTags.removeAliasBtn", {}, "Remove alias")}
              class="text-brand-text-secondary hover:text-brand-text-primary transition-colors shrink-0"
            >
              <Trash2 class="w-4 h-4" />
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</Modal>
