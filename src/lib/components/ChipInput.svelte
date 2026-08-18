<script lang="ts">
  import { X } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { parseMultiValue, joinMultiValue } from "../utils/multiValue";

  interface Props {
    /** `; `-delimited value list, matching the shared multi-value storage
        format used by `songs.genre`/`artist`/`album_artist`/`composer`. */
    value?: string;
    id?: string;
    disabled?: boolean;
    placeholder?: string;
    /** Mirrors Input.svelte's `highlighted` — e.g. to flag a field just
        changed by an AcoustID lookup, until edited or re-looked-up. */
    highlighted?: boolean;
    class?: string;
    /** Fires on every draft keystroke, before a chip is committed — e.g. to
        clear a `highlighted` flag as soon as the user starts editing. */
    oninput?: () => void;
  }

  let {
    value = $bindable(""),
    id,
    disabled = false,
    placeholder,
    highlighted = false,
    class: className = "",
    oninput,
  }: Props = $props();

  let chips = $derived(parseMultiValue(value));
  let draft = $state("");

  function commitDraft() {
    // A single keystroke can contain more than one value (comma-separated
    // paste, or typed "Rock, Blues" then Enter) — split and add them all.
    const additions = parseMultiValue(draft.replace(/,/g, ";"));
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
    value = joinMultiValue(next);
    draft = "";
  }

  function removeChip(index: number) {
    if (disabled) return;
    value = joinMultiValue(chips.filter((_, i) => i !== index));
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
  class="flex flex-wrap items-center gap-1.5 border rounded-lg bg-brand-main px-2 py-1.5 min-h-9 focus-within:border-brand-accent transition-colors {highlighted ? 'border-brand-accent ring-1 ring-brand-accent/40' : 'border-brand-border'} {disabled ? 'opacity-50' : ''} {className}"
>
  {#each chips as chip, i (chip)}
    <span class="inline-flex items-center gap-1 pl-2 pr-1 py-0.5 rounded-full bg-brand-accent/15 text-brand-accent-text text-xs font-medium">
      {chip}
      {#if !disabled}
        <button
          type="button"
          onclick={() => removeChip(i)}
          class="text-brand-accent-text/70 hover:text-red-400 transition-colors"
          aria-label={i18n.t('chipInput.removeItem', { value: chip })}
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
    placeholder={chips.length === 0 ? (placeholder ?? "") : ""}
    onkeydown={handleKeydown}
    onblur={commitDraft}
    oninput={() => oninput?.()}
    class="flex-1 min-w-[80px] bg-transparent outline-none text-xs text-brand-text-primary placeholder:text-brand-text-secondary/50 py-0.5"
  />
</div>
