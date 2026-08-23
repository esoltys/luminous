<script lang="ts">
  import { AlertTriangle, GripVertical } from "lucide-svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { GENRE_PALETTE_HUES, genreColorHsl } from "../utils/genrePalette";

  interface Props {
    onOpenMainTag: (tag: string) => void;
    onOpenGenreEdge: (root: string, child: string) => void;
    selectMode: boolean;
    selected: Set<string>;
    onToggleSelect: (name: string) => void;
    sortField?: "name" | "count";
    sortAsc?: boolean;
    /** Collapses cards down to compact header-only rows (mirrors the
     * Albums/Artists cards-vs-rows toggle). */
    compact?: boolean;
  }

  let {
    onOpenMainTag,
    onOpenGenreEdge,
    selectMode,
    selected,
    onToggleSelect,
    sortField = "name",
    sortAsc = true,
    compact = false,
  }: Props = $props();

  let colorPopoverFor = $state<string | null>(null);

  /** Sorts both the cards themselves and each card's own children — display
   * only, doesn't touch the persisted drag-reorder sort_order. */
  let sortedHierarchy = $derived.by(() => {
    const dir = sortAsc ? 1 : -1;
    const cmp = (a: { name: string; song_count: number }, b: { name: string; song_count: number }) =>
      sortField === "name" ? a.name.localeCompare(b.name) * dir : (a.song_count - b.song_count) * dir;
    return tagsStore.hierarchy
      .map((g) => ({ ...g, children: [...g.children].sort(cmp) }))
      .sort(cmp);
  });

  // Drag reparent/promote/reorder cannot use native HTML5 drag-and-drop —
  // Tauri intercepts OS-level drag-drop at the webview layer so `dragstart`
  // never fires in-page (see ChipInput.svelte). Mirrors its pointer-event
  // pattern instead.
  let draggedChip = $state<{ name: string; fromGroup: string } | null>(null);
  /** Whole-card drag (via the header's grip handle) — demotes a top-level
   * card into a sub-genre chip under whatever card it's dropped on. Shares
   * the same dropTarget tracking as chip drags since hit-testing doesn't
   * care which kind of drag is in progress. */
  let draggedCard = $state<string | null>(null);
  let dropTarget = $state<{ kind: "card" | "header" | "chip"; group: string; chip?: string } | null>(null);
  /** Cursor position while a drag is active, for the floating ghost label —
   * cleared alongside draggedChip/draggedCard on pointerup. */
  let pointerPos = $state<{ x: number; y: number } | null>(null);

  /** What the floating ghost shows: the dragged item's own label, colored
   * like its current card (falls back to the target card's own hue for a
   * promoted-from-nowhere case, though that shouldn't normally happen). */
  let ghostInfo = $derived.by(() => {
    if (draggedChip) {
      const group = tagsStore.hierarchy.find((g) => g.name === draggedChip!.fromGroup);
      return { label: draggedChip.name, colorIndex: group?.color_index ?? 0 };
    }
    if (draggedCard) {
      const group = tagsStore.hierarchy.find((g) => g.name === draggedCard);
      return { label: draggedCard, colorIndex: group?.color_index ?? 0 };
    }
    return null;
  });

  function handleChipPointerDown(e: PointerEvent, name: string, fromGroup: string) {
    if (selectMode) return;
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    draggedChip = { name, fromGroup };
    pointerPos = { x: e.clientX, y: e.clientY };
  }

  function handleCardPointerDown(e: PointerEvent, name: string) {
    if (selectMode) return;
    e.preventDefault();
    draggedCard = name;
    pointerPos = { x: e.clientX, y: e.clientY };
  }

  function handlePointerMove(e: PointerEvent) {
    if (!draggedChip && !draggedCard) return;
    pointerPos = { x: e.clientX, y: e.clientY };
    const el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    const chipEl = el?.closest<HTMLElement>("[data-chip-key]");
    const headerEl = el?.closest<HTMLElement>("[data-card-header]");
    const cardEl = el?.closest<HTMLElement>("[data-card-name]");
    if (chipEl) {
      dropTarget = { kind: "chip", group: chipEl.dataset.chipGroup!, chip: chipEl.dataset.chipKey! };
    } else if (headerEl) {
      dropTarget = { kind: "header", group: headerEl.dataset.cardHeader! };
    } else if (cardEl) {
      dropTarget = { kind: "card", group: cardEl.dataset.cardName! };
    } else {
      dropTarget = null;
    }
  }

  async function handlePointerUp() {
    const chip = draggedChip;
    const card = draggedCard;
    const target = dropTarget;
    draggedChip = null;
    draggedCard = null;
    dropTarget = null;
    pointerPos = null;

    if (card) {
      if (target && target.group !== card) {
        await tagsStore.demoteGroupToChild(card, target.group);
      }
      return;
    }

    if (!chip || !target) return;
    if (target.kind === "header" && target.group === chip.fromGroup) {
      await tagsStore.promoteTag(chip.name);
    } else if (target.kind === "card" && target.group !== chip.fromGroup) {
      await tagsStore.reparentTag(chip.name, target.group);
    } else if (target.kind === "chip" && target.group === chip.fromGroup && target.chip !== chip.name) {
      const group = tagsStore.hierarchy.find((g) => g.name === chip.fromGroup);
      const newIndex = group?.children.findIndex((c) => c.name === target.chip) ?? 0;
      await tagsStore.reorderTagInGroup(chip.name, newIndex);
    } else if (target.kind === "chip" && target.group !== chip.fromGroup) {
      await tagsStore.reparentTag(chip.name, target.group);
    }
  }
