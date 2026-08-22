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
    /** Existing values to autocomplete the draft against (e.g. every known
        genre/tag name) — shown as a dropdown of case-insensitive prefix
        matches, excluding values already added as chips. Omit for plain
        free-text input with no suggestions. */
    suggestions?: string[];
  }

  let {
    value = $bindable(""),
    id,
    disabled = false,
    placeholder,
    highlighted = false,
    class: className = "",
    oninput,
    suggestions,
  }: Props = $props();

  let chips = $derived(parseMultiValue(value));
  let draft = $state("");
  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);
  let showSuggestions = $state(false);

  let matchingSuggestions = $derived.by(() => {
    if (!suggestions || draft.trim() === "") return [];
    const query = draft.trim().toLowerCase();
    return suggestions
      .filter((s) => s.toLowerCase().includes(query) && !chips.some((c) => c.toLowerCase() === s.toLowerCase()))
      .slice(0, 8);
  });

  function pickSuggestion(name: string) {
    draft = name;
    commitDraft();
    showSuggestions = false;
  }

  function reorderChip(fromIndex: number, toIndex: number) {
    if (disabled || fromIndex === toIndex) return;
    const next = [...chips];
    const [moved] = next.splice(fromIndex, 1);
    next.splice(toIndex, 0, moved);
    value = joinMultiValue(next);
  }

  // Chip reordering deliberately does NOT use the native HTML5 drag-and-drop
  // API (dragstart/dragover/drop). Tauri's window has dragDropEnabled: true
  // (needed for OS file-drop-to-import elsewhere in the app), and per Tauri's
  // documented behavior that intercepts native OS drag-drop at the webview
  // level in a way that prevents HTML5 DnD events from ever firing in-page —
  // dragstart never dispatches, so no amount of dragover/drop handling here
  // would help. Plain pointer-event tracking sidesteps that entirely.
  function handleChipPointerDown(e: PointerEvent, index: number) {
    if (disabled) return;
    // Let the remove button's own click handler run normally instead of
    // hijacking it into a drag.
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    draggedIndex = index;
  }

  function handlePointerMove(e: PointerEvent) {
    if (draggedIndex === null) return;
    const target = (document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null)?.closest(
      "[data-chip-index]"
    );
    const index = target ? Number(target.getAttribute("data-chip-index")) : null;
    dragOverIndex = index !== null && !Number.isNaN(index) ? index : null;
  }

  function handlePointerUp() {
    if (draggedIndex !== null && dragOverIndex !== null) {
      reorderChip(draggedIndex, dragOverIndex);
    }
    draggedIndex = null;
    dragOverIndex = null;
  }

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

<svelte:window
  onpointermove={handlePointerMove}
  onpointerup={handlePointerUp}
/>

<div class="relative">
<div
  class="flex flex-wrap items-center gap-1.5 border rounded-lg bg-brand-main px-2 py-1.5 min-h-9 focus-within:border-brand-accent transition-colors {highlighted ? 'border-brand-accent ring-1 ring-brand-accent/40' : 'border-brand-border'} {disabled ? 'opacity-50' : ''} {className}"
>
  {#each chips as chip, i (chip)}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <span
      data-chip-index={i}
      onpointerdown={(e) => handleChipPointerDown(e, i)}
      title={i18n.t('chipInput.dragToReorder', {}, 'Drag to reorder')}
      class="inline-flex items-center gap-1 pl-2 pr-1 py-0.5 rounded-full bg-brand-accent/15 text-brand-text-primary border border-brand-accent/25 text-xs font-medium select-none touch-none transition-[opacity,box-shadow] {disabled ? '' : 'cursor-grab active:cursor-grabbing'} {draggedIndex === i ? 'opacity-40' : ''} {dragOverIndex === i && draggedIndex !== null && draggedIndex !== i ? 'ring-2 ring-brand-accent' : ''}"
    >
      {chip}
      {#if !disabled}
        <button
          type="button"
          onclick={() => removeChip(i)}
          class="text-brand-text-secondary hover:text-red-400 transition-colors"
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
    onfocus={() => { showSuggestions = true; }}
    onblur={() => { commitDraft(); showSuggestions = false; }}
    oninput={() => { showSuggestions = true; oninput?.(); }}
    class="flex-1 min-w-[80px] bg-transparent outline-none text-xs text-brand-text-primary placeholder:text-brand-text-secondary/50 py-0.5"
  />
</div>
{#if showSuggestions && matchingSuggestions.length > 0}
  <div class="absolute z-20 top-full mt-1 left-0 right-0 max-h-40 overflow-y-auto rounded-lg bg-brand-sidebar border border-brand-border shadow-2xl">
    {#each matchingSuggestions as name (name)}
      <button
        type="button"
        onmousedown={(e) => { e.preventDefault(); pickSuggestion(name); }}
        class="w-full text-left px-3 py-1.5 text-xs text-brand-text-primary hover:bg-brand-accent/15 transition-colors"
      >
        {name}
      </button>
    {/each}
  </div>
{/if}
</div>
