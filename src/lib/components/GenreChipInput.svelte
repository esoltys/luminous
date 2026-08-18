<script lang="ts">
  import { X } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { parseGenres, joinGenres } from "../utils/genre";

  interface Props {
    /** `; `-delimited genre list, matching `songs.genre`'s on-disk/DB storage format. */
    value?: string;
    id?: string;
    disabled?: boolean;
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

  let chips = $derived(parseGenres(value));
  let draft = $state("");

  function commitDraft() {
    // A single keystroke can contain more than one genre (comma-separated
    // paste, or typed "Rock, Blues" then Enter) — split and add them all.
    const additions = parseGenres(draft.replace(/,/g, ";"));
    if (additions.length === 0) {
      draft = "";
      return;
    }

    const next = [...chips];
    for (const addition of additions) {
      if (!next.some((c) => c.toLowerCase() === addition.toLowerCase())) {
        next.push(addition);
      }
    }
    value = joinGenres(next);
    draft = "";
  }

  function removeChip(index: number) {
    if (disabled) return;
    value = joinGenres(chips.filter((_, i) => i !== index));
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      // Enter here commits a chip, it must not also bubble up to the
      // enclosing modal's "Enter submits the form" handler and save/close
      // the editor out from under the user before the chip even renders.
      e.preventDefault();
      e.stopPropagation();
      commitDraft();
    } else if (e.key === "Backspace" && draft === "" && chips.length > 0) {
      removeChip(chips.length - 1);
    }
  }
</script>

<div
  class="flex flex-wrap items-center gap-1.5 border rounded-lg bg-brand-main px-2 py-1.5 min-h-9 border-brand-border focus-within:border-brand-accent transition-colors {disabled ? 'opacity-50' : ''} {className}"
>
  {#each chips as chip, i (chip)}
    <span class="inline-flex items-center gap-1 pl-2 pr-1 py-0.5 rounded-full bg-brand-accent/15 text-brand-accent-text text-xs font-medium">
      {chip}
      {#if !disabled}
        <button
          type="button"
          onclick={() => removeChip(i)}
          class="text-brand-accent-text/70 hover:text-red-400 transition-colors"
          aria-label={i18n.t('genreChipInput.removeGenre', { genre: chip })}
        >
          <X class="w-3 h-3" />
        </button>
      {/if}
    </span>
  {/each}
  <input
    {id}
    bind:value={draft}
    {disabled}
    placeholder={chips.length === 0 ? (placeholder ?? i18n.t('genreChipInput.placeholder')) : ""}
    onkeydown={handleKeydown}
    onblur={commitDraft}
    class="flex-1 min-w-[80px] bg-transparent outline-none text-xs text-brand-text-primary placeholder:text-brand-text-secondary/50 py-0.5"
  />
</div>