</script>

<svelte:window onpointermove={handlePointerMove} onpointerup={handlePointerUp} />

<div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
  {#each sortedHierarchy as group (group.name)}
    <div
      data-card-name={group.name}
      class="rounded-lg bg-brand-sidebar border overflow-hidden transition-[opacity,box-shadow,border-color,transform] {draggedCard === group.name ? 'opacity-40' : ''} {(dropTarget?.kind === 'card' || dropTarget?.kind === 'header') && dropTarget.group === group.name && (draggedChip || draggedCard) ? 'border-brand-accent ring-4 ring-brand-accent/50 scale-[1.02] bg-brand-accent/5' : 'border-brand-border/60'}"
    >
      <div
        data-card-header={group.name}
        class="flex items-center gap-2 px-3 py-2.5 transition-colors {dropTarget?.kind === 'header' && dropTarget.group === group.name ? 'bg-brand-accent/25' : ''}"
      >
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span
          onpointerdown={(e) => handleCardPointerDown(e, group.name)}
          class="shrink-0 touch-none {selectMode ? '' : 'cursor-grab active:cursor-grabbing'} text-brand-text-secondary/50 hover:text-brand-text-secondary"
          title={i18n.t("songTags.dragCardTooltip", {}, "Drag to make this a sub-genre of another card")}
        >
          <GripVertical class="w-3.5 h-3.5" />
        </span>
        <div class="relative shrink-0">
          <button
            type="button"
            onclick={() => { colorPopoverFor = colorPopoverFor === group.name ? null : group.name; }}
            class="w-4 h-4 rounded-full border border-black/10 shrink-0"
            style="background-color: {genreColorHsl(group.color_index)}"
            title={i18n.t("songTags.changeColorTooltip", {}, "Change color")}
            aria-label={i18n.t("songTags.changeColorTooltip", {}, "Change color")}
          ></button>
          {#if colorPopoverFor === group.name}
            <div class="absolute z-20 top-6 left-0 grid grid-cols-5 gap-1.5 p-2 rounded-lg bg-brand-main border border-brand-border shadow-2xl">
              {#each GENRE_PALETTE_HUES as _, i (i)}
                <button
                  type="button"
                  onclick={() => { tagsStore.setGroupColor(group.name, i); colorPopoverFor = null; }}
                  class="w-5 h-5 rounded-full border-2 {group.color_index === i ? 'border-brand-text-primary' : 'border-transparent'}"
                  style="background-color: {genreColorHsl(i)}"
                  aria-label={`${i}`}
                ></button>
              {/each}
            </div>
          {/if}
        </div>
        <button
          type="button"
          onclick={() => onOpenMainTag(group.name)}
          class="flex-1 min-w-0 flex items-center justify-between gap-2 text-left"
        >
          <span class="text-sm font-semibold text-brand-text-primary truncate">{group.name}</span>
          <span class="text-xs text-brand-text-secondary tabular-nums shrink-0">
            {i18n.t("songTags.songCount", { count: group.song_count }, `${group.song_count} songs`)}
          </span>
        </button>
      </div>

      <div class="px-3 pb-3 flex flex-wrap gap-1.5 min-h-9 {compact ? 'hidden' : ''}">
        {#if group.children.length === 0}
          <p class="text-xs text-brand-text-secondary/70 italic py-1">
            {i18n.t("songTags.noSubgenresYet", {}, "No sub-genres yet — drag a tag here")}
          </p>
        {/if}
        {#each group.children as child (child.name)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span
            data-chip-key={child.name}
            data-chip-group={group.name}
            onpointerdown={(e) => handleChipPointerDown(e, child.name, group.name)}
            onclick={() => { if (selectMode) onToggleSelect(child.name); }}
            class="inline-flex items-center gap-1 pl-2 pr-1.5 py-1 rounded-full border text-xs font-medium select-none touch-none transition-[opacity,box-shadow,transform] {selectMode ? 'cursor-pointer' : 'cursor-grab active:cursor-grabbing'} {draggedChip?.name === child.name ? 'opacity-40' : ''} {dropTarget?.kind === 'chip' && dropTarget.chip === child.name ? 'ring-4 ring-brand-accent scale-110' : selected.has(child.name) ? 'ring-2 ring-brand-accent' : ''}"
            style={`background-color: color-mix(in srgb, ${genreColorHsl(group.color_index)} 22%, transparent); border-color: color-mix(in srgb, ${genreColorHsl(group.color_index)} 45%, transparent); color: ${genreColorHsl(group.color_index)};`}
          >
            {#if selectMode}
              <input
                type="checkbox"
                checked={selected.has(child.name)}
                onchange={() => onToggleSelect(child.name)}
                class="w-3 h-3 pointer-events-none"
              />
            {/if}
            <button
              type="button"
              onclick={() => !selectMode && onOpenGenreEdge(group.name, child.name)}
              class="inline-flex items-baseline gap-1"
            >
              <span>{child.name}</span>
              <span class="text-[0.85em] opacity-70">{child.song_count}</span>
            </button>
            {#if child.is_conflict}
              <span title={i18n.t("songTags.conflictTooltip", { name: child.name }, `"${child.name}" is also a top-level genre elsewhere in your library`)}>
                <AlertTriangle class="w-3 h-3 text-amber-400" />
              </span>
            {/if}
          </span>
        {/each}
      </div>
    </div>
  {/each}
</div>

{#if ghostInfo && pointerPos}
  <div
    class="fixed z-50 pointer-events-none px-3 py-1.5 rounded-full border text-xs font-semibold shadow-2xl -translate-y-1/2"
    style={`left: ${pointerPos.x + 16}px; top: ${pointerPos.y}px; background-color: color-mix(in srgb, ${genreColorHsl(ghostInfo.colorIndex)} 40%, var(--color-brand-sidebar)); color: color-mix(in srgb, ${genreColorHsl(ghostInfo.colorIndex)} 90%, var(--color-brand-text-primary)); border-color: color-mix(in srgb, ${genreColorHsl(ghostInfo.colorIndex)} 60%, transparent);`}
  >
    {ghostInfo.label}
  </div>
{/if}
