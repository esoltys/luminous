<script lang="ts">
  import { XIcon as X, PlusIcon as Plus } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { parseMultiValue, joinMultiValue } from "../utils/multiValue";

  interface Props {
    /** `; `-delimited value list, matching the shared multi-value storage
        format used by `songs.genre`/`artist`/`album_artist`/`composer`. */
    value?: string;
    id?: string;
    disabled?: boolean;
    /** Placeholder for the add-value input. */
    placeholder?: string;
    class?: string;
  }

  let {
    value = $bindable(""),
    id,
    disabled = false,
    placeholder,
    class: className = "",
  }: Props = $props();

  let chips = $derived(parseMultiValue(value));
  let draft = $state("");

  function handleAdd() {
    const raw = draft.trim().replace(/^,+|,+$/g, "");
    if (!raw) return;
    const parts = raw.split(",").map((s) => s.trim()).filter(Boolean);
    let next = [...chips];
    for (const part of parts) {
      if (!next.some((c) => c.toLowerCase() === part.toLowerCase())) {
        next = [...next, part];
      }
    }
    value = joinMultiValue(next);
    draft = "";
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      // Must not bubble up to an enclosing modal's "Enter submits the form"
      // handler and save/close the editor out from under the user before
      // the chip even renders.
      e.preventDefault();
      e.stopPropagation();
      handleAdd();
    }
  }

  function handleRemove(index: number) {
    value = joinMultiValue(chips.filter((_, i) => i !== index));
  }
</script>

<div class="flex flex-col gap-2 {className}">
  {#if chips.length > 0}
    <div class="flex flex-wrap gap-1.5">
      {#each chips as chip, idx (chip)}
        <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25">
          <span>{chip}</span>
          <button
            type="button"
            onclick={() => handleRemove(idx)}
            disabled={disabled}
            class="hover:text-red-400 focus:outline-none cursor-pointer disabled:opacity-40"
            title={i18n.t("chipInput.removeItem", { value: chip })}
          >
            <X class="w-3 h-3" />
          </button>
        </span>
      {/each}
    </div>
  {/if}

  <div class="flex items-center gap-2">
    <input
      {id}
      type="text"
      bind:value={draft}
      onkeydown={handleKeydown}
      {disabled}
      {placeholder}
      class="flex-1 px-3 py-1.5 rounded-lg bg-brand-main/50 border border-brand-border text-brand-text-primary placeholder:text-brand-text-secondary/50 text-xs focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent transition-colors"
    />
    <button
      type="button"
      onclick={handleAdd}
      disabled={!draft.trim() || disabled}
      class="shrink-0 text-xs font-medium text-brand-text-primary hover:underline disabled:opacity-40 disabled:hover:no-underline flex items-center gap-1 cursor-pointer"
    >
      <Plus class="w-3.5 h-3.5" />
      {i18n.t("chipInput.add", {}, "Add")}
    </button>
  </div>
</div>
